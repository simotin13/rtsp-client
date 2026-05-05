mod rtsp_client;
mod rtp;
mod mp4_writer;
mod player;
mod h264_recorder;
mod nal;
mod h264;

use std::process;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::fs::File;
use crate::mp4_writer::Mp4Writer;
use crate::h264_recorder::H264Recorder;
use crate::nal::NalEvent;
use crate::h264::parse_sps_resolution;

extern crate ctrlc;

fn parse_single_nalu<'a>(nal_unit_type: u8, payload: &'a [u8], rtp_ts: u32,) -> Option<NalEvent<'a>> {
    if payload.is_empty() {
        return None;
    }

    match nal_unit_type {
        rtp::NAL_UNIT_TYPE_UNSPECIFIED => None,

        rtp::NAL_UNIT_TYPE_NON_IDR => Some(NalEvent::Video {
            data: payload,
            ts: rtp_ts,
            is_key: false,
        }),

        rtp::NAL_UNIT_TYPE_IDR => Some(NalEvent::Video {
            data: payload,
            ts: rtp_ts,
            is_key: true,
        }),

        rtp::NAL_UNIT_TYPE_SEI => Some(NalEvent::Sei),

        rtp::NAL_UNIT_TYPE_SPS => {
            Some(NalEvent::Sps(payload))
        }

        rtp::NAL_UNIT_TYPE_PPS => {
            Some(NalEvent::Pps(payload))
        }

        rtp::NAL_UNIT_TYPE_AUD => None,

        rtp::NAL_UNIT_TYPE_END_OF_SEQUENCE => {
            // 必要ならイベントにしてもいい
            None
        }

        rtp::NAL_UNIT_TYPE_END_OF_STREAM => {
            Some(NalEvent::End)
        }

        rtp::NAL_UNIT_TYPE_FILLER_DATA => None,

        rtp::NAL_UNIT_TYPE_SPS_EXT => None,

        rtp::NAL_UNIT_TYPE_PARTITION_A
        | rtp::NAL_UNIT_TYPE_PARTITION_B
        | rtp::NAL_UNIT_TYPE_PARTITION_C => {
            None
        }

        // STAP-Aはここでは扱わない（上位で分解）
        rtp::NAL_UNIT_TYPE_STAP_A => {
            None
        }

        // FU-Aもここでは扱わない（再構成後にここに来る）
        28 => {
            None
        }

        29 => {
            // FU-B 未対応
            None
        }

        _ => {
            None
        }
    }
}

fn parse_stap_a<'a>(payload: &'a [u8]) -> Vec<&'a [u8]> {
    let mut nalus = Vec::new();

    // STAP-Aは最低でも header(1) + size(2) 必要
    if payload.len() < 3 {
        return nalus;
    }

    // 1バイト目はSTAP-Aヘッダなのでスキップ
    let mut offset = 1;

    while offset + 2 <= payload.len() {
        // NALUサイズ（2バイト big endian）
        let size = u16::from_be_bytes([
            payload[offset],
            payload[offset + 1],
        ]) as usize;

        offset += 2;

        // サイズチェック
        if offset + size > payload.len() {
            eprintln!("Invalid STAP-A: size exceeds payload");
            break;
        }

        // NALU本体（そのまま借用）
        let nalu = &payload[offset..offset + size];
        nalus.push(nalu);

        offset += size;
    }

    nalus
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: rtsp-client <rtsp url>         # MP4録画");
        eprintln!("       rtsp-client --play <rtsp url>  # ストリーム表示");
        std::process::exit(1);
    }

    // --play モード
    if args[1] == "--play" {
        if args.len() < 3 {
            eprintln!("Usage: rtsp-client --play <rtsp url>");
            std::process::exit(1);
        }
        player::run_player(args[2].clone());
        return;
    }

    let rtp_receiver = rtp::RTPReceiver::new();
    let rtp_port = rtp_receiver.get_rtp_port();
    let rtcp_port = rtp_receiver.get_rtcp_port();
    println!("rtp_port:{}, rtcp_port:{}", rtp_port, rtcp_port);

    let rtsp_url = &args[1];
    let mut rtsp_client = match rtsp_client::RTSPClient::new(rtsp_url.to_string(), rtp_port) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to create RTSPClient: {}", e);
            process::exit(1);
        }
    };

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("\nCtrl+C received, stopping...");
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl+C handler");

    rtsp_client.options().expect("failed to send OPTIONS request");
    rtsp_client.describe().expect("failed to send DESCRIBE request");
    rtsp_client.setup_tracks().expect("failed to send SETUP request for tracks");
    rtsp_client.play().expect("failed to send PLAY request");

    // 録画状態
    let mut sps_nal: Option<Vec<u8>> = None;
    let mut pps_nal: Option<Vec<u8>> = None;
    let mut mp4: Option<Mp4Writer> = None;

    // FU-A 組み立てバッファ（デコーダ用: スタートコードあり、MP4用: なし）
    let mut fragment_dec_buf: Vec<u8> = Vec::new();
    let mut fragment_mp4_buf: Vec<u8> = Vec::new();
    let mut fragment_dts: u32 = 0;
    let mut fragment_is_keyframe: bool = false;
    let mut recorder = H264Recorder::new();

    while running.load(Ordering::SeqCst) {
        let (header, payload) = match rtp_receiver.receive() {
            Ok((h, p)) => (h, p),
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                continue;
            }
            Err(e) => {
                eprintln!("RTP receive error: {:?}", e);
                continue;
            }
        };

        if payload.is_empty() {
            continue;
        }

        let rtp_ts = header.timestamp;
        let nal_header = payload[0];
        let nal_unit_type = nal_header & 0x1F;

        match nal_unit_type {
            rtp::NAL_UNIT_TYPE_UNSPECIFIED => {
                println!("Received NAL unit with unspecified type, skipping");
            }
            rtp::NAL_UNIT_TYPE_STAP_A => {
                for nalu in parse_stap_a(&payload) {
                    let nal_type = nalu[0] & 0x1F;
                    if let Some(ev) = parse_single_nalu(nal_type, nalu, rtp_ts) {
                        recorder.handle_event(ev);
                    }
                }
            }
            rtp::NAL_UNIT_TYPE_STAP_B => {
                println!("Received STAP-B NAL unit, which is not supported in this implementation");
            }
            rtp::NAL_UNIT_TYPE_MTAP16 => {
                println!("Received MTAP16 NAL unit, which is not supported in this implementation");
            }
            rtp::NAL_UNIT_TYPE_MTAP24 => {
                println!("Received MTAP24 NAL unit, which is not supported in this implementation");
            }
            rtp::NAL_UNIT_TYPE_FU_A => {
                // FU-A (fragmentation unit without DON)
                println!("@@@@ Received FU-A NAL unit");
                if payload.len() < 2 {
                    continue;
                }
                let fu_header = payload[1];
                let start_bit      = (fu_header >> 7) & 0x01;
                let end_bit        = (fu_header >> 6) & 0x01;
                let fu_nal_unit_type = fu_header & 0x1F;
                let fu_nal_header   = (nal_header & 0xE0) | fu_nal_unit_type;

                if start_bit == 1 {
                    // デコーダ用（スタートコードあり）
                    fragment_dec_buf.clear();
                    fragment_dec_buf.extend_from_slice(&[0x00, 0x00, 0x01, fu_nal_header]);
                    // MP4 用（スタートコードなし）
                    fragment_mp4_buf.clear();
                    fragment_mp4_buf.push(fu_nal_header);

                    fragment_dts = rtp_ts;
                    fragment_is_keyframe = fu_nal_unit_type == rtp::NAL_UNIT_TYPE_IDR;
                }

                fragment_dec_buf.extend_from_slice(&payload[2..]);
                fragment_mp4_buf.extend_from_slice(&payload[2..]);

                if end_bit == 1 {
                    recorder.handle_event(NalEvent::Video {
                        data: &fragment_mp4_buf,
                        ts: fragment_dts,
                        is_key: fragment_is_keyframe,
                    });
                }
            }

            rtp::NAL_UNIT_TYPE_FU_B => {
                println!("Received FU-B NAL unit, which is not supported in this implementation");
            }
            _ => {
                if let Some(ev) = parse_single_nalu(nal_unit_type, &payload, rtp_ts) {
                    recorder.handle_event(ev);
                }
            }
        }
    }

    // MP4 ファイルを確定して書き出す
    recorder.finalize();

    println!("Shutting down...");
    rtsp_client.shutdown();
}