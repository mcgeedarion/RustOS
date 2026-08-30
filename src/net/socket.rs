// src/net/socket.rs
//! Socket API Implementation
//!
//! Provides BSD-style socket interface for TCP/UDP communications.

use crate::net::tcp::state_machine::{TcpControlBlock, TcpState, TcpFlags, TcpAction, TcpError as TcpStackError};
use crate::net::ipv4::Ipv4Addr;
use crate::sync::SpinLock;
use alloc::vec::Vec;
use core::ffi::c_int;

/// Socket domain/family
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketDomain {
    Unix,
    Inet, // IPv4
    Inet6, // IPv6
}

/// Socket type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketType {
    Stream,  // TCP
    Datagram, // UDP
    Raw,
}

/// Socket protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketProtocol {
    Ip,
    Tcp,
    Udp,
    Icmp,
}

/// Socket address (simplified IPv4)
#[derive(Debug, Clone, Copy)]
pub struct SocketAddrIn {
    pub addr: Ipv4Addr,
    pub port: u16,
}

impl SocketAddrIn {
    pub fn new(addr: Ipv4Addr, port: u16) -> Self {
        Self { addr, port }
    }
}

/// Socket error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketError {
    InvalidDomain,
    InvalidType,
    InvalidProtocol,
    AddressInUse,
    AddressNotAvailable,
    NotConnected,
    ConnectionRefused,
    ConnectionReset,
    TimedOut,
    BufferFull,
    BufferEmpty,
    AlreadyConnected,
    NotListening,
    InvalidState,
    PermissionDenied,
}

/// Socket file descriptor operations
pub trait SocketOps {
    fn bind(&mut self, addr: SocketAddrIn) -> Result<(), SocketError>;
    fn listen(&mut self, backlog: usize) -> Result<(), SocketError>;
    fn accept(&mut self) -> Result<Box<dyn SocketOps>, SocketError>;
    fn connect(&mut self, addr: SocketAddrIn) -> Result<(), SocketError>;
    fn send(&mut self, buf: &[u8]) -> Result<usize, SocketError>;
    fn recv(&mut self, buf: &mut [u8]) -> Result<usize, SocketError>;
    fn close(&mut self) -> Result<(), SocketError>;
    fn getsockopt(&self, level: c_int, optname: c_int) -> Result<Vec<u8>, SocketError>;
    fn setsockopt(&mut self, level: c_int, optname: c_int, optval: &[u8]) -> Result<(), SocketError>;
}

/// TCP Socket implementation
pub struct TcpSocket {
    domain: SocketDomain,
    socket_type: SocketType,
    protocol: SocketProtocol,
    state: SocketState,
    local_addr: Option<SocketAddrIn>,
    remote_addr: Option<SocketAddrIn>,
    tcb: Option<TcpControlBlock>,
    listen_queue: SpinLock<Vec<TcpControlBlock>>, // For listening sockets
    options: SocketOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SocketState {
    Closed,
    Bound,
    Listening,
    Connecting,
    Connected,
    Closing,
}

#[derive(Debug, Clone)]
struct SocketOptions {
    reuse_addr: bool,
    keep_alive: bool,
    no_delay: bool,
    linger: Option<u32>, // Seconds
    receive_buffer_size: usize,
    send_buffer_size: usize,
}

impl Default for SocketOptions {
    fn default() -> Self {
        Self {
            reuse_addr: false,
            keep_alive: false,
            no_delay: false,
            linger: None,
            receive_buffer_size: 65535,
            send_buffer_size: 65535,
        }
    }
}

impl TcpSocket {
    pub fn new(domain: SocketDomain, socket_type: SocketType, protocol: SocketProtocol) -> Result<Self, SocketError> {
        if domain != SocketDomain::Inet {
            return Err(SocketError::InvalidDomain);
        }
        if socket_type != SocketType::Stream {
            return Err(SocketError::InvalidType);
        }
        if protocol != SocketProtocol::Tcp && protocol != SocketProtocol::Ip {
            return Err(SocketError::InvalidProtocol);
        }

        Ok(Self {
            domain,
            socket_type,
            protocol,
            state: SocketState::Closed,
            local_addr: None,
            remote_addr: None,
            tcb: None,
            listen_queue: SpinLock::new(Vec::new()),
            options: SocketOptions::default(),
        })
    }
}

impl SocketOps for TcpSocket {
    fn bind(&mut self, addr: SocketAddrIn) -> Result<(), SocketError> {
        if self.state != SocketState::Closed {
            return Err(SocketError::InvalidState);
        }
        
        // Check if address already in use (simplified check)
        // In real implementation, would check global socket table
        
        self.local_addr = Some(addr);
        self.state = SocketState::Bound;
        Ok(())
    }

    fn listen(&mut self, _backlog: usize) -> Result<(), SocketError> {
        if self.state != SocketState::Bound {
            return Err(SocketError::NotListening);
        }
        
        // Initialize TCB for listening
        let local = self.local_addr.ok_or(SocketError::AddressNotAvailable)?;
        self.tcb = Some(TcpControlBlock::new(
            local.addr,
            Ipv4Addr::ANY, // Remote not known yet
            local.port,
            0, // Remote port not known
        ));
        
        self.state = SocketState::Listening;
        Ok(())
    }

    fn accept(&mut self) -> Result<Box<dyn SocketOps>, SocketError> {
        if self.state != SocketState::Listening {
            return Err(SocketError::NotListening);
        }

        // In real implementation, would wait for incoming connection
        // For now, return error if queue empty
        let mut queue = self.listen_queue.lock();
        if queue.is_empty() {
            return Err(SocketError::BufferEmpty);
        }

        let accepted_tcb = queue.remove(0);
        let remote_addr = SocketAddrIn::new(accepted_tcb.remote_addr, accepted_tcb.remote_port);
        
        let mut new_socket = TcpSocket::new(self.domain, self.socket_type, self.protocol)?;
        new_socket.state = SocketState::Connected;
        new_socket.local_addr = self.local_addr;
        new_socket.remote_addr = Some(remote_addr);
        new_socket.tcb = Some(accepted_tcb);
        
        Ok(Box::new(new_socket))
    }

    fn connect(&mut self, addr: SocketAddrIn) -> Result<(), SocketError> {
        if self.state != SocketState::Bound && self.state != SocketState::Closed {
            return Err(SocketError::AlreadyConnected);
        }

        let local = self.local_addr.unwrap_or(SocketAddrIn::new(Ipv4Addr::ANY, 0));
        
        // Create TCB and initiate connection
        let mut tcb = TcpControlBlock::new(local.addr, addr.addr, local.port, addr.port);
        
        // Send SYN (state transition handled by TCP stack)
        tcb.state = TcpState::SynSent;
        tcb.snd_nxt = 1; // Initial sequence number
        
        self.tcb = Some(tcb);
        self.remote_addr = Some(addr);
        self.state = SocketState::Connecting;
        
        // In real implementation, would wait for SYN-ACK
        // For now, assume immediate success for loopback
        if addr.addr.is_loopback() {
            self.state = SocketState::Connected;
            if let Some(ref mut tcb) = self.tcb {
                tcb.state = TcpState::Established;
            }
        }
        
        Ok(())
    }

    fn send(&mut self, buf: &[u8]) -> Result<usize, SocketError> {
        if self.state != SocketState::Connected {
            return Err(SocketError::NotConnected);
        }

        let tcb = self.tcb.as_mut().ok_or(SocketError::NotConnected)?;
        
        // Check window size
        let available_window = tcb.snd_wnd - (tcb.snd_nxt - tcb.snd_una);
        if available_window == 0 {
            return Err(SocketError::BufferFull);
        }

        let to_send = core::cmp::min(buf.len(), available_window as usize);
        
        // Copy to send buffer
        let mut send_buf = tcb.send_buffer.lock();
        send_buf.extend_from_slice(&buf[..to_send]);
        
        // Update sequence number
        tcb.snd_nxt += to_send as u32;
        
        Ok(to_send)
    }

    fn recv(&mut self, buf: &mut [u8]) -> Result<usize, SocketError> {
        if self.state != SocketState::Connected {
            return Err(SocketError::NotConnected);
        }

        let tcb = self.tcb.as_mut().ok_or(SocketError::NotConnected)?;
        
        let mut recv_buf = tcb.recv_buffer.lock();
        if recv_buf.is_empty() {
            return Err(SocketError::BufferEmpty);
        }

        let to_read = core::cmp::min(buf.len(), recv_buf.len());
        buf[..to_read].copy_from_slice(&recv_buf[..to_read]);
        recv_buf.drain(..to_read);
        
        // Update window
        tcb.rcv_wnd = tcb.options.receive_buffer_size as u32 - recv_buf.len() as u32;
        
        Ok(to_read)
    }

    fn close(&mut self) -> Result<(), SocketError> {
        if self.state != SocketState::Connected {
            self.state = SocketState::Closed;
            return Ok(());
        }

        let tcb = self.tcb.as_mut().ok_or(SocketError::NotConnected)?;
        
        match tcb.initiate_close() {
            Ok(TcpAction::SendFin) => {
                tcb.snd_nxt += 1; // FIN consumes one sequence number
                self.state = SocketState::Closing;
                Ok(())
            },
            Err(_) => Err(SocketError::InvalidState),
            _ => Ok(()),
        }
    }

    fn getsockopt(&self, _level: c_int, optname: c_int) -> Result<Vec<u8>, SocketError> {
        // Simplified option handling
        match optname {
            0x0002 => Ok(vec![if self.options.reuse_addr { 1 } else { 0 }]), // SO_REUSEADDR
            0x0009 => Ok(vec![if self.options.keep_alive { 1 } else { 0 }]), // SO_KEEPALIVE
            _ => Err(SocketError::InvalidState),
        }
    }

    fn setsockopt(&mut self, _level: c_int, optname: c_int, optval: &[u8]) -> Result<(), SocketError> {
        if optval.is_empty() {
            return Err(SocketError::InvalidState);
        }
        
        match optname {
            0x0002 => { // SO_REUSEADDR
                self.options.reuse_addr = optval[0] != 0;
                Ok(())
            },
            0x0009 => { // SO_KEEPALIVE
                self.options.keep_alive = optval[0] != 0;
                Ok(())
            },
            _ => Err(SocketError::InvalidState),
        }
    }
}

// Socket creation factory
pub fn socket(domain: SocketDomain, socket_type: SocketType, protocol: SocketProtocol) -> Result<Box<dyn SocketOps>, SocketError> {
    match (domain, socket_type, protocol) {
        (SocketDomain::Inet, SocketType::Stream, SocketProtocol::Tcp) |
        (SocketDomain::Inet, SocketType::Stream, SocketProtocol::Ip) => {
            Ok(Box::new(TcpSocket::new(domain, socket_type, protocol)?))
        },
        _ => Err(SocketError::InvalidType),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_creation() {
        let sock = socket(SocketDomain::Inet, SocketType::Stream, SocketProtocol::Tcp);
        assert!(sock.is_ok());
    }

    #[test]
    fn test_bind_and_listen() {
        let mut sock = TcpSocket::new(SocketDomain::Inet, SocketType::Stream, SocketProtocol::Tcp).unwrap();
        
        let addr = SocketAddrIn::new(Ipv4Addr::new(127, 0, 0, 1), 8080);
        assert!(sock.bind(addr).is_ok());
        assert!(sock.listen(10).is_ok());
    }
}
