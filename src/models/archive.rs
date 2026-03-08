use crate::utils::get_unix_timestamp;
use bytemuck::{Pod, Zeroable};
use std::io::Write;

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct ArchiveHeader {
    pub signature: [u8; 4],
    pub version: u8,
    pub timestamp: u64,
}

unsafe impl Pod for ArchiveHeader {}
unsafe impl Zeroable for ArchiveHeader {}

impl ArchiveHeader {
    pub fn new() -> Self {
        Self {
            signature: *b"DARI",
            version: 5,
            timestamp: get_unix_timestamp().unwrap(),
        }
    }

    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(bytemuck::bytes_of(self))
    }
}
