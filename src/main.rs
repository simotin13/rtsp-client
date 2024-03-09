mod rtsp_client;
mod rtp;
use std::process;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

extern crate ctrlc;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage. rtsp-client <rtsp url>");
        std::process::exit(1);
    }

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
    let mut rtp_receiver = rtp::RTPReceiver::new(rtsp_client.get_client_port());
    while running.load(Ordering::SeqCst) {
        let (header, payload) = rtp_receiver.receive();
        println!("RTP Header: {:?}", header);
    }

    println!("stop receiving...");
    rtsp_client.shutdown();
}