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
                println!("@@@@@@@@@@@@ Received SPS");
                self.sps = Some(sps.to_vec());
                if let (Some(ref sps), Some(ref pps)) = (&self.sps, &self.pps) {
                    if self.mp4.is_none() {
                        self.mp4 = Self::try_init(sps, pps);
                    }
                }
            }

            NalEvent::Pps(pps) => {
                println!("@@@@@@@@@@@@ Received PPS");
                self.pps = Some(pps.to_vec());
                if let (Some(ref sps), Some(ref pps)) = (&self.sps, &self.pps) {
                    if self.mp4.is_none() {
                        self.mp4 = Self::try_init(sps, pps);
                    }
                }
            }

            NalEvent::Video { data, ts, is_key } => {
                if let Some(ref mut writer) = self.mp4 {
                    println!("*********** Writing video sample: ts={}, is_key={}", ts, is_key);
                    let _ = writer.write_sample(data, ts, is_key);
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