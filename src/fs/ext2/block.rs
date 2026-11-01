extern crate alloc;
use super::superblock::Ext2Fs;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

pub fn read_block(fs: &Ext2Fs, blkno: u32) -> Vec<u8> {
    fs.read_block(blkno)
}
pub fn write_block(fs: &Ext2Fs, blkno: u32, data: &[u8]) {
    fs.write_block(blkno, data)
}
