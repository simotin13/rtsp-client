extern crate base64;
use std::env;
use std::io::BufRead;
use std::io::{Write, BufReader, BufWriter};
use std::net::TcpStream;
use url::Url;
use std::collections::HashMap;
use std::process;

fn main() {
    const USER_AGENT: &str = "my-rtsp-client";

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage. rtsp-client <rtsp url>");
        std::process::exit(1);
    }

    let rtsp_url = &args[1];

    // check if url starts with rtsp:// or rtspt://
    if !rtsp_url.starts_with("rtsp://") && !rtsp_url.starts_with("rtspt://") {
        eprintln!("rtsp_url does not start with rtsp:// or rtspt://");
        std::process::exit(1);
    }

    let tmp = rtsp_url.split("://").collect::<Vec<&str>>()[1];
    if !tmp.contains("@") {
        eprintln!("rtsp_url does not contain @");
        std::process::exit(1);
    }
    let tmp = tmp.split("@").collect::<Vec<&str>>()[0];
    // check if url contains :
    if !tmp.contains(":") {
        eprintln!("rtsp_url does not contain : for username:password");
        std::process::exit(1);
    }

    // get username and password
    let pair = tmp.split(":").collect::<Vec<&str>>();
    let username = pair[0];
    let password = pair[1];

    // get url,port
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
    let mut c_seq = 1;
    let mut request = String::new();
    request += &format!("OPTIONS {} RTSP/1.0\r\n", rtsp_url);
    let mut req_headers: HashMap<String, String> = HashMap::new();
    req_headers.insert("CSeq".to_string(), c_seq.to_string());
    req_headers.insert("User-Agent".to_string(), USER_AGENT.to_string());
    for (key, value) in &req_headers {
        request += &format!("{}: {}\r\n", key, value);
    }
    request+= "\r\n";

    if let Err(e) = writer.write_all(request.as_bytes()) {
        eprintln!("failed to send OPTION request: {}", e);
        process::exit(1);
    }
    let _ = writer.flush();

    let mut reader = BufReader::new(&stream);
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

    // ====================================================
    // DESCRIBE
    // ====================================================
    c_seq += 1;
    let mut request = String::new();
    request += &format!("DESCRIBE {} RTSP/1.0\r\n", rtsp_url);

    let mut req_headers: HashMap<String, String> = HashMap::new();
    req_headers.insert("CSeq".to_string(), c_seq.to_string());
    req_headers.insert("User-Agent".to_string(), USER_AGENT.to_string());
    // basic authentication
    let auth = base64::encode(&format!("{}:{}", username, password));
    req_headers.insert("Authorization".to_string(), format!("Basic {}", auth));
    for (key, value) in &req_headers {
        request += &format!("{}: {}\r\n", key, value);
    }
    request+= "\r\n";
    if let Err(e) = writer.write_all(request.as_bytes()) {
        eprintln!("failed to send OPTION request: {}", e);
        process::exit(1);
    }
    let _ = writer.flush();

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

    // ====================================================
    // SETUP
    // ====================================================
    c_seq += 1;
    let mut request = String::new();
    request += &format!("SETUP {}/track1 RTSP/1.0\r\n", rtsp_url);

    let mut req_headers: HashMap<String, String> = HashMap::new();
    req_headers.insert("CSeq".to_string(), c_seq.to_string());
    req_headers.insert("User-Agent".to_string(), USER_AGENT.to_string());
    let auth = base64::encode(&format!("{}:{}", username, password));
    req_headers.insert("Authorization".to_string(), format!("Basic {}", auth));
    // TODO: port number should be dynamic
    req_headers.insert("Transport".to_string(), "RTP/AVP;unicast;client_port=56648-56649".to_string());
    for (key, value) in &req_headers {
        request += &format!("{}: {}\r\n", key, value);
    }
    request+= "\r\n";
    if let Err(e) = writer.write_all(request.as_bytes()) {
        eprintln!("failed to send OPTION request: {}", e);
        process::exit(1);
    }
    let _ = writer.flush();

    let mut session = String::new();
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(len) => {
                println!("line[{}]", line.trim_end_matches("\r\n").to_string());
                
                // check if session is in the response
                let pair = line.split(":").collect::<Vec<&str>>();
                if pair.len() == 2 {
                    if pair[0].trim() == "Session" {
                        session = pair[1].trim().to_string();
                        session = session.split(";").collect::<Vec<&str>>()[0].to_string();
                        println!("session:{}", session);
                    }
                }
        
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

    // ====================================================
    // SETUP track2
    // ====================================================
    c_seq += 1;
    let mut request = String::new();
    request += &format!("SETUP {}/track2 RTSP/1.0\r\n", rtsp_url);
    let mut req_headers: HashMap<String, String> = HashMap::new();
    req_headers.insert("CSeq".to_string(), c_seq.to_string());
    req_headers.insert("User-Agent".to_string(), USER_AGENT.to_string());
    let auth = base64::encode(&format!("{}:{}", username, password));
    req_headers.insert("Authorization".to_string(), format!("Basic {}", auth));
    req_headers.insert("Session".to_string(), session);

    // TODO: port number should be dynamic
    req_headers.insert("Transport".to_string(), "RTP/AVP;unicast;client_port=56648-56649".to_string());
    for (key, value) in &req_headers {
        request += &format!("{}: {}\r\n", key, value);
    }
    request+= "\r\n";
    if let Err(e) = writer.write_all(request.as_bytes()) {
        eprintln!("failed to send OPTION request: {}", e);
        process::exit(1);
    }
    let _ = writer.flush();

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