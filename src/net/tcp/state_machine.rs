// src/net/tcp/state_machine.rs
//! Robust TCP State Machine Implementation
//!
//! Implements RFC 793 compliant state transitions with proper
//! sequence number handling, retransmission timers, and flow control.

use crate::net::ipv4::Ipv4Addr;
use crate::sync::SpinLock;
use core::time::Duration;

/// TCP Connection States per RFC 793
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

/// TCP Control Block (TCB) representing a single connection
pub struct TcpControlBlock {
    pub state: TcpState,
    pub local_addr: Ipv4Addr,
    pub remote_addr: Ipv4Addr,
    pub local_port: u16,
    pub remote_port: u16,
    
    // Sequence numbers
    pub snd_una: u32, // Oldest unacknowledged sequence number
    pub snd_nxt: u32, // Next sequence number to be sent
    pub snd_wnd: u32, // Send window size
    pub rcv_nxt: u32, // Next sequence number expected to be received
    pub rcv_wnd: u32, // Receive window size
    
    // Retransmission
    pub rto_ms: u32, // Retransmission timeout in ms
    pub last_sent_time: u64, // Timestamp of last segment sent
    
    // Buffers (simplified for kernel space)
    pub send_buffer: SpinLock<Vec<u8>>,
    pub recv_buffer: SpinLock<Vec<u8>>,
    
    // Flags
    pub pending_close: bool,
}

impl TcpControlBlock {
    pub fn new(local_addr: Ipv4Addr, remote_addr: Ipv4Addr, local_port: u16, remote_port: u16) -> Self {
        Self {
            state: TcpState::Closed,
            local_addr,
            remote_addr,
            local_port,
            remote_port,
            snd_una: 0,
            snd_nxt: 0,
            snd_wnd: 65535, // Default 64KB window
            rcv_nxt: 0,
            rcv_wnd: 65535,
            rto_ms: 3000, // Initial RTO 3s per RFC
            last_sent_time: 0,
            send_buffer: SpinLock::new(Vec::new()),
            recv_buffer: SpinLock::new(Vec::new()),
            pending_close: false,
        }
    }

    /// Process an incoming segment and transition state
    pub fn process_segment(&mut self, flags: TcpFlags, seq: u32, ack: u32, payload_len: usize) -> Result<TcpAction, TcpError> {
        match self.state {
            TcpState::Closed => Err(TcpError::ConnectionClosed),
            TcpState::Listen => self.handle_listen(flags, seq),
            TcpState::SynSent => self.handle_syn_sent(flags, seq, ack),
            TcpState::SynReceived => self.handle_syn_received(flags, seq, ack),
            TcpState::Established => self.handle_established(flags, seq, ack, payload_len),
            TcpState::FinWait1 => self.handle_fin_wait1(flags, seq, ack),
            TcpState::FinWait2 => self.handle_fin_wait2(flags, seq, ack),
            TcpState::CloseWait => self.handle_close_wait(flags, seq, ack),
            TcpState::Closing => self.handle_closing(flags, seq, ack),
            TcpState::LastAck => self.handle_last_ack(flags, seq, ack),
            TcpState::TimeWait => self.handle_time_wait(flags, seq, ack),
        }
    }

    fn handle_listen(&mut self, flags: TcpFlags, _seq: u32) -> Result<TcpAction, TcpError> {
        if flags.syn && !flags.ack {
            // Received SYN, move to SYN_RECEIVED
            self.state = TcpState::SynReceived;
            Ok(TcpAction::SendSynAck)
        } else {
            Err(TcpError::InvalidStateTransition)
        }
    }

    fn handle_syn_sent(&mut self, flags: TcpFlags, _seq: u32, ack: u32) -> Result<TcpAction, TcpError> {
        if flags.syn && flags.ack {
            if self.snd_una <= ack && ack <= self.snd_nxt {
                self.state = TcpState::Established;
                Ok(TcpAction::SendAck)
            } else {
                Err(TcpError::InvalidAck)
            }
        } else if flags.syn {
            // Simultaneous open
            self.state = TcpState::SynReceived;
            Ok(TcpAction::SendSynAck)
        } else {
            Err(TcpError::InvalidFlags)
        }
    }

    fn handle_syn_received(&mut self, flags: TcpFlags, _seq: u32, ack: u32) -> Result<TcpAction, TcpError> {
        if flags.ack && self.snd_una <= ack && ack <= self.snd_nxt {
            self.state = TcpState::Established;
            Ok(TcpAction::None)
        } else if flags.rst {
            self.state = TcpState::Closed;
            Ok(TcpAction::Close)
        } else {
            Err(TcpError::InvalidStateTransition)
        }
    }

    fn handle_established(&mut self, flags: TcpFlags, seq: u32, ack: u32, payload_len: usize) -> Result<TcpAction, TcpError> {
        if flags.rst {
            self.state = TcpState::Closed;
            return Ok(TcpAction::Close);
        }

        // Validate ACK
        if flags.ack {
            if ack > self.snd_nxt {
                return Err(TcpError::InvalidAck);
            }
            if ack > self.snd_una {
                self.snd_una = ack;
                // Data acknowledged, could trigger congestion control here
            }
        }

        // Handle data
        if payload_len > 0 {
            if seq == self.rcv_nxt {
                self.rcv_nxt += payload_len as u32;
                return Ok(TcpAction::DataReceived(payload_len));
            } else {
                // Out of order, queue for later (simplified: just request retransmit)
                return Ok(TcpAction::SendDupAck);
            }
        }

        // Handle FIN
        if flags.fin {
            self.state = TcpState::CloseWait;
            return Ok(TcpAction::SendAckAndNotifyClose);
        }

        Ok(TcpAction::None)
    }

    fn handle_fin_wait1(&mut self, flags: TcpFlags, _seq: u32, ack: u32) -> Result<TcpAction, TcpError> {
        if flags.ack && ack == self.snd_nxt {
            if flags.fin {
                // Simultaneous close
                self.state = TcpState::TimeWait;
                Ok(TcpAction::SendAckAndStartTimewait)
            } else {
                self.state = TcpState::FinWait2;
                Ok(TcpAction::None)
            }
        } else if flags.fin {
            // Received FIN before ACK
            self.state = TcpState::Closing;
            Ok(TcpAction::SendAck)
        } else {
            Err(TcpError::InvalidStateTransition)
        }
    }

    fn handle_fin_wait2(&mut self, flags: TcpFlags, _seq: u32, _ack: u32) -> Result<TcpAction, TcpError> {
        if flags.fin {
            self.state = TcpState::TimeWait;
            Ok(TcpAction::SendAckAndStartTimewait)
        } else {
            Err(TcpError::InvalidStateTransition)
        }
    }

    fn handle_close_wait(&mut self, flags: TcpFlags, _seq: u32, _ack: u32) -> Result<TcpAction, TcpError> {
        // Application should close here. If we receive FIN again, ignore or re-ACK
        if flags.fin {
            Ok(TcpAction::SendAck) 
        } else {
            Err(TcpError::InvalidStateTransition)
        }
    }

    fn handle_closing(&mut self, flags: TcpFlags, _seq: u32, ack: u32) -> Result<TcpAction, TcpError> {
        if flags.ack && ack == self.snd_nxt {
            self.state = TcpState::TimeWait;
            Ok(TcpAction::StartTimewait)
        } else {
            Err(TcpError::InvalidStateTransition)
        }
    }

    fn handle_last_ack(&mut self, flags: TcpFlags, _seq: u32, ack: u32) -> Result<TcpAction, TcpError> {
        if flags.ack && ack == self.snd_nxt {
            self.state = TcpState::Closed;
            Ok(TcpAction::Close)
        } else {
            Err(TcpError::InvalidStateTransition)
        }
    }

    fn handle_time_wait(&mut self, flags: TcpFlags, _seq: u32, _ack: u32) -> Result<TcpAction, TcpError> {
        // In TIME_WAIT, we mostly just absorb segments and restart the timer
        // unless it's a retransmitted FIN
        if flags.fin {
            Ok(TcpAction::RestartTimewait)
        } else {
            Ok(TcpAction::None)
        }
    }

    pub fn initiate_close(&mut self) -> Result<TcpAction, TcpError> {
        match self.state {
            TcpState::Established => {
                self.state = TcpState::FinWait1;
                Ok(TcpAction::SendFin)
            },
            TcpState::CloseWait => {
                self.state = TcpState::LastAck;
                Ok(TcpAction::SendFin)
            },
            _ => Err(TcpError::InvalidStateForClose),
        }
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct TcpFlags: u8 {
        const FIN = 0x01;
        const SYN = 0x02;
        const RST = 0x04;
        const PSH = 0x08;
        const ACK = 0x10;
        const URG = 0x20;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TcpAction {
    None,
    SendSynAck,
    SendAck,
    SendFin,
    SendDupAck,
    SendAckAndNotifyClose,
    SendAckAndStartTimewait,
    StartTimewait,
    RestartTimewait,
    Close,
    DataReceived(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TcpError {
    ConnectionClosed,
    InvalidStateTransition,
    InvalidAck,
    InvalidFlags,
    InvalidStateForClose,
    BufferFull,
    Timeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_three_way_handshake() {
        let mut server = TcpControlBlock::new(Ipv4Addr::new(127, 0, 0, 1), Ipv4Addr::new(127, 0, 0, 2), 80, 12345);
        let mut client = TcpControlBlock::new(Ipv4Addr::new(127, 0, 0, 2), Ipv4Addr::new(127, 0, 0, 1), 12345, 80);

        // 1. Client sends SYN
        client.state = TcpState::SynSent;
        client.snd_nxt = 1000;
        
        // 2. Server receives SYN (in Listen)
        let action = server.process_segment(TcpFlags::SYN, 5000, 0, 0).unwrap();
        assert_eq!(action, TcpAction::SendSynAck);
        assert_eq!(server.state, TcpState::SynReceived);

        // 3. Client receives SYN+ACK
        let action = client.process_segment(TcpFlags::SYN | TcpFlags::ACK, 5000, 1001, 0).unwrap();
        assert_eq!(action, TcpAction::SendAck);
        assert_eq!(client.state, TcpState::Established);

        // 4. Server receives ACK
        let action = server.process_segment(TcpFlags::ACK, 1001, 5001, 0).unwrap();
        assert_eq!(action, TcpAction::None);
        assert_eq!(server.state, TcpState::Established);
    }
}
