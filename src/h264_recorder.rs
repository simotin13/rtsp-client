use std::fs::File;
use crate::nal::NalEvent;
use crate::mp4_writer::Mp4Writer;
use crate::h264;

pub struct H264Recorder {
    mp4: Option<Mp4Writer>,
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
}

impl H264Recorder {
    fn try_init(sps: &[u8], pps: &[u8]) -> Option<Mp4Writer> {
        println!("try_init In...");
        let (width, height) = h264::parse_sps_resolution(sps)?;
        println!("*********** Video resolution: {}x{}", width, height);
        let file = File::create("output.mp4").ok()?;
        let mut writer = Mp4Writer::new(file, width, height);
        writer.write_header().ok()?;
        writer.set_sps_pps(sps.to_vec(), pps.to_vec());
        println!("*********** MP4 recording started -> output.mp4");
        Some(writer)
    }

    pub fn new() -> Self {
        Self {
            mp4: None,
            sps: None,
            pps: None,
        }
    }

    pub fn handle_event(&mut self, ev: NalEvent) {
        match ev {
            NalEvent::Sps(sps) => {
                self.sps = Some(sps.to_vec());
                if self.mp4.is_some() {
                    // mp4 初期化済み: avcC 用 SPS を最新の in-band SPS に更新
                    if let Some(ref pps) = self.pps {
                        let pps = pps.clone();
                        if let Some(ref mut writer) = self.mp4 {
                            writer.set_sps_pps(sps.to_vec(), pps);
                        }
                    }
                } else if let (Some(ref s), Some(ref p)) = (&self.sps, &self.pps) {
                    self.mp4 = Self::try_init(s, p);
                }
            }

            NalEvent::Pps(pps) => {
                self.pps = Some(pps.to_vec());
                if self.mp4.is_some() {
                    // mp4 初期化済み: avcC 用 PPS を最新の in-band PPS に更新
                    if let Some(ref sps) = self.sps {
                        let sps = sps.clone();
                        if let Some(ref mut writer) = self.mp4 {
                            writer.set_sps_pps(sps, pps.to_vec());
                        }
                    }
                } else if let (Some(ref s), Some(ref p)) = (&self.sps, &self.pps) {
                    self.mp4 = Self::try_init(s, p);
                }
            }

            NalEvent::Video { data, ts, is_key } => {
                if let Some(ref mut writer) = self.mp4 {
                    // IDRが来るまでは書き込まない（最初の数フレームが灰色になるのを防ぐため）
                    if is_key || writer.sample_count() > 0 {
                        let _ = writer.write_sample(data, ts, is_key);
                    }
                }
            }

            _ => {}
        }
    }

    pub fn finalize(&mut self) {
        if let Some(ref mut writer) = self.mp4 {
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
            println!("No samples recorded, output.mp4 not finalized.");
        }
    }
}