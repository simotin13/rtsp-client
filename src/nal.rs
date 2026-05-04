#[derive(Debug)]
pub enum NalEvent<'a> {
    Video { data: &'a [u8], ts: u32, is_key: bool, },
    Sps(&'a [u8]),
    Pps(&'a [u8]),
    Sei,
    End,
}
