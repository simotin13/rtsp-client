use std::env;
use std::net::TcpStream;
use url::Url;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage. rtsp-client <rtsp url>");
        std::process::exit(1);
    }

    let rtsp_url = &args[1];

    let url = match Url::parse(rtsp_url) {
        Ok(url) => url,
        Err(_) => {
            eprintln!("invalid url:{}", rtsp_url);
            process::exit(1);
        }
    };

    let host = match url.host_str() {
        Some(host) => host.to_string(),
        None => {
            eprintln!("invalid url:{}", rtsp_url);
            process::exit(1);
        }
    };

    let mut port = 554;
    if let Some(p) = url.port() {
        port = p;
    }
    let stream = TcpStream::connect(&format!("{}:{}",host, port)).expect(&format!("failed to connect server {}:{}, url", host, port));

    println!("connected to rtsp server {}:{}", host, port);

    stream.shutdown(std::net::Shutdown::Both).unwrap();
}