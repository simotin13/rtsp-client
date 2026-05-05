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
use crate::h264_recorder::H264Recorder;
use crate::nal::NalEvent;

extern crate ctrlc;

fn parse_stap_a<'a>(payload: &'a [u8]) -> Vec<&'a [u8]> {
    let mut nalus = Vec::new();

    if payload.len() < 3 {
        return nalus;
    }

    let mut offset = 1; // 先頭の STAP-A ヘッダバイトをスキップ

    while offset + 2 <= payload.len() {
        let size = u16::from_be_bytes([payload[offset], payload[offset + 1]]) as usize;
        offset += 2;

        if offset + size > payload.len() {
            eprintln!("Invalid STAP-A: size exceeds payload");
            break;
        }

        nalus.push(&payload[offset..offset + size]);
        offset += size;
    }

    nalus
}

/// アクセスユニットバッファに NAL ユニットを AVCC 形式で追加する。
fn au_push(au_buf: &mut Vec<u8>, nal: &[u8]) {
    au_buf.extend_from_slice(&(nal.len() as u32).to_be_bytes());
    au_buf.extend_from_slice(nal);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: rtsp-client <rtsp url>         # MP4録画");
        eprintln!("       rtsp-client --play <rtsp url>  # ストリーム表示");
        std::process::exit(1);
    }

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

    let mut recorder = H264Recorder::new();

    rtsp_client.play().expect("failed to send PLAY request");

    // SDP の sprop-parameter-sets から SPS/PPS を取得してレコーダに注入
    if let Some((sps, pps)) = rtsp_client.get_video_sprop_parameter_sets() {
        recorder.handle_event(NalEvent::Sps(&sps));
        recorder.handle_event(NalEvent::Pps(&pps));
    }

    // アクセスユニット蓄積バッファ
    // 同じ RTP タイムスタンプを持つすべての NAL を AVCC 形式で蓄積し、
    // タイムスタンプが変わるかマーカービットが立ったら 1 サンプルとして書き込む。
    let mut au_buf: Vec<u8> = Vec::new();
    let mut au_ts: Option<u32> = None;
    let mut au_is_key: bool = false;

    // FU-A 断片蓄積バッファ
    let mut fragment_buf: Vec<u8> = Vec::new();
    let mut fragment_is_key: bool = false;

    while running.load(Ordering::SeqCst) {
        let (header, payload) = match rtp_receiver.receive() {
            Ok((h, p)) => (h, p),
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => {
                eprintln!("RTP receive error: {:?}", e);
                continue;
            }
        };

        if payload.is_empty() {
            continue;
        }

        let rtp_ts = header.timestamp;
        let marker = header.marker;
        let nal_header = payload[0];
        let nal_unit_type = nal_header & 0x1F;
        println!("nal_unit_type: {}", nal_unit_type);

        // タイムスタンプが変わったら前のアクセスユニットをフラッシュ
        if au_ts.map_or(false, |t| t != rtp_ts) {
            if !au_buf.is_empty() {
                recorder.handle_event(NalEvent::Video {
                    data: &au_buf,
                    ts: au_ts.unwrap(),
                    is_key: au_is_key,
                });
            }
            au_buf.clear();
            au_ts = None;
            au_is_key = false;
        }

        match nal_unit_type {
            rtp::NAL_UNIT_TYPE_STAP_A => {
                for nalu in parse_stap_a(&payload) {
                    let nal_type = nalu[0] & 0x1F;
                    println!("  STAP-A contains NAL unit type: {}", nal_type);
                    match nal_type {
                        rtp::NAL_UNIT_TYPE_SPS => {
                            recorder.handle_event(NalEvent::Sps(nalu));
                        }
                        rtp::NAL_UNIT_TYPE_PPS => {
                            recorder.handle_event(NalEvent::Pps(nalu));
                        }
                        rtp::NAL_UNIT_TYPE_IDR | rtp::NAL_UNIT_TYPE_NON_IDR => {
                            au_ts = Some(rtp_ts);
                            au_push(&mut au_buf, nalu);
                            if nal_type == rtp::NAL_UNIT_TYPE_IDR {
                                au_is_key = true;
                            }
                        }
                        _ => {}
                    }
                }
            }

            rtp::NAL_UNIT_TYPE_FU_A => {
                if payload.len() < 2 {
                    continue;
                }
                let fu_header = payload[1];
                let start_bit = (fu_header >> 7) & 0x01;
                let end_bit = (fu_header >> 6) & 0x01;
                let fu_nal_unit_type = fu_header & 0x1F;
                let fu_nal_header = (nal_header & 0xE0) | fu_nal_unit_type;
                println!("fu_nal_unit_type: {}, start_bit: {}, end_bit: {}", fu_nal_unit_type, start_bit, end_bit);

                if start_bit == 1 {
                    fragment_buf.clear();
                    fragment_buf.push(fu_nal_header);
                    fragment_is_key = fu_nal_unit_type == rtp::NAL_UNIT_TYPE_IDR;
                }

                // start を見ていない断片は破棄する
                if !fragment_buf.is_empty() {
                    fragment_buf.extend_from_slice(&payload[2..]);
                }

                if end_bit == 1 && !fragment_buf.is_empty() {
                    au_ts = Some(rtp_ts);
                    au_push(&mut au_buf, &fragment_buf);
                    if fragment_is_key {
                        au_is_key = true;
                    }
                    fragment_buf.clear();
                }
            }

            rtp::NAL_UNIT_TYPE_SPS => {
                recorder.handle_event(NalEvent::Sps(&payload));
            }

            rtp::NAL_UNIT_TYPE_PPS => {
                recorder.handle_event(NalEvent::Pps(&payload));
            }

            rtp::NAL_UNIT_TYPE_IDR => {
                au_ts = Some(rtp_ts);
                au_push(&mut au_buf, &payload);
                au_is_key = true;
            }

            rtp::NAL_UNIT_TYPE_NON_IDR => {
                au_ts = Some(rtp_ts);
                au_push(&mut au_buf, &payload);
            }

            _ => {} // SEI, AUD, フィラー等は無視
        }

        // マーカービットが立っていればアクセスユニット完了 → フラッシュ
        if marker == 1 && !au_buf.is_empty() {
            recorder.handle_event(NalEvent::Video {
                data: &au_buf,
                ts: au_ts.unwrap(),
                is_key: au_is_key,
            });
            au_buf.clear();
            au_ts = None;
            au_is_key = false;
        }
    }

    // 未フラッシュのアクセスユニットを書き込む
    if !au_buf.is_empty() {
        if let Some(ts) = au_ts {
            recorder.handle_event(NalEvent::Video { data: &au_buf, ts, is_key: au_is_key });
        }
    }

    recorder.finalize();

    println!("Shutting down...");
    rtsp_client.shutdown();
}
