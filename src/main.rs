mod rtsp_client;
mod rtp;
mod h264;
use std::process;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use openh264::decoder::Decoder;

extern crate ctrlc;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage. rtsp-client <rtsp url>");
        std::process::exit(1);
    }

    // openh264 decoder
    let api = openh264::OpenH264API::from_source();
    let mut decoder = match Decoder::new(api) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("failed to create decoder: {}", e);
            process::exit(1);
        }
    };

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("stop requested...");
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl+C handler");

    let rtsp_url = &args[1];

    let mut rtsp_client = match rtsp_client::RTSPClient::new(rtsp_url.to_string(), 56789) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to create RTSPClient: {}", e);
            process::exit(1);
        }
    };

    // OPTIONS
    rtsp_client.options().expect("failed to send OPTIONS request");

    // DESCRIBE
    rtsp_client.describe().expect("failed to send DESCRIBE request");

    // SETUP track1
    rtsp_client.setup_track1().expect("failed to send SETUP request for track1");

    // SETUP track2
    rtsp_client.setup_track2().expect("failed to send SETUP request for track2");

    // PLAY
    rtsp_client.play().expect("failed to send PLAY request");

    // receive RTP
    let rtp_receiver = rtp::RTPReceiver::new(rtsp_client.get_client_port());
    let mut payload_with_start_code : Vec<u8> = Vec::new();
    let mut fragment_buffer : Vec<u8> = Vec::new();

    while running.load(Ordering::SeqCst) {
        let (header, payload) = rtp_receiver.receive();
        println!("RTP Header: {:?}", header);

        // make payload with start code for openH264 decoder
        payload_with_start_code.push(0x00);
        payload_with_start_code.push(0x00);
        payload_with_start_code.push(0x01);
        payload_with_start_code.extend(payload.clone());

        // check NAL header
        let nal_header = payload[0];

        // 1: paylod has error, 0: payload is correct
        let nal_forbidden_bit = 0x01 & (nal_header >> 7);
        println!("RTP NAL Forbidden Bit: {}", nal_forbidden_bit);

        // idc value means the importance of the NAL unit
        let nal_ref_idc = 0x03 & (nal_header >> 5);
        println!("RTP NAL Ref IDC: {}", nal_ref_idc);

        let nal_unit_type = nal_header & 0x1F;
        match nal_unit_type {
            0 => {
                println!("RTP NAL Type Unspecified: {}", nal_unit_type);
                continue;
            },
            rtp::NAL_UNIT_TYPE_NON_IDR => {
                println!("RTP NAL Type Non IDR: {}", nal_unit_type);
            },
            rtp::NAL_UNIT_TYPE_PARTITION_A => {
                println!("RTP NAL Type Partition A: {}", nal_unit_type);
            },
            rtp::NAL_UNIT_TYPE_PARTITION_B => {
                println!("RTP NAL Type Partition B: {}", nal_unit_type);
            },
            rtp::NAL_UNIT_TYPE_PARTITION_C => {
                println!("RTP NAL Type Partition C: {}", nal_unit_type);
            },
            rtp::NAL_UNIT_TYPE_IDR => {
                println!("RTP NAL Type IDR: {}", nal_unit_type);
                let idr = decoder.decode(&payload_with_start_code);
                match idr {
                    Ok(idr) => {
                        // TODO
                    },
                    Err(e) => {
                        eprintln!("******** failed to decode: {}", e);
                        // stop process
                        break;
                    }
                }
            },
            rtp::NAL_UNIT_TYPE_SEI => {
                println!("RTP NAL Type SEI: {}", nal_unit_type);
            },
            rtp::NAL_UNIT_TYPE_SPS => {
                println!("RTP NAL Type SPS: {}", nal_unit_type);
                let sps = decoder.decode(&payload_with_start_code);
                match sps {
                    Ok(sps) => {
                        println!("******** SPS: {:?}", sps);
                    },
                    Err(e) => {
                        eprintln!("******** failed to decode: {}", e);
                        // stop process
                        break;
                    }
                }
            },
            rtp::NAL_UNIT_TYPE_PPS => {
                println!("RTP NAL Type PPS: {}", nal_unit_type);
                // decode PPS
                let pps = decoder.decode(&payload_with_start_code);
                match pps {
                    Ok(pps) => {
                        println!("******** PPS: {:?}", pps);
                    },
                    Err(e) => {
                        eprintln!("******** failed to decode: {}", e);
                    }
                }
            },
            rtp::NAL_UNIT_TYPE_AUD => {
                println!("RTP NAL Type AUD: {}", nal_unit_type);
            },
            rtp::NAL_UNIT_TYPE_END_OF_SEQUENCE => {
                println!("RTP NAL Type End of Sequence: {}", nal_unit_type);
            },
            rtp::NAL_UNIT_TYPE_END_OF_STREAM => {
                println!("RTP NAL Type End of Stream: {}", nal_unit_type);
            },
            rtp::NAL_UNIT_TYPE_FILLER_DATA => {
                println!("RTP NAL Type Filler Data: {}", nal_unit_type);
            },
            rtp::NAL_UNIT_TYPE_SPS_EXT => {
                println!("RTP NAL Type SPS Ext: {}", nal_unit_type);
            },
            24..=27 => {
                println!("RTP NAL Type Aggregation Packet: {}", nal_unit_type);
            },
            28 => {
                // without DON
                println!("RTP NAL Type FU-A: {}", nal_unit_type);
                let fu_header = payload[1];
                let start_bit = (fu_header >> 7) & 0x01;
                let end_bit = (fu_header >> 6) & 0x01;
                let reserved = (fu_header >> 5) & 0x01;
                let fu_nal_unit_type = fu_header & 0x1F;
                println!("RTP FU Start Bit: {}, End bit:{}, Reserved Bit:{}, FU nal_unit_type:{}", start_bit, end_bit, reserved, fu_nal_unit_type);
                if start_bit == 1 {
                    fragment_buffer.clear();
                    // set start code for decode
                    fragment_buffer.push(0x00);
                    fragment_buffer.push(0x00);
                    fragment_buffer.push(0x01);
                    let mut fu_nal_header = nal_header & 0xE0;
                    fu_nal_header |= fu_nal_unit_type;
                    fragment_buffer.push(fu_nal_header);
                }
                fragment_buffer.extend_from_slice(&payload[2..]);
                if end_bit == 1 {
                    let yuv = decoder.decode(&fragment_buffer);
                    match yuv {
                        Ok(yuv) => {
                            // TODO
                        },
                        Err(e) => {
                            eprintln!("******** failed to decode: {}", e);
                        }
                    }
                }
            },
            29 => {
                // with DON
                println!("RTP NAL Type FU-B: {}", nal_unit_type);
                let fu_header = payload[1];
                let start_bit = (fu_header >> 7) & 0x01;
                let end_bit = (fu_header >> 6) & 0x01;
                let reserved = (fu_header >> 5) & 0x01;
                let fu_nal_unit_type = fu_header & 0x1F;
                println!("RTP FU Start Bit: {}, End bit:{}, Reserved Bit:{}, FU nal_unit_type:{}", start_bit, end_bit, reserved, fu_nal_unit_type);
            },
            _ => {
                println!("RTP NAL Type Unknown: {}", nal_unit_type);
            }
        }

        if nal_unit_type == rtp::NAL_UNIT_TYPE_IDR {
            let yuv = decoder.decode(&payload);
            match yuv {
                Ok(yuv) => {
                    println!("YUV: {:?}", yuv);
                },
                Err(e) => {
                    eprintln!("failed to decode: {}", e);
                }
            }
        }
    }

    println!("stop receiving...");
    rtsp_client.shutdown();
}