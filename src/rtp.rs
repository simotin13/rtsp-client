use std::net::{UdpSocket};
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
    socket: UdpSocket,
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

    pub fn receive(&self) -> (RTPHeader, Vec<u8>) {
        let mut buffer = [0; 1500];
        let (size, _) = self.socket.recv_from(&mut buffer).unwrap();
        let header = self.parse_rtp_header(&buffer);
        let payload = buffer[12..size].to_vec();
        (header, payload)
    }

    pub fn new(port: u16) -> RTPReceiver {
        println!("RTPReceiver::new({})", port);
        let socket = UdpSocket::bind(format!("0.0.0.0:{}", port)).unwrap();
        RTPReceiver {
            socket,
        }
    }
}