//! ACPI NUMA topology — SRAT + SLIT table parsers.
//!
//! ## Tables used
//!
//! - **SRAT** (System Resource Affinity Table, ACPI 6.5 §5.2.16) Maps CPUs (LAPIC / x2APIC) and
//!   memory ranges to proximity domains (NUMA nodes).
//! - **SLIT** (System Locality Information Table, ACPI 6.5 §5.2.17) Provides a symmetric distance
//!   matrix between all NUMA nodes.
//!
//! ## Topology model
//!
//! We expose:
//! - `MAX_NODES` proximity domains, each with a list of associated memory ranges and a bitmask of
//!   LAPIC IDs.
//! - A flat distance matrix indexed `[from][to]` where 10 = local access.
//!
//! ## Thread Safety
//!
//! All initialization must complete before SMP is enabled. After initialization,
//! read-only access is safe from multiple threads.

use super::SdtHeader;
use crate::println;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Once;

pub const MAX_NODES: usize = 8;
const MAX_MEM_RANGES: usize = 16;
const SRAT_DISTANCE_LOCAL: u8 = 10;

const SRAT_TYPE_LAPIC: u8 = 0;
const SRAT_TYPE_MEM: u8 = 1;
const SRAT_TYPE_X2APIC: u8 = 2;

#[derive(Copy, Clone, Default, Debug)]
pub struct MemRange {
    pub base: u64,
    pub len: u64,
    /// Hot-pluggable memory range (SRAT flag bit 1).
    pub hotpluggable: bool,
    /// Non-volatile / persistent memory (SRAT flag bit 2).
    pub persistent: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct NumaNode {
    /// Proximity domain identifier.
    pub domain: u32,
    pub mem_ranges: [MemRange; MAX_MEM_RANGES],
    pub mem_range_cnt: usize,
    /// Bitmask of LAPIC IDs assigned to this node (up to 64 CPUs per node).
    pub lapic_mask: u64,
    /// Set to true if at least one enabled LAPIC or memory range was found.
    pub present: bool,
}

impl NumaNode {
    const fn empty() -> Self {
        Self {
            domain: 0,
            mem_ranges: [MemRange {
                base: 0,
                len: 0,
                hotpluggable: false,
                persistent: false,
            }; MAX_MEM_RANGES],
            mem_range_cnt: 0,
            lapic_mask: 0,
            present: false,
        }
    }
}

/// NUMA node storage with proper synchronization.
/// Initialized once during boot, then read-only.
static NODES: Once<[NumaNode; MAX_NODES]> = Once::new();
static NODE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Distance matrix: `DISTANCES[i][j]` is the relative latency from node `i`
/// to node `j`.  Initialised to 10 (local) on the diagonal, 20 everywhere
/// else; overwritten by `parse_slit()` when the table is present.
static DISTANCES: Once<[[u8; MAX_NODES]; MAX_NODES]> = Once::new();

/// Get mutable reference to NODES array during initialization.
/// 
/// # Safety
/// - Must only be called during single-threaded boot before SMP
/// - NODES must not have been initialized yet
unsafe fn get_nodes_mut() -> &'static mut [NumaNode; MAX_NODES] {
    NODES.get_or_try_init(|| {
        let mut nodes = [NumaNode::empty(); MAX_NODES];
        Ok::<[NumaNode; MAX_NODES], ()>(nodes)
    }).unwrap()
}

/// Get mutable reference to DISTANCES array during initialization.
/// 
/// # Safety
/// - Must only be called during single-threaded boot before SMP
/// - DISTANCES must not have been initialized yet
unsafe fn get_distances_mut() -> &'static mut [[u8; MAX_NODES]; MAX_NODES] {
    DISTANCES.get_or_try_init(|| {
        let mut d = [[20u8; MAX_NODES]; MAX_NODES];
        for i in 0..MAX_NODES {
            d[i][i] = SRAT_DISTANCE_LOCAL;
        }
        Ok::<[[u8; MAX_NODES]; MAX_NODES], ()>(d)
    }).unwrap()
}

unsafe fn node_for_domain(domain: u32) -> Option<&'static mut NumaNode> {
    let count = NODE_COUNT.load(Ordering::Relaxed);
    let nodes = get_nodes_mut();
    
    // Look for existing entry.
    for n in &mut nodes[..count] {
        if n.domain == domain {
            return Some(n);
        }
    }
    // Allocate a new slot.
    if count >= MAX_NODES {
        return None;
    }
    nodes[count].domain = domain;
    nodes[count].present = true;
    NODE_COUNT.store(count + 1, Ordering::Relaxed);
    Some(&mut nodes[count])
}

#[repr(C, packed)]
struct SratLapic {
    kind: u8,
    len: u8,
    prox_lo: u8, // bits [7:0] of proximity domain
    lapic_id: u8,
    flags: u32,
    sapic_eid: u8,
    prox_hi: [u8; 3], // bits [31:8] of proximity domain
    _clk: u32,
}

#[repr(C, packed)]
struct SratMem {
    kind: u8,
    len: u8,
    prox_dom: u32,
    _rsvd: u16,
    base_lo: u32,
    base_hi: u32,
    len_lo: u32,
    len_hi: u32,
    _rsvd2: u32,
    flags: u32,
    _rsvd3: u64,
}

#[repr(C, packed)]
struct SratX2Apic {
    kind: u8,
    len: u8,
    _rsvd: u16,
    prox_dom: u32,
    x2apic_id: u32,
    flags: u32,
    _clk: u32,
    _rsvd2: u32,
}

/// Parse the SRAT table to discover NUMA topology.
///
/// # Safety
/// - Must be called after `super::init()` during single-threaded boot
/// - Must complete before SMP initialization
pub unsafe fn parse_srat() {
    let hdr = match super::find_table(b"SRAT") {
        Some(p) => p,
        None => {
            println!("acpi/numa: no SRAT — assuming single node");
            // Populate a synthetic node 0 with no memory ranges.
            let nodes = get_nodes_mut();
            nodes[0].domain = 0;
            nodes[0].present = true;
            NODE_COUNT.store(1, Ordering::Relaxed);
            return;
        },
    };

    let total = (*hdr).len as usize;
    // SRAT header = SdtHeader (36) + 4 (reserved) + 8 (reserved2) = 48 bytes.
    let body_off = core::mem::size_of::<SdtHeader>() + 12;
    if total <= body_off {
        println!("acpi/numa: SRAT too small");
        return;
    }

    let base = hdr as usize;
    let end = base + total;
    let mut p = base + body_off;

    while p + 2 <= end {
        let kind = *(p as *const u8);
        let len = *((p + 1) as *const u8) as usize;
        if len < 2 || p + len > end {
            break;
        }

        match kind {
            SRAT_TYPE_LAPIC => {
                if len < core::mem::size_of::<SratLapic>() {
                    p += len;
                    continue;
                }
                let e = &*(p as *const SratLapic);
                let flags = e.flags;
                if flags & 1 == 0 {
                    p += len;
                    continue;
                } // not enabled
                let domain = e.prox_lo as u32
                    | ((e.prox_hi[0] as u32) << 8)
                    | ((e.prox_hi[1] as u32) << 16)
                    | ((e.prox_hi[2] as u32) << 24);
                let lid = e.lapic_id;
                if let Some(node) = node_for_domain(domain) {
                    if lid < 64 {
                        node.lapic_mask |= 1u64 << lid;
                    }
                }
            },
            SRAT_TYPE_MEM => {
                if len < core::mem::size_of::<SratMem>() {
                    p += len;
                    continue;
                }
                let e = &*(p as *const SratMem);
                if e.flags & 1 == 0 {
                    p += len;
                    continue;
                } // not enabled
                let domain = e.prox_dom;
                let base_pa = (e.base_hi as u64) << 32 | e.base_lo as u64;
                let range_len = (e.len_hi as u64) << 32 | e.len_lo as u64;
                let hotplug = e.flags & (1 << 1) != 0;
                let persist = e.flags & (1 << 2) != 0;
                if let Some(node) = node_for_domain(domain) {
                    let idx = node.mem_range_cnt;
                    if idx < MAX_MEM_RANGES {
                        node.mem_ranges[idx] = MemRange {
                            base: base_pa,
                            len: range_len,
                            hotpluggable: hotplug,
                            persistent: persist,
                        };
                        node.mem_range_cnt += 1;
                    }
                }
            },
            SRAT_TYPE_X2APIC => {
                if len < core::mem::size_of::<SratX2Apic>() {
                    p += len;
                    continue;
                }
                let e = &*(p as *const SratX2Apic);
                if e.flags & 1 == 0 {
                    p += len;
                    continue;
                }
                let domain = e.prox_dom;
                let xid = e.x2apic_id;
                if let Some(node) = node_for_domain(domain) {
                    if xid < 64 {
                        node.lapic_mask |= 1u64 << xid;
                    }
                }
            },
            _ => {},
        }
        p += len;
    }

    let count = NODE_COUNT.load(Ordering::Relaxed);
    println!(
        "acpi/numa: {} NUMA node(s) discovered from SRAT",
        count
    );
    
    // Ensure NODES is initialized before reading
    let _ = get_nodes_mut();
    
    let nodes = NODES.get().unwrap();
    for i in 0..count {
        let n = &nodes[i];
        println!(
            "  Node {}  lapic_mask={:#018x}  {} mem range(s)",
            n.domain, n.lapic_mask, n.mem_range_cnt
        );
        for r in 0..n.mem_range_cnt {
            let mr = &n.mem_ranges[r];
            println!(
                "    [{:#018x} + {:#010x})  hp={}  persist={}",
                mr.base, mr.len, mr.hotpluggable, mr.persistent
            );
        }
    }
}

pub unsafe fn parse_slit() {
    let hdr = match super::find_table(b"SLIT") {
        Some(p) => p,
        None => {
            println!("acpi/numa: no SLIT, using default distances");
            return;
        },
    };

    let total = (*hdr).len as usize;
    // SLIT header = SdtHeader (36) + 8 (locality_count: u64) = 44 bytes.
    let body_off = core::mem::size_of::<SdtHeader>();
    if total < body_off + 8 {
        return;
    }

    let count_ptr = (hdr as usize + body_off) as *const u64;
    let locality_count = count_ptr.read_unaligned() as usize;
    let matrix_off = body_off + 8;
    let matrix_bytes = total.saturating_sub(matrix_off);
    let expected = locality_count * locality_count;

    if matrix_bytes < expected {
        println!(
            "acpi/numa: SLIT matrix too small ({} < {})",
            matrix_bytes, expected
        );
        return;
    }

    let matrix = (hdr as usize + matrix_off) as *const u8;
    let n = locality_count.min(MAX_NODES);
    
    // Get mutable reference to distances during init
    let distances = get_distances_mut();
    for i in 0..n {
        for j in 0..n {
            distances[i][j] = *matrix.add(i * locality_count + j);
        }
    }
    println!("acpi/numa: SLIT {}×{} distance matrix loaded", n, n);
    // Print first row as a quick sanity check.
    let row: &[u8] = core::slice::from_raw_parts(matrix, n);
    print_distance_row(0, row);
}

fn print_distance_row(node: usize, row: &[u8]) {
    crate::kprint!("  node {} distances: ", node);
    for d in row {
        crate::kprint!("{:3} ", d);
    }
    println!();
}

/// Initialise NUMA topology (must be called after `super::init()`).
pub unsafe fn init() {
    parse_srat();
    parse_slit();
}

/// Number of discovered NUMA nodes.
pub fn node_count() -> usize {
    NODE_COUNT.load(Ordering::Acquire)
}

/// Immutable reference to all discovered nodes.
/// 
/// # Panics
/// Panics if called before `init()` has initialized the NUMA subsystem.
pub fn nodes() -> &'static [NumaNode] {
    // Ensure initialization has completed
    let _ = NODES.get().expect("NUMA nodes not initialized");
    let count = NODE_COUNT.load(Ordering::Acquire);
    unsafe { 
        core::slice::from_raw_parts(NODES.get().unwrap().as_ptr(), count)
    }
}

/// Relative access distance from `from` to `to`.
/// Returns 10 for local, higher values for remote.
/// 
/// # Panics
/// Panics if called before `init()` has initialized the NUMA subsystem.
pub fn distance(from: usize, to: usize) -> u8 {
    if from >= MAX_NODES || to >= MAX_NODES {
        return u8::MAX;
    }
    let distances = DISTANCES.get().expect("NUMA distances not initialized");
    distances[from][to]
}

/// Return the NUMA node that owns the physical address `pa`, if known.
pub fn node_for_phys(pa: u64) -> Option<usize> {
    let nodes = nodes();
    for (idx, node) in nodes.iter().enumerate() {
        for r in 0..node.mem_range_cnt {
            let mr = &node.mem_ranges[r];
            if pa >= mr.base && pa < mr.base + mr.len {
                return Some(idx);
            }
        }
    }
    None
}

/// Return the NUMA node that owns the given LAPIC ID, if known.
pub fn node_for_lapic(lapic_id: u8) -> Option<usize> {
    if lapic_id >= 64 {
        return None;
    }
    let mask = 1u64 << lapic_id;
    let nodes = nodes();
    for (idx, node) in nodes.iter().enumerate() {
        if node.lapic_mask & mask != 0 {
            return Some(idx);
        }
    }
    None
}
