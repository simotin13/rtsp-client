mod rtsp_client;
use std::process;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage. rtsp-client <rtsp url>");
        std::process::exit(1);
    }

    let rtsp_url = &args[1];

    let mut rtsp_client = match rtsp_client::RTSPClient::new(rtsp_url.to_string()) {
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

    rtsp_client.shutdown();
}