extern crate base64;
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
    username: String,
    password: String,
    c_seq: u32,
    session: String,
    stream: TcpStream,
}

impl RTSPClient {
    pub fn new(rtsp_url: String, client_port: u16) -> Result<RTSPClient, String> {
        // check if url starts with rtsp:// or rtspt://
        if !rtsp_url.starts_with("rtsp://") && !rtsp_url.starts_with("rtspt://") {
            return Err("URL must start with rtsp:// or rtspt://".to_string());
        }
        let tmp = rtsp_url.split("://").collect::<Vec<&str>>()[1];
        if !tmp.contains("@") {
            return Err("rtsp_url does not contain @".to_string());
        }
        let tmp = tmp.split("@").collect::<Vec<&str>>()[0];
        // check if url contains :
        if !tmp.contains(":") {
            return Err("rtsp_url does not contain : for username:password".to_string());
        }

        // get username and password
        let pair = tmp.split(":").collect::<Vec<&str>>();
        let username = pair[0];
        let password = pair[1];

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
            username: username.to_string(),
            password: password.to_string(),
            session: String::new(),
            c_seq: 0,
            stream: stream.try_clone().unwrap(),
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

    pub fn describe(&mut self) -> Result<(), String> {
        self.c_seq += 1;
        let mut request = String::new();
        request += &format!("DESCRIBE {} RTSP/1.0\r\n", self.rtsp_url);

        let mut req_headers: HashMap<String, String> = HashMap::new();
        req_headers.insert("CSeq".to_string(), self.c_seq.to_string());
        req_headers.insert("User-Agent".to_string(), self.user_agent.to_string());

        // basic authentication
        let auth = base64::encode(&format!("{}:{}", self.username, self.password));
        req_headers.insert("Authorization".to_string(), format!("Basic {}", auth));
        for (key, value) in &req_headers {
            request += &format!("{}: {}\r\n", key, value);
        }
        request+= "\r\n";

        let mut writer = BufWriter::new(&self.stream);
        if let Err(e) = writer.write_all(request.as_bytes()) {
            eprintln!("failed to send DESCRIBE request: {}", e);
            return Err(format!("failed to send DESCRIBE request: {}", e));
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

    pub fn setup_track1(&mut self) -> Result<(), String> {
        self.c_seq += 1;
        let mut request = String::new();
        request += &format!("SETUP {}/track1 RTSP/1.0\r\n", self.rtsp_url);

        let mut req_headers: HashMap<String, String> = HashMap::new();
        req_headers.insert("CSeq".to_string(), self.c_seq.to_string());
        req_headers.insert("User-Agent".to_string(), self.user_agent.to_string());
        let auth = base64::encode(&format!("{}:{}", self.username, self.password));
        req_headers.insert("Authorization".to_string(), format!("Basic {}", auth));

        // TODO: port number should be dynamic
        let transport = format!("RTP/AVP;unicast;client_port={}-{}", self.client_port, self.client_port+1);
        req_headers.insert("Transport".to_string(), transport);
        for (key, value) in &req_headers {
            request += &format!("{}: {}\r\n", key, value);
        }
        request+= "\r\n";

        let mut writer = BufWriter::new(&self.stream);
        if let Err(e) = writer.write_all(request.as_bytes()) {
            return Err(format!("failed to send SETUP request: {}", e));
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
                    return Err(format!("failed to read_line: {}", e));
                }
            }
        }

        Ok(())
    }

    pub fn setup_track2(&mut self) -> Result<(), String> {
        self.c_seq += 1;
        let mut request = String::new();
        request += &format!("SETUP {}/track2 RTSP/1.0\r\n", self.rtsp_url);
        let mut req_headers: HashMap<String, String> = HashMap::new();
        req_headers.insert("CSeq".to_string(), self.c_seq.to_string());
        req_headers.insert("User-Agent".to_string(), self.user_agent.to_string());
        let auth = base64::encode(&format!("{}:{}", self.username, self.password));
        req_headers.insert("Authorization".to_string(), format!("Basic {}", auth));
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
            return Err(format!("failed to send SETUP request: {}", e));
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

    pub fn play(&mut self) -> Result<(), String> {
        self.c_seq += 1;
        let mut request = String::new();
        request += &format!("PLAY {} RTSP/1.0\r\n", self.rtsp_url);
        let mut req_headers: HashMap<String, String> = HashMap::new();
        req_headers.insert("CSeq".to_string(), self.c_seq.to_string());
        req_headers.insert("User-Agent".to_string(), self.user_agent.to_string());
        let auth = base64::encode(&format!("{}:{}", self.username, self.password));
        req_headers.insert("Authorization".to_string(), format!("Basic {}", auth));
        req_headers.insert("Session".to_string(), self.session.clone());
        req_headers.insert("Range".to_string(), "npt=0.000-".to_string());
        for (key, value) in &req_headers {
            request += &format!("{}: {}\r\n", key, value);
        }
        request+= "\r\n";

        let mut writer = BufWriter::new(&self.stream);
        if let Err(e) = writer.write_all(request.as_bytes()) {
            return Err(format!("failed to send PLAY request: {}", e));
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