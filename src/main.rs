mod rtsp_client;
mod rtp;
mod h264;
mod mp4_writer;
mod player;

use std::process;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::fs::File;
use mp4_writer::Mp4Writer;

extern crate ctrlc;

fn try_init_mp4(sps: &[u8], pps: &[u8]) -> Option<Mp4Writer> {
    let (width, height) = h264::parse_sps_resolution(sps)?;
    println!("Video resolution: {}x{}", width, height);
    let file = File::create("output.mp4").ok()?;
    let mut writer = Mp4Writer::new(file, width, height);
    writer.write_header().ok()?;
    writer.set_sps_pps(sps.to_vec(), pps.to_vec());
    println!("MP4 recording started -> output.mp4");
    Some(writer)
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
            0 => {
                // Unspecified
            },
            rtp::NAL_UNIT_TYPE_NON_IDR => {
                if let Some(ref mut writer) = mp4 {
                    if let Err(e) = writer.write_sample(&payload, rtp_ts, false) {
                        eprintln!("mp4 write_sample error: {}", e);
                    }
                }
            },
            rtp::NAL_UNIT_TYPE_IDR => {
                if let Some(ref mut writer) = mp4 {
                    if let Err(e) = writer.write_sample(&payload, rtp_ts, true) {
                        eprintln!("mp4 write_sample error: {}", e);
                    }
                }
            },
            rtp::NAL_UNIT_TYPE_SEI => {
                // SEI は MP4 に書かない
            },
            rtp::NAL_UNIT_TYPE_SPS => {
                println!("SPS received");
                h264::decode_sps(&payload);
                sps_nal = Some(payload.clone());
                if mp4.is_none() {
                    if let (Some(sps), Some(pps)) = (&sps_nal, &pps_nal) {
                        mp4 = try_init_mp4(sps, pps);
                    }
                }
            },
            rtp::NAL_UNIT_TYPE_PPS => {
                println!("PPS received");
                pps_nal = Some(payload.clone());
                if mp4.is_none() {
                    if let (Some(sps), Some(pps)) = (&sps_nal, &pps_nal) {
                        mp4 = try_init_mp4(sps, pps);
                    }
                }
            },
            rtp::NAL_UNIT_TYPE_AUD => {},
            rtp::NAL_UNIT_TYPE_END_OF_SEQUENCE => {
                println!("End of Sequence");
            },
            rtp::NAL_UNIT_TYPE_END_OF_STREAM => {
                println!("End of Stream");
                break;
            },
            rtp::NAL_UNIT_TYPE_FILLER_DATA => {},
            rtp::NAL_UNIT_TYPE_SPS_EXT => {},
            rtp::NAL_UNIT_TYPE_PARTITION_A |
            rtp::NAL_UNIT_TYPE_PARTITION_B |
            rtp::NAL_UNIT_TYPE_PARTITION_C => {
                println!("Partition packet (type {})", nal_unit_type);
            },
            rtp::NAL_UNIT_TYPE_STAP_A => {
                println!("Aggregation packet (type {})", nal_unit_type);
                // STAP-A (Single-Time Aggregation Packet)

                if payload.len() < 2 {
                    eprintln!("Invalid STAP-A packet");
                    continue;
                }

                // NALU Size (2 bytes)
                let nalu_size = u16::from_be_bytes([payload[1], payload[2]]) as usize;
                if payload.len() < 2 + nalu_size {
                    eprintln!("Invalid STAP-A packet: NALU size exceeds payload");
                    continue;
                }

                // NALU Header (1 byte)
                let stap_a_nal_header = payload[3];
                let stap_a_nal_unit_type = stap_a_nal_header & 0x1F;
                println!("@@@@ NALU Size: {}, NALU Type: {}", nalu_size, stap_a_nal_unit_type);
            },
            25..=27 => {
                println!("Aggregation packet (type {})", nal_unit_type);
            },
            28 => {
                // FU-A (fragmentation unit without DON)
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
                    if let Some(ref mut writer) = mp4 {
                        if let Err(e) = writer.write_sample(&fragment_mp4_buf, fragment_dts, fragment_is_keyframe) {
                            eprintln!("mp4 write_sample error: {}", e);
                        }
                    }
                }
            },
            29 => {
                // FU-B (with DON) - 未対応
                println!("FU-B (type 29) not supported");
            },
            _ => {
                println!("Unknown NAL type: {}", nal_unit_type);
            }
        }
    }

    // MP4 ファイルを確定して書き出す
    if let Some(ref mut writer) = mp4 {
        let count = writer.sample_count();
        if count > 0 {
            match writer.finalize() {
                Ok(_) => println!("output.mp4 saved ({} samples)", count),
                Err(e) => eprintln!("Failed to finalize MP4: {}", e),
            }
        } else {
            println!("No samples recorded, output.mp4 not finalized.");
        }
    } else {
        println!("Recording was not started (no SPS/PPS received).");
    }

    println!("Shutting down...");
    rtsp_client.shutdown();
}