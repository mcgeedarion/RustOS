#![no_main]

extern crate alloc;

use libfuzzer_sys::fuzz_target;

// The production ext4 parser uses this interface only for device I/O during
// mount. Fuzzing calls pure metadata decoders and never mounts or allocates
// according to untrusted disk-size fields.
mod drivers {
    pub mod virtio_blk {
        pub fn is_present() -> bool { false }
        pub fn read_sectors(_lba: u64, _buffer: &mut [u8]) -> Result<(), ()> { Err(()) }
    }
}

#[path = "../../src/fs/ext4.rs"]
mod production_ext4;

fuzz_target!(|data: &[u8]| {
    let offset = data
        .get(..core::mem::size_of::<usize>())
        .and_then(|bytes| bytes.try_into().ok())
        .map(usize::from_le_bytes)
        .unwrap_or(0);
    let block_number = data
        .get(..core::mem::size_of::<u64>())
        .and_then(|bytes| bytes.try_into().ok())
        .map(u64::from_le_bytes)
        .unwrap_or(0);
    production_ext4::fuzz_metadata(data, offset, block_number);
});
