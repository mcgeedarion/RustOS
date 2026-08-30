// src/syscall/net.rs
//! Network-related System Calls
//!
//! Implements socket, bind, connect, accept, send, recv, and related syscalls.

use crate::net::socket::{SocketOps, SocketDomain, SocketType, SocketProtocol, SocketAddrIn, SocketError};
use crate::process::Process;
use crate::fd::{FileDescriptor, FileLike};
use core::ffi::{c_int, c_void};
use alloc::boxed::Box;

/// Socket syscall: Create an endpoint for communication
/// 
/// # Arguments
/// * `domain` - Communication domain (AF_INET, AF_UNIX, etc.)
/// * `type_` - Socket type (SOCK_STREAM, SOCK_DGRAM, etc.)
/// * `protocol` - Protocol to use (IPPROTO_TCP, IPPROTO_UDP, etc.)
/// 
/// # Returns
/// File descriptor on success, negative errno on failure
pub fn sys_socket(domain: c_int, type_: c_int, protocol: c_int) -> Result<c_int, i32> {
    let sock_domain = match domain {
        1 => SocketDomain::Unix,   // AF_UNIX
        2 => SocketDomain::Inet,   // AF_INET
        10 => SocketDomain::Inet6, // AF_INET6
        _ => return Err(-22), // EINVAL
    };

    let sock_type = match type_ & 0xF {
        1 => SocketType::Stream,   // SOCK_STREAM
        2 => SocketType::Datagram, // SOCK_DGRAM
        3 => SocketType::Raw,      // SOCK_RAW
        _ => return Err(-22), // EINVAL
    };

    let sock_protocol = match protocol {
        0 => SocketProtocol::Ip,   // IPPROTO_IP
        6 => SocketProtocol::Tcp,  // IPPROTO_TCP
        17 => SocketProtocol::Udp, // IPPROTO_UDP
        1 => SocketProtocol::Icmp, // IPPROTO_ICMP
        _ if protocol <= 255 => SocketProtocol::Ip,
        _ => return Err(-22), // EINVAL
    };

    match crate::net::socket::socket(sock_domain, sock_type, sock_protocol) {
        Ok(socket_ops) => {
            let proc = Process::current().ok_or(-3)?; // ESRCH
            let mut fd_table = proc.fd_table.lock();
            
            // Find available FD
            let fd = fd_table.alloc()?;
            let file_like = Box::new(SocketFileLike { inner: socket_ops });
            fd_table.insert(fd, FileDescriptor::new(file_like));
            
            Ok(fd)
        },
        Err(e) => Err(socket_error_to_errno(e)),
    }
}

/// Bind syscall: Assign a local address to a socket
pub fn sys_bind(sockfd: c_int, addr: *const c_void, addrlen: c_int) -> Result<c_int, i32> {
    if addr.is_null() || addrlen < 8 {
        return Err(-22); // EINVAL
    }

    let proc = Process::current().ok_or(-3)?; // ESRCH
    let fd_table = proc.fd_table.lock();
    let fd = fd_table.get(sockfd).ok_or(-9)?; // EBADF
    
    // Cast to SocketFileLike
    let socket_file = fd.as_any().downcast_ref::<SocketFileLike>().ok_or(-88)?; // ENOTSOCK
    
    // Parse sockaddr_in structure (simplified)
    unsafe {
        let sock_addr = &*(addr as *const sockaddr_in);
        if sock_addr.sin_family != 2 { // AF_INET
            return Err(-22); // EINVAL
        }
        
        let ip_bytes = sock_addr.sin_addr.to_bytes();
        let socket_addr = SocketAddrIn::new(
            crate::net::ipv4::Ipv4Addr::from_bytes(ip_bytes),
            u16::from_be(sock_addr.sin_port),
        );
        
        drop(fd_table);
        let mut fd_table = proc.fd_table.lock();
        let fd = fd_table.get_mut(sockfd).ok_or(-9)?;
        let socket_file = fd.as_any_mut().downcast_mut::<SocketFileLike>().ok_or(-88)?;
        
        socket_file.inner.bind(socket_addr)
            .map(|_| 0)
            .map_err(socket_error_to_errno)
    }
}

/// Listen syscall: Mark a socket as passive
pub fn sys_listen(sockfd: c_int, backlog: c_int) -> Result<c_int, i32> {
    if backlog < 0 {
        return Err(-22); // EINVAL
    }

    let proc = Process::current().ok_or(-3)?;
    let mut fd_table = proc.fd_table.lock();
    let fd = fd_table.get_mut(sockfd).ok_or(-9)?;
    let socket_file = fd.as_any_mut().downcast_mut::<SocketFileLike>().ok_or(-88)?;
    
    socket_file.inner.listen(backlog as usize)
        .map(|_| 0)
        .map_err(socket_error_to_errno)
}

/// Accept syscall: Accept a connection on a socket
pub fn sys_accept(sockfd: c_int, addr: *mut c_void, addrlen: *mut c_int) -> Result<c_int, i32> {
    let proc = Process::current().ok_or(-3)?;
    let mut fd_table = proc.fd_table.lock();
    let fd = fd_table.get_mut(sockfd).ok_or(-9)?;
    let socket_file = fd.as_any_mut().downcast_mut::<SocketFileLike>().ok_or(-88)?;
    
    match socket_file.inner.accept() {
        Ok(new_socket) => {
            // Allocate new FD for accepted socket
            let new_fd = fd_table.alloc()?;
            let file_like = Box::new(SocketFileLike { inner: new_socket });
            fd_table.insert(new_fd, FileDescriptor::new(file_like));
            
            // Fill in client address if provided
            if !addr.is_null() && !addrlen.is_null() {
                unsafe {
                    // TODO: Fill sockaddr_in with peer address
                    *addrlen = core::mem::size_of::<sockaddr_in>() as c_int;
                }
            }
            
            Ok(new_fd)
        },
        Err(e) => Err(socket_error_to_errno(e)),
    }
}

/// Connect syscall: Initiate a connection on a socket
pub fn sys_connect(sockfd: c_int, addr: *const c_void, addrlen: c_int) -> Result<c_int, i32> {
    if addr.is_null() || addrlen < 8 {
        return Err(-22); // EINVAL
    }

    let proc = Process::current().ok_or(-3)?;
    let mut fd_table = proc.fd_table.lock();
    let fd = fd_table.get_mut(sockfd).ok_or(-9)?;
    let socket_file = fd.as_any_mut().downcast_mut::<SocketFileLike>().ok_or(-88)?;
    
    unsafe {
        let sock_addr = &*(addr as *const sockaddr_in);
        if sock_addr.sin_family != 2 { // AF_INET
            return Err(-22); // EINVAL
        }
        
        let ip_bytes = sock_addr.sin_addr.to_bytes();
        let socket_addr = SocketAddrIn::new(
            crate::net::ipv4::Ipv4Addr::from_bytes(ip_bytes),
            u16::from_be(sock_addr.sin_port),
        );
        
        socket_file.inner.connect(socket_addr)
            .map(|_| 0)
            .map_err(socket_error_to_errno)
    }
}

/// Send syscall: Send data on a socket
pub fn sys_send(sockfd: c_int, buf: *const c_void, len: usize, flags: c_int) -> Result<c_int, i32> {
    if buf.is_null() || len == 0 {
        return Err(-22); // EINVAL
    }

    let proc = Process::current().ok_or(-3)?;
    let mut fd_table = proc.fd_table.lock();
    let fd = fd_table.get_mut(sockfd).ok_or(-9)?;
    let socket_file = fd.as_any_mut().downcast_mut::<SocketFileLike>().ok_or(-88)?;
    
    let slice = unsafe { core::slice::from_raw_parts(buf as *const u8, len) };
    
    match socket_file.inner.send(slice) {
        Ok(n) => Ok(n as c_int),
        Err(e) => Err(socket_error_to_errno(e)),
    }
}

/// Recv syscall: Receive data from a socket
pub fn sys_recv(sockfd: c_int, buf: *mut c_void, len: usize, flags: c_int) -> Result<c_int, i32> {
    if buf.is_null() || len == 0 {
        return Err(-22); // EINVAL
    }

    let proc = Process::current().ok_or(-3)?;
    let mut fd_table = proc.fd_table.lock();
    let fd = fd_table.get_mut(sockfd).ok_or(-9)?;
    let socket_file = fd.as_any_mut().downcast_mut::<SocketFileLike>().ok_or(-88)?;
    
    let slice = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, len) };
    
    match socket_file.inner.recv(slice) {
        Ok(n) => Ok(n as c_int),
        Err(e) => Err(socket_error_to_errno(e)),
    }
}

/// Close syscall: Close a socket
pub fn sys_shutdown(sockfd: c_int, how: c_int) -> Result<c_int, i32> {
    let proc = Process::current().ok_or(-3)?;
    let mut fd_table = proc.fd_table.lock();
    let fd = fd_table.get_mut(sockfd).ok_or(-9)?;
    let socket_file = fd.as_any_mut().downcast_mut::<SocketFileLike>().ok_or(-88)?;
    
    // how: 0=SHUT_RD, 1=SHUT_WR, 2=SHUT_RDWR
    // For simplicity, we just close the socket
    socket_file.inner.close()
        .map(|_| 0)
        .map_err(socket_error_to_errno)
}

/// Getsockopt syscall: Get socket options
pub fn sys_getsockopt(sockfd: c_int, level: c_int, optname: c_int, optval: *mut c_void, optlen: *mut c_int) -> Result<c_int, i32> {
    if optval.is_null() || optlen.is_null() {
        return Err(-22); // EINVAL
    }

    let proc = Process::current().ok_or(-3)?;
    let fd_table = proc.fd_table.lock();
    let fd = fd_table.get(sockfd).ok_or(-9)?;
    let socket_file = fd.as_any().downcast_ref::<SocketFileLike>().ok_or(-88)?;
    
    match socket_file.inner.getsockopt(level, optname) {
        Ok(val) => {
            unsafe {
                let len_ptr = *optlen;
                let copy_len = core::cmp::min(val.len(), len_ptr as usize);
                core::ptr::copy_nonoverlapping(val.as_ptr(), optval as *mut u8, copy_len);
                *optlen = copy_len as c_int;
            }
            Ok(0)
        },
        Err(e) => Err(socket_error_to_errno(e)),
    }
}

/// Setsockopt syscall: Set socket options
pub fn sys_setsockopt(sockfd: c_int, level: c_int, optname: c_int, optval: *const c_void, optlen: c_int) -> Result<c_int, i32> {
    if optval.is_null() || optlen <= 0 {
        return Err(-22); // EINVAL
    }

    let proc = Process::current().ok_or(-3)?;
    let mut fd_table = proc.fd_table.lock();
    let fd = fd_table.get_mut(sockfd).ok_or(-9)?;
    let socket_file = fd.as_any_mut().downcast_mut::<SocketFileLike>().ok_or(-88)?;
    
    let val = unsafe { core::slice::from_raw_parts(optval as *const u8, optlen as usize) };
    
    socket_file.inner.setsockopt(level, optname, val)
        .map(|_| 0)
        .map_err(socket_error_to_errno)
}

/// Helper: Convert SocketError to errno
fn socket_error_to_errno(err: SocketError) -> i32 {
    match err {
        SocketError::InvalidDomain => -22, // EINVAL
        SocketError::InvalidType => -22,   // EINVAL
        SocketError::InvalidProtocol => -22, // EINVAL
        SocketError::AddressInUse => -98,  // EADDRINUSE
        SocketError::AddressNotAvailable => -99, // EADDRNOTAVAIL
        SocketError::NotConnected => -107, // ENOTCONN
        SocketError::ConnectionRefused => -111, // ECONNREFUSED
        SocketError::ConnectionReset => -104, // ECONNRESET
        SocketError::TimedOut => -110,     // ETIMEDOUT
        SocketError::BufferFull => -105,   // ENOBUFS
        SocketError::BufferEmpty => -11,   // EAGAIN/EWOULDBLOCK
        SocketError::AlreadyConnected => -106, // EISCONN
        SocketError::NotListening => -108, // ENOTCONN or EINVAL
        SocketError::InvalidState => -22,  // EINVAL
        SocketError::PermissionDenied => -1, // EPERM
    }
}

/// Wrapper struct to implement FileLike for sockets
struct SocketFileLike {
    inner: Box<dyn SocketOps>,
}

impl FileLike for SocketFileLike {
    fn read(&self, _buf: &mut [u8]) -> Result<usize, i32> {
        Err(-29) // ESPIPE - sockets don't support seek-based read
    }
    
    fn write(&self, _buf: &[u8]) -> Result<usize, i32> {
        Err(-29) // ESPIPE
    }
    
    fn flush(&self) -> Result<(), i32> {
        Ok(())
    }
}

/// sockaddr_in structure (C compatibility)
#[repr(C)]
struct sockaddr_in {
    sin_family: u16,
    sin_port: u16,
    sin_addr: in_addr,
    sin_zero: [u8; 8],
}

#[repr(C)]
struct in_addr {
    s_addr: u32,
}

impl in_addr {
    fn to_bytes(&self) -> [u8; 4] {
        self.s_addr.to_be_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_error_conversion() {
        assert_eq!(socket_error_to_errno(SocketError::InvalidDomain), -22);
        assert_eq!(socket_error_to_errno(SocketError::ConnectionRefused), -111);
    }
}
