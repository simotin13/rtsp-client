use std::net::{UdpSocket};
use std::time::Duration;
use std::io;
pub const NAL_UNIT_TYPE_NON_IDR: u8 = 1;
pub const NAL_UNIT_TYPE_PARTITION_A: u8 = 2;
pub const NAL_UNIT_TYPE_PARTITION_B: u8 = 3;
pub const NAL_UNIT_TYPE_PARTITION_C: u8 = 4;
pub const NAL_UNIT_TYPE_IDR: u8 = 5;                // Instant Decoder Refresh
pub const NAL_UNIT_TYPE_SEI: u8 = 6;
pub const NAL_UNIT_TYPE_SPS: u8 = 7;
pub const NAL_UNIT_TYPE_PPS: u8 = 8;
pub const NAL_UNIT_TYPE_AUD: u8 = 9;
pub const NAL_UNIT_TYPE_END_OF_SEQUENCE: u8 = 10;
pub const NAL_UNIT_TYPE_END_OF_STREAM: u8 = 11;
pub const NAL_UNIT_TYPE_FILLER_DATA: u8 = 12;
pub const NAL_UNIT_TYPE_SPS_EXT: u8 = 13;

/*
struct NALUnit {
    nal_header: u8,
    rbsp: Vec<u8>,
}
*/

#[derive(Debug)]
pub struct RTPHeader {
    version: u8,
    padding: u8,
    extension: u8,
    csrc_count: u8,
    marker: u8,
    payload_type: u8,
    sequence_number: u16,
    timestamp: u32,
    ssrc: u32,
}

pub struct RTPReceiver {
    rtp_socket: UdpSocket,
    rtcp_socket: UdpSocket,
}

impl RTPReceiver {
    pub fn parse_rtp_header(&self, data: &[u8]) -> RTPHeader {
        let version = data[0] >> 6;
        let padding = (data[0] >> 5) & 0x01;
        let extension = (data[0] >> 4) & 0x01;
        let csrc_count = data[0] & 0x0F;
        let marker = data[1] >> 7;
        let payload_type = data[1] & 0x7F;
        let sequence_number = u16::from_be_bytes([data[2], data[3]]);
        let timestamp = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let ssrc = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        RTPHeader {
            version,
            padding,
            extension,
            csrc_count,
            marker,
            payload_type,
            sequence_number,
            timestamp,
            ssrc,
        }
    }

    pub fn receive(&self) -> Result<(RTPHeader, Vec<u8>), io::Error> {
        let mut buffer = [0; 1500];
        let mut header: Option<RTPHeader> = None;
        let mut payload: Vec<u8> = Vec::new();
        self.rtp_socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        match self.rtp_socket.recv_from(&mut buffer) {
            Ok((size, _)) => {
                let header = self.parse_rtp_header(&buffer);
                let payload = buffer[12..size].to_vec();
                return Ok((header, payload));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                Err(io::Error::new(io::ErrorKind::TimedOut, "recv timed out"))
            }
            Err(e) => panic!("recv error: {:?}", e),
        }
    }

    pub fn new() -> RTPReceiver {
        let rtp_socket = UdpSocket::bind("0.0.0.0:0").unwrap();
        let port = rtp_socket.local_addr().unwrap().port();
        let rtcp_socket = UdpSocket::bind(format!("0.0.0.0:{}", (port+1))).unwrap();
        RTPReceiver {
            rtp_socket,
            rtcp_socket
        }
    }

    pub fn get_rtp_port(&self) -> u16 {
        return self.rtp_socket.local_addr().unwrap().port();
    }
    pub fn get_rtcp_port(&self) -> u16 {
        return self.rtcp_socket.local_addr().unwrap().port();
    }

}