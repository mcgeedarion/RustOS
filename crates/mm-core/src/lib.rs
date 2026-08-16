//! Memory Management Core for RustOS
//!
//! This crate provides core memory management abstractions including:
//! - Physical memory management (PMM) traits
//! - Virtual memory management (VMM) traits
//! - Page table abstractions
//! - NUMA-aware allocation policies

#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::fmt;
use core::ptr::NonNull;
use alloc::vec::Vec;

pub use bitflags::bitflags;

/// Memory error types with automatic errno conversion
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmError {
    OutOfMemory = 12,
    InvalidArg = 22,
    NotSupported = 95,
    AccessDenied = 13,
    Busy = 16,
}

impl From<MmError> for isize {
    fn from(err: MmError) -> Self {
        err as isize
    }
}

impl From<MmError> for i32 {
    fn from(err: MmError) -> Self {
        err as i32
    }
}

impl fmt::Display for MmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MmError::OutOfMemory => write!(f, "Out of memory"),
            MmError::InvalidArg => write!(f, "Invalid argument"),
            MmError::NotSupported => write!(f, "Operation not supported"),
            MmError::AccessDenied => write!(f, "Access denied"),
            MmError::Busy => write!(f, "Resource busy"),
        }
    }
}

/// Physical page frame number
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pfn(pub usize);

impl Pfn {
    pub const fn new(num: usize) -> Self {
        Self(num)
    }
    
    pub const fn to_phys(self, page_size: usize) -> usize {
        self.0 * page_size
    }
    
    pub const fn from_phys(addr: usize, page_size: usize) -> Self {
        Self(addr / page_size)
    }
}

/// Virtual page number
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Vpn(pub usize);

impl Vpn {
    pub const fn new(num: usize) -> Self {
        Self(num)
    }
}

/// Page table entry flags
bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct PageFlags: u64 {
        const PRESENT = 1 << 0;
        const WRITABLE = 1 << 1;
        const USER = 1 << 2;
        const NO_CACHE = 1 << 3;
        const GLOBAL = 1 << 4;
        const ACCESSED = 1 << 5;
        const DIRTY = 1 << 6;
        const HUGE = 1 << 7;
    }
}

/// Memory protection flags
bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct ProtFlags: u32 {
        const PROT_NONE = 0x0;
        const PROT_READ = 0x1;
        const PROT_WRITE = 0x2;
        const PROT_EXEC = 0x4;
    }
}

/// Memory mapping flags
bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct MapFlags: u32 {
        const MAP_SHARED = 0x01;
        const MAP_PRIVATE = 0x02;
        const MAP_FIXED = 0x10;
        const MAP_ANONYMOUS = 0x20;
        const MAP_GROWSDOWN = 0x100;
        const MAP_LOCKED = 0x2000;
        const MAP_NORESERVE = 0x4000;
    }
}

/// Physical memory manager trait
pub trait PhysicalMemoryManager: Send + Sync {
    /// Allocate a single physical page
    fn alloc_page(&self) -> Result<Pfn, MmError>;
    
    /// Allocate multiple contiguous physical pages
    fn alloc_pages(&self, count: usize) -> Result<Pfn, MmError>;
    
    /// Free a physical page
    fn free_page(&self, pfn: Pfn) -> Result<(), MmError>;
    
    /// Free multiple physical pages
    fn free_pages(&self, pfn: Pfn, count: usize) -> Result<(), MmError>;
    
    /// Get total physical memory in bytes
    fn total_memory(&self) -> usize;
    
    /// Get available physical memory in bytes
    fn available_memory(&self) -> usize;
}

/// Virtual memory manager trait
pub trait VirtualMemoryManager: Send + Sync {
    /// Map a virtual address to a physical frame
    fn map(&self, vpn: Vpn, pfn: Pfn, flags: PageFlags) -> Result<(), MmError>;
    
    /// Unmap a virtual address
    fn unmap(&self, vpn: Vpn) -> Result<(), MmError>;
    
    /// Update page flags
    fn protect(&self, vpn: Vpn, flags: PageFlags) -> Result<(), MmError>;
    
    /// Translate virtual address to physical
    fn translate(&self, vpn: Vpn) -> Option<Pfn>;
    
    /// Flush TLB for a specific address
    fn flush_tlb_one(&self, vpn: Vpn);
    
    /// Flush entire TLB
    fn flush_tlb_all(&self);
}

/// Memory policy for NUMA-aware allocation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryPolicy {
    /// Default policy - allocate on local node
    Local,
    /// Bind to specific NUMA node
    Bind { node_id: usize },
    /// Interleave across nodes
    Interleave { nodes: Vec<usize> },
    /// Prefer specific node, fallback to others
    Preferred { node_id: usize },
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self::Local
    }
}

/// NUMA node information
#[derive(Debug, Clone)]
pub struct NumaNode {
    pub id: usize,
    pub start_pfn: Pfn,
    pub end_pfn: Pfn,
    pub memory_size: usize,
}

/// Memory policy abstraction for NUMA support
pub struct MemoryPolicyManager {
    default_policy: spin::Mutex<MemoryPolicy>,
    numa_nodes: spin::Mutex<Vec<NumaNode>>,
}

impl MemoryPolicyManager {
    pub const fn new() -> Self {
        Self {
            default_policy: spin::Mutex::new(MemoryPolicy::Local),
            numa_nodes: spin::Mutex::new(Vec::new()),
        }
    }
    
    /// Set the default memory policy
    pub fn set_default_policy(&self, policy: MemoryPolicy) {
        *self.default_policy.lock() = policy;
    }
    
    /// Get the current default policy
    pub fn get_default_policy(&self) -> MemoryPolicy {
        self.default_policy.lock().clone()
    }
    
    /// Register a NUMA node
    pub fn register_numa_node(&self, node: NumaNode) {
        let mut nodes = self.numa_nodes.lock();
        nodes.push(node);
    }
    
    /// Get NUMA node by ID
    pub fn get_numa_node(&self, id: usize) -> Option<NumaNode> {
        let nodes = self.numa_nodes.lock();
        nodes.iter().find(|n| n.id == id).cloned()
    }
    
    /// Select a node based on policy
    pub fn select_node(&self, policy: &MemoryPolicy) -> Option<usize> {
        match policy {
            MemoryPolicy::Local => Some(0),
            MemoryPolicy::Bind { node_id } => Some(*node_id),
            MemoryPolicy::Interleave { nodes } => {
                if nodes.is_empty() {
                    None
                } else {
                    // Simple round-robin simulation
                    Some(nodes[0])
                }
            }
            MemoryPolicy::Preferred { node_id } => Some(*node_id),
        }
    }
}

// Global memory policy manager
pub static MEMORY_POLICY_MANAGER: MemoryPolicyManager = MemoryPolicyManager::new();

/// Helper macro for memory operation results
#[macro_export]
macro_rules! mm_result {
    ($expr:expr) => {
        match $expr {
            Ok(v) => Ok(v),
            Err(e) => Err(e.into()),
        }
    };
}
