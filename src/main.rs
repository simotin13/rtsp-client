use std::env;
use std::io::BufRead;
use std::io::{Write, BufReader, BufWriter};
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
    let mut writer = BufWriter::new(&stream);

    println!("connected to rtsp server {}:{}", host, port);
    // send OPTIONS
    let c_seq = 1;
    let mut request = String::new();
    request += "OPTIONS rtsp://192.168.1.39:554/stream1 RTSP/1.0\r\n";
    request += &format!("CSeq: {}\r\n", c_seq);
    request+= "User-Agent: my-rtsp-client\r\n";
    request+= "\r\n";
    if let Err(e) = writer.write_all(request.as_bytes()) {
        eprintln!("failed to send OPTION request: {}", e);
        process::exit(1);
    }
    let _ = writer.flush();

    let mut reader = BufReader::new(&stream);
    let lines:Vec<String> = Vec::new();
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(len) => {
                println!("line[{}]", line.trim_end_matches("\r\n").to_string());
                if len == 0 {
                    break;
                }
                if line == "\r\n" {
                    break;
                }
            },
            Err(e) => {
                eprintln!("failed to read_line {}", e);
                break;
            }
        }
    }
    for line in &lines {
        println!("{}", line);
    }
    stream.shutdown(std::net::Shutdown::Both).unwrap();
}