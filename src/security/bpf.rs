//! Minimal `bpf(2)` map support.
//!
//! This implements the non-program BPF commands that libc probes and small
//! runtime tests commonly exercise: map creation plus element lookup, update,
//! deletion, and next-key iteration for ARRAY and HASH maps.  Program loading
//! and verifier-backed execution remain unsupported and return `EINVAL` rather
//! than a blanket `ENOSYS` for the whole syscall.

extern crate alloc;

use crate::core::fast_hash::KernelFastMap;
use alloc::vec::Vec;
use spin::Mutex;

const EBADF: isize = -9;
const EFAULT: isize = -14;
const EINVAL: isize = -22;
const ENOENT: isize = -2;
const ENOMEM: isize = -12;
const EMFILE: isize = -24;
const E2BIG: isize = -7;

const BPF_MAP_CREATE: i32 = 0;
const BPF_MAP_LOOKUP_ELEM: i32 = 1;
const BPF_MAP_UPDATE_ELEM: i32 = 2;
const BPF_MAP_DELETE_ELEM: i32 = 3;
const BPF_MAP_GET_NEXT_KEY: i32 = 4;

const BPF_MAP_TYPE_HASH: u32 = 1;
const BPF_MAP_TYPE_ARRAY: u32 = 2;

const BPF_ANY: u64 = 0;
const BPF_NOEXIST: u64 = 1;
const BPF_EXIST: u64 = 2;

const MAX_KEY_SIZE: usize = 512;
const MAX_VALUE_SIZE: usize = 4096;
const MAX_ENTRIES: usize = 65_536;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MapCreateAttr {
    map_type: u32,
    key_size: u32,
    value_size: u32,
    max_entries: u32,
    map_flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ElemAttr {
    map_fd: u32,
    key: u64,
    value: u64,
    flags: u64,
}

#[derive(Clone)]
struct BpfMap {
    map_type: u32,
    key_size: usize,
    value_size: usize,
    max_entries: usize,
    entries: Vec<(Vec<u8>, Vec<u8>)>,
}

static MAPS: Mutex<KernelFastMap<usize, BpfMap>> = Mutex::new(KernelFastMap::new());

fn copy_attr<T: Copy + Default>(attr: usize, size: u32) -> Result<T, isize> {
    if attr == 0 {
        return Err(EFAULT);
    }
    if (size as usize) < core::mem::size_of::<T>() {
        return Err(EINVAL);
    }
    let mut value = T::default();
    crate::uaccess::copy_from_user(
        &mut value as *mut T as *mut u8,
        attr,
        core::mem::size_of::<T>(),
    )
    .map_err(|_| EFAULT)?;
    Ok(value)
}

fn copy_user_bytes(addr: u64, len: usize) -> Result<Vec<u8>, isize> {
    if len == 0 || addr == 0 {
        return Err(EFAULT);
    }
    let mut bytes = alloc::vec![0u8; len];
    crate::uaccess::copy_from_user(bytes.as_mut_ptr(), addr as usize, len).map_err(|_| EFAULT)?;
    Ok(bytes)
}

fn put_user_bytes(addr: u64, bytes: &[u8]) -> Result<(), isize> {
    if bytes.is_empty() || addr == 0 {
        return Err(EFAULT);
    }
    crate::uaccess::copy_to_user(addr as usize, bytes.as_ptr(), bytes.len()).map_err(|_| EFAULT)
}

fn find_entry(map: &BpfMap, key: &[u8]) -> Option<usize> {
    map.entries
        .iter()
        .position(|(entry_key, _)| entry_key == key)
}

pub fn is_bpf_fd(fd: usize) -> bool {
    MAPS.lock().contains_key(&fd)
}

pub fn close_bpf_fd(fd: usize) {
    MAPS.lock().remove(&fd);
    let _ = crate::fs::vfs::close(fd);
}

/// Duplicate hook for process-local fd aliases. BPF map state is shared by the
/// backing fd, so there is no per-alias state to clone.
pub fn dup_bpf_fd(_fd: usize) {}

fn map_create(attr: usize, size: u32) -> isize {
    let attr = match copy_attr::<MapCreateAttr>(attr, size) {
        Ok(attr) => attr,
        Err(e) => return e,
    };
    if attr.map_flags != 0
        || !matches!(attr.map_type, BPF_MAP_TYPE_HASH | BPF_MAP_TYPE_ARRAY)
        || attr.key_size == 0
        || attr.value_size == 0
        || attr.max_entries == 0
        || attr.key_size as usize > MAX_KEY_SIZE
        || attr.value_size as usize > MAX_VALUE_SIZE
        || attr.max_entries as usize > MAX_ENTRIES
    {
        return EINVAL;
    }
    if attr.map_type == BPF_MAP_TYPE_ARRAY && attr.key_size != 4 {
        return EINVAL;
    }

    let fd = match crate::fs::vfs::open_anon(crate::fs::vfs::O_CLOEXEC) {
        Ok(fd) => fd,
        Err(_) => return EMFILE,
    };
    let mut entries = Vec::new();
    if attr.map_type == BPF_MAP_TYPE_ARRAY {
        if entries.try_reserve(attr.max_entries as usize).is_err() {
            let _ = crate::fs::vfs::close(fd);
            return ENOMEM;
        }
        for index in 0..attr.max_entries {
            entries.push((
                index.to_ne_bytes().to_vec(),
                alloc::vec![0u8; attr.value_size as usize],
            ));
        }
    }
    MAPS.lock().insert(
        fd,
        BpfMap {
            map_type: attr.map_type,
            key_size: attr.key_size as usize,
            value_size: attr.value_size as usize,
            max_entries: attr.max_entries as usize,
            entries,
        },
    );
    fd as isize
}

fn elem_attr(attr: usize, size: u32) -> Result<ElemAttr, isize> {
    copy_attr::<ElemAttr>(attr, size)
}

fn map_lookup(attr: usize, size: u32) -> isize {
    let attr = match elem_attr(attr, size) {
        Ok(attr) => attr,
        Err(e) => return e,
    };
    let maps = MAPS.lock();
    let map = match maps.get(&(attr.map_fd as usize)) {
        Some(map) => map,
        None => return EBADF,
    };
    let key = match copy_user_bytes(attr.key, map.key_size) {
        Ok(key) => key,
        Err(e) => return e,
    };
    let index = match find_entry(map, &key) {
        Some(index) => index,
        None => return ENOENT,
    };
    put_user_bytes(attr.value, &map.entries[index].1).map_or_else(|e| e, |_| 0)
}

fn map_update(attr: usize, size: u32) -> isize {
    let attr = match elem_attr(attr, size) {
        Ok(attr) => attr,
        Err(e) => return e,
    };
    if !matches!(attr.flags, BPF_ANY | BPF_NOEXIST | BPF_EXIST) {
        return EINVAL;
    }
    let mut maps = MAPS.lock();
    let map = match maps.get_mut(&(attr.map_fd as usize)) {
        Some(map) => map,
        None => return EBADF,
    };
    let key = match copy_user_bytes(attr.key, map.key_size) {
        Ok(key) => key,
        Err(e) => return e,
    };
    let value = match copy_user_bytes(attr.value, map.value_size) {
        Ok(value) => value,
        Err(e) => return e,
    };
    let existing = find_entry(map, &key);
    if map.map_type == BPF_MAP_TYPE_ARRAY && existing.is_none() {
        return E2BIG;
    }
    match (attr.flags, existing) {
        (BPF_NOEXIST, Some(_)) => EINVAL,
        (BPF_EXIST, None) => ENOENT,
        (_, Some(index)) => {
            map.entries[index].1 = value;
            0
        },
        (_, None) => {
            if map.entries.len() >= map.max_entries {
                return E2BIG;
            }
            map.entries.push((key, value));
            0
        },
    }
}

fn map_delete(attr: usize, size: u32) -> isize {
    let attr = match elem_attr(attr, size) {
        Ok(attr) => attr,
        Err(e) => return e,
    };
    let mut maps = MAPS.lock();
    let map = match maps.get_mut(&(attr.map_fd as usize)) {
        Some(map) => map,
        None => return EBADF,
    };
    if map.map_type == BPF_MAP_TYPE_ARRAY {
        return EINVAL;
    }
    let key = match copy_user_bytes(attr.key, map.key_size) {
        Ok(key) => key,
        Err(e) => return e,
    };
    match find_entry(map, &key) {
        Some(index) => {
            map.entries.swap_remove(index);
            0
        },
        None => ENOENT,
    }
}

fn map_get_next_key(attr: usize, size: u32) -> isize {
    let attr = match elem_attr(attr, size) {
        Ok(attr) => attr,
        Err(e) => return e,
    };
    let maps = MAPS.lock();
    let map = match maps.get(&(attr.map_fd as usize)) {
        Some(map) => map,
        None => return EBADF,
    };
    let next = if attr.key == 0 {
        map.entries.first().map(|(key, _)| key)
    } else {
        let key = match copy_user_bytes(attr.key, map.key_size) {
            Ok(key) => key,
            Err(e) => return e,
        };
        find_entry(map, &key).and_then(|index| map.entries.get(index + 1).map(|(key, _)| key))
    };
    match next {
        Some(key) => put_user_bytes(attr.value, key).map_or_else(|e| e, |_| 0),
        None => ENOENT,
    }
}

pub fn sys_bpf(cmd: i32, attr: usize, size: u32) -> isize {
    match cmd {
        BPF_MAP_CREATE => map_create(attr, size),
        BPF_MAP_LOOKUP_ELEM => map_lookup(attr, size),
        BPF_MAP_UPDATE_ELEM => map_update(attr, size),
        BPF_MAP_DELETE_ELEM => map_delete(attr, size),
        BPF_MAP_GET_NEXT_KEY => map_get_next_key(attr, size),
        _ => EINVAL,
    }
}
