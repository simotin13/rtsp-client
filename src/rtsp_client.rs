extern crate base64;
use std::io::Read;
use std::io::Write;
use std::io::BufRead;
use std::io::{BufReader, BufWriter};
use std::net::TcpStream;
use url::Url;
use std::collections::HashMap;

pub struct RTSPClient {
    user_agent: &'static str,
    rtsp_url: String,
    host: String,
    port: u16,
    server_port: u16,
    client_port: u16,
    with_auth: bool,
    username: String,
    password: String,
    c_seq: u32,
    session: String,
    stream: TcpStream,
    base_url: String,
    tracks: Vec<Track>,
}

#[derive(Clone)]
struct Track {
    media: String,
    port: u16,
    proto: String,                 // RTP/AVP ...etc
    formats: Vec<String>,          // 96, 97 ...etc
    connection: Option<String>,    // c=
    bandwidth: Vec<String>,        // b=
    attributes: Vec<(String,String)>, // a=key:value
}

impl RTSPClient {
    pub fn new(rtsp_url: String, client_port: u16) -> Result<RTSPClient, String> {
        // check if url starts with rtsp:// or rtspt://
        if !rtsp_url.starts_with("rtsp://") && !rtsp_url.starts_with("rtspt://") {
            return Err("URL must start with rtsp:// or rtspt://".to_string());
        }

        let mut with_auth = false;
        let mut username = "";
        let mut password = "";
        let tmp = rtsp_url.split("://").collect::<Vec<&str>>()[1];
        if tmp.contains("@") {
            with_auth = true;

            let tmp = tmp.split("@").collect::<Vec<&str>>()[0];
            // check if url contains :
            if !tmp.contains(":") {
                return Err("rtsp_url does not contain : for username:password".to_string());
            }

            // get username and password
            let pair = tmp.split(":").collect::<Vec<&str>>();
            if pair.len() != 2 {
                return Err("rtsp_url does not contain valid username:password".to_string());
            }
            username = pair[0];
            password = pair[1];
        }

        // get url,port
        let url = match Url::parse(&rtsp_url) {
            Ok(u) => u,
            Err(e) => return Err(format!("failed to parse url: {}", e)),
        };

        let host = match url.host_str() {
            Some(h) => h,
            None => return Err(format!("failed to parse url: {}", url)),
        };

        let mut port = 554;
        if let Some(p) = url.port() {
            port = p;
        };

        // connect to RTSP server, if failed, return Err
        let stream = match TcpStream::connect(&format!("{}:{}", host, port)) {
            Ok(s) => s,
            Err(_) => return Err(format!("failed to connect to {}:{}", host, port)),
        };
    
        Ok(RTSPClient {
            user_agent: "my-rtsp-client",
            rtsp_url: rtsp_url.to_string(),
            host: host.to_string(),
            port: port,
            server_port: 0,
            client_port: client_port,
            with_auth: with_auth,
            username: username.to_string(),
            password: password.to_string(),
            session: String::new(),
            c_seq: 0,
            stream: stream.try_clone().unwrap(),
            base_url: String::new(),
            tracks: Vec::new(),
        })
    }

    pub fn options(&mut self) -> Result<(), String> {
        self.c_seq += 1;
        let mut request = String::new();
        request += &format!("OPTIONS {} RTSP/1.0\r\n", self.rtsp_url);
        let mut req_headers: HashMap<String, String> = HashMap::new();
        req_headers.insert("CSeq".to_string(), self.c_seq.to_string());
        req_headers.insert("User-Agent".to_string(), self.user_agent.to_string());
        for (key, value) in &req_headers {
            request += &format!("{}: {}\r\n", key, value);
        }
        request+= "\r\n";
   
        let mut writer = BufWriter::new(&self.stream);
        if let Err(e) = writer.write_all(request.as_bytes()) {
            return Err(format!("failed to send OPTIONS request: {}", e));
        }
        let _ = writer.flush();

        let mut reader = BufReader::new(&self.stream);
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
                    return Err(format!("failed to read_line: {}", e));
                }
            }
        }

        Ok(())
    }

    pub fn describe(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.c_seq += 1;
        let mut request = String::new();
        request += &format!("DESCRIBE {} RTSP/1.0\r\n", self.rtsp_url);

        let mut req_headers: HashMap<String, String> = HashMap::new();
        req_headers.insert("CSeq".to_string(), self.c_seq.to_string());
        req_headers.insert("User-Agent".to_string(), self.user_agent.to_string());

        // basic authentication
        if self.with_auth {
            let auth = base64::encode(&format!("{}:{}", self.username, self.password));
            req_headers.insert("Authorization".to_string(), format!("Basic {}", auth));
        }
        for (key, value) in &req_headers {
            request += &format!("{}: {}\r\n", key, value);
        }
        request+= "\r\n";

        let mut writer = BufWriter::new(&self.stream);
        if let Err(e) = writer.write_all(request.as_bytes()) {
            eprintln!("failed to send DESCRIBE request: {}", e);
            return Err(format!("failed to send DESCRIBE request: {}", e).into());
        }
        let _ = writer.flush();

        let mut reader = BufReader::new(&self.stream);
        let mut headers = HashMap::new();

        // read status line
        let mut status_line = String::new();
        reader.read_line(&mut status_line)?;
        let parts: Vec<&str> = status_line.split_whitespace().collect();
        let status_code = parts.get(1).and_then(|s| s.parse::<u16>().ok());
        if status_code != Some(200) {
            return Err(format!("DESCRIBE request failed with status code: {:?}", status_code).into());
        }

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
                    if let Some((key, value)) = line.split_once(":") {
                        headers.insert(key.trim().to_string(), value.trim().to_string());
                    }
                },
                Err(e) => {
                    return Err(format!("failed to read_line: {}", e).into());
                }
            }
        }

        // Content-Base
        if let Some(content_base) = headers.get("Content-Base") {
            self.base_url = content_base.to_string();
        } else {
            self.base_url = self.rtsp_url.clone();
        }
        println!("base_url: {}", self.base_url);

        // Content-Length
        let content_length = headers.get("Content-Length").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        let mut body = vec![0; content_length];
        reader.read_exact(&mut body)?;
        let sdp = String::from_utf8_lossy(&body);
        println!("SDP:\n{}", sdp);

        self.tracks.clear();
        let mut current: Option<Track> = None;
        for line in sdp.lines() {
            if line.starts_with("m=") {
                // 前のトラックを確定
                if let Some(t) = current.take() {
                    self.tracks.push(t);
                }

                // create new track
                let parts: Vec<&str> = line.split_whitespace().collect();
                current = Some(Track {
                    media: parts[0].trim_start_matches("m=").to_string(),
                    port: parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(0),
                    proto: parts.get(2).unwrap_or(&"").to_string(),
                    formats: parts.iter().skip(3).map(|s| s.to_string()).collect(),
                    connection: None,
                    bandwidth: vec![],
                    attributes: vec![],
                });

                continue;
            }

            if let Some(t) = current.as_mut() {
                if line.starts_with("c=") {
                    t.connection = Some(line.trim_start_matches("c=").to_string());
                } else if line.starts_with("b=") {
                    t.bandwidth.push(line.trim_start_matches("b=").to_string());
                } else if line.starts_with("a=") {
                    if let Some((key, value)) = line.trim_start_matches("a=").split_once(":") {
                        t.attributes.push((key.to_string(), value.to_string()));
                    }
                }
            }
        }
        // 確定していないトラックを追加
        if let Some(t) = current.take() {
            self.tracks.push(t);
        }

        // dump tracks
        for (i, t) in self.tracks.iter().enumerate() {
            println!("Track {}:", i);
            println!("  media: {}", t.media);
            println!("  port: {}", t.port);
            println!("  proto: {}", t.proto);
            println!("  formats: {:?}", t.formats);
            println!("  connection: {:?}", t.connection);
            println!("  bandwidth: {:?}", t.bandwidth);
            println!("  attributes:");
            for (key, value) in &t.attributes {
                println!("    {}: {}", key, value);
            }
        }

        Ok(())
    }

    pub fn setup_tracks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let tracks = self.tracks.clone();

        for track in tracks {
            if track.media == "video" {
                self.setup_track1()?;
            } else if track.media == "audio" {
                self.setup_track2()?;
            }
        }
        Ok(())
    }

    pub fn setup_track1(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.c_seq += 1;
        let mut request = String::new();

        // video trackを取得
        let video_track = self.tracks.iter().find(|t| t.media == "video");
        let track = video_track.ok_or("no video track found")?;

        // attributesからcontrolを取得
        let control_attr = track.attributes.iter().find(|(k, _)| k == "control");
        let control = control_attr.ok_or("no control attribute found in video track")?.1.clone();
        let setup_url = if control.starts_with("rtsp://") {
            control.clone()
        } else {
            format!("{}{}", self.base_url, control)
        };
        println!("setup_url:\n{}", setup_url);
        request += &format!("SETUP {} RTSP/1.0\r\n", setup_url);

        let mut req_headers: HashMap<String, String> = HashMap::new();
        req_headers.insert("CSeq".to_string(), self.c_seq.to_string());
        req_headers.insert("User-Agent".to_string(), self.user_agent.to_string());
        if self.with_auth {
            let auth = base64::encode(&format!("{}:{}", self.username, self.password));
            req_headers.insert("Authorization".to_string(), format!("Basic {}", auth));
        }

        let transport = format!("RTP/AVP;unicast;client_port={}-{}", self.client_port, self.client_port+1);
        req_headers.insert("Transport".to_string(), transport);
        for (key, value) in &req_headers {
            request += &format!("{}: {}\r\n", key, value);
        }
        request+= "\r\n";
        println!("SETUP request:\n{}", request);

        let mut writer = BufWriter::new(&self.stream);
        if let Err(e) = writer.write_all(request.as_bytes()) {
            return Err(format!("failed to send SETUP request: {}", e).into());
        }
        let _ = writer.flush();

        let mut reader = BufReader::new(&self.stream);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(len) => {
                    println!("line[{}]", line.trim_end_matches("\r\n").to_string());
                    
                    // check if session is in the response
                    let pair = line.split(":").collect::<Vec<&str>>();
                    if pair.len() == 2 {
                        if pair[0].trim() == "Session" {
                            let tmp = pair[1].trim().to_string();
                            let session = tmp.split(";").collect::<Vec<&str>>()[0].to_string();
                            self.session = session;
                            println!("session:{}", self.session);
                        }

                        if pair[0].trim() == "Transport" {
                            let tmp = pair[1].trim().to_string();
                            let pair = tmp.split(";").collect::<Vec<&str>>();
                            for p in pair {
                                if p.contains("server_port") {
                                    let tmp = p.split("=").collect::<Vec<&str>>()[1];
                                    let server_port = tmp.split("-").collect::<Vec<&str>>()[0].parse::<u16>().unwrap();
                                    self.server_port = server_port;
                                }
                            }
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
                    return Err(format!("failed to read_line: {}", e).into());
                }
            }
        }

        Ok(())
    }

    pub fn setup_track2(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.c_seq += 1;
        let mut request = String::new();
        request += &format!("SETUP {}/track2 RTSP/1.0\r\n", self.rtsp_url);
        let mut req_headers: HashMap<String, String> = HashMap::new();
        req_headers.insert("CSeq".to_string(), self.c_seq.to_string());
        req_headers.insert("User-Agent".to_string(), self.user_agent.to_string());
        if self.with_auth {
            let auth = base64::encode(&format!("{}:{}", self.username, self.password));
            req_headers.insert("Authorization".to_string(), format!("Basic {}", auth));
        }
        req_headers.insert("Session".to_string(), self.session.clone());
    
        // TODO: port number should be dynamic
        let transport = format!("RTP/AVP;unicast;client_port={}-{}", self.client_port, self.client_port+1);
        req_headers.insert("Transport".to_string(), transport);
        for (key, value) in &req_headers {
            request += &format!("{}: {}\r\n", key, value);
        }
        request+= "\r\n";

        let mut writer = BufWriter::new(&self.stream);
        if let Err(e) = writer.write_all(request.as_bytes()) {
            return Err(format!("failed to send SETUP request: {}", e).into());
        }
        let _ = writer.flush();

        let mut reader = BufReader::new(&self.stream);
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
                    return Err(format!("failed to read_line: {}", e).into());
                }
            }
        }

        Ok(())
    }

    pub fn play(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.c_seq += 1;
        let mut request = String::new();
        request += &format!("PLAY {} RTSP/1.0\r\n", self.rtsp_url);
        let mut req_headers: HashMap<String, String> = HashMap::new();
        req_headers.insert("CSeq".to_string(), self.c_seq.to_string());
        req_headers.insert("User-Agent".to_string(), self.user_agent.to_string());
        if self.with_auth {
            let auth = base64::encode(&format!("{}:{}", self.username, self.password));
            req_headers.insert("Authorization".to_string(), format!("Basic {}", auth));
        }
        req_headers.insert("Session".to_string(), self.session.clone());
        req_headers.insert("Range".to_string(), "npt=0.000-".to_string());
        for (key, value) in &req_headers {
            request += &format!("{}: {}\r\n", key, value);
        }
        request+= "\r\n";

        let mut writer = BufWriter::new(&self.stream);
        if let Err(e) = writer.write_all(request.as_bytes()) {
            return Err(format!("failed to send PLAY request: {}", e).into());
        }
        let _ = writer.flush();

        let mut reader = BufReader::new(&self.stream);
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
                    return Err(format!("failed to read_line: {}", e).into());
                }
            }
        }
        Ok(())
    }

    pub fn shutdown(&mut self) {
        self.stream.shutdown(std::net::Shutdown::Both).unwrap();
    }

    pub fn get_host(&self) -> String {
        self.host.clone()
    }

    pub fn get_port(&self) -> u16 {
        self.port
    }

    pub fn get_server_port(&self) -> u16 {
        self.server_port
    }

    pub fn get_client_port(&self) -> u16 {
        self.client_port
    }
}