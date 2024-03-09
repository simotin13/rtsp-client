mod rtsp_client;
mod rtp;
use std::process;
use std::env;
use std::net::{UdpSocket};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage. rtsp-client <rtsp url>");
        std::process::exit(1);
    }

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
    let socket = UdpSocket::bind("0.0.0.0:56789").unwrap();
    //let socket = UdpSocket::bind("127.0.0.1:56789").unwrap();
    //let target = format!("{}:{}", rtsp_client.get_host(), rtsp_client.get_server_port());
    //println!("Connecting to {}", target);
    //socket.connect(target).expect("connect function failed");
    let mut buf = [0; 1500];
    loop {
        println!("receiving!!!");
        match socket.recv_from(&mut buf) {
          Ok((buf_size, src_addr)) => {
            println!("Received {} bytes from {}", buf_size, src_addr);
          },
          Err(e) => {
            println!("couldn't recieve request: {:?}", e);
          }
        }
      }
    /*
    //let mut count = 0;
    //let mut rtp_receiver = rtp::RTPReceiver::new(rtsp_client.get_client_port());
    loop {
        let (header, payload) = rtp_receiver.receive();
        println!("RTP Header: {:?}", header);
    }
    */

    //rtsp_client.shutdown();
}