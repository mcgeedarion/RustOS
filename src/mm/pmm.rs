//! Boot-map-sized PMM with serialized buddy lists and a contiguous boot pool.
//! init_from_regions must run before APs or concurrent memory-map updates.
//! This retains the existing identity-addressable physical-frame convention.
#![allow(unexpected_cfgs)] // pmm_host_test is supplied only by the standalone harness.
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use crate::allocator::InterruptGuard;
use crate::mm::boot_memory::Regions;

pub const PAGE_SIZE: usize = 4096;
pub const MAX_ORDER: usize = 11;
pub const MAX_NODES: usize = 8;
pub const MAX_PA: usize = 64 * 1024 * 1024 * 1024;
const MAX_FRAMES: usize = MAX_PA / PAGE_SIZE;
const POOL_PAGES: usize = 16_384;
static PMM_LOCK: spin::Mutex<()> = spin::Mutex::new(());

#[repr(C, align(4096))]
struct Pool(UnsafeCell<[u8; POOL_PAGES * PAGE_SIZE]>);
// Allocation is serialized by PMM_LOCK; disjoint issued pages belong to callers.
unsafe impl Sync for Pool {}
static POOL: Pool = Pool(UnsafeCell::new([0; POOL_PAGES * PAGE_SIZE]));
static BUMP: AtomicUsize = AtomicUsize::new(0);
static BOOT_FREE_HEAD: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static BOOT_FREE_CNT: AtomicUsize = AtomicUsize::new(0);
static POOL_FREE_BITS: [core::sync::atomic::AtomicU64; POOL_PAGES / 64] = {
    const Z: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
    [Z; POOL_PAGES / 64]
};
fn pool_base() -> usize { POOL.0.get() as *mut u8 as usize }
fn pool_index(pa: usize) -> Option<usize> {
    let base = pool_base();
    (pa >= base && pa - base < POOL_PAGES * PAGE_SIZE).then(|| (pa - base) / PAGE_SIZE)
}
fn pool_bit_set_free(index: usize) -> bool {
    let bit = 1u64 << (index % 64);
    POOL_FREE_BITS[index / 64].fetch_or(bit, Ordering::Relaxed) & bit == 0
}
fn boot_push(pa: usize) {
    unsafe { (pa as *mut *mut u8).write(BOOT_FREE_HEAD.load(Ordering::Relaxed)) };
    BOOT_FREE_HEAD.store(pa as *mut u8, Ordering::Relaxed);
    BOOT_FREE_CNT.fetch_add(1, Ordering::Relaxed);
}
fn boot_pop() -> usize {
    let head = BOOT_FREE_HEAD.load(Ordering::Relaxed);
    if head.is_null() { return 0; }
    let next = unsafe { (head as *const *mut u8).read() };
    BOOT_FREE_HEAD.store(next, Ordering::Relaxed);
    BOOT_FREE_CNT.fetch_sub(1, Ordering::Relaxed);
    if let Some(index) = pool_index(head as usize) {
        POOL_FREE_BITS[index / 64].fetch_and(!(1u64 << (index % 64)), Ordering::Relaxed);
    }
    head as usize
}
// Called only under PMM_LOCK. A failed request never advances the cursor.
fn reserve_boot_pages(pages: usize) -> Option<usize> {
    let current = BUMP.load(Ordering::Relaxed);
    let end = current.checked_add(pages)?;
    if pages == 0 || end > POOL_PAGES { return None; }
    BUMP.store(end, Ordering::Relaxed);
    Some(pool_base() + current * PAGE_SIZE)
}
fn alloc_boot_pages(pages: usize) -> Option<usize> {
    let pa = if pages == 1 {
        let recycled = boot_pop();
        if recycled != 0 { recycled } else { reserve_boot_pages(1)? }
    } else { reserve_boot_pages(pages)? };
    unsafe { zero_pages(pa, pages) };
    Some(pa)
}

pub mod page_flags {
    pub const FLAG_FREE: u8 = 1;
    pub const FLAG_RESERVED: u8 = 2;
    pub const FLAG_BUDDY: u8 = 4;
    pub const FLAG_BOOT: u8 = 8;
}
use page_flags::*;
#[repr(C, align(8))]
pub struct PageInfo {
    pub refcount: AtomicU32, pub flags: AtomicU8, pub order: AtomicU8,
    pub numa_node: AtomicU8, pub _pad: u8, pub free_next: AtomicPtr<PageInfo>,
}
impl PageInfo {
    const fn zero() -> Self {
        Self { refcount: AtomicU32::new(0), flags: AtomicU8::new(0),
            order: AtomicU8::new(0), numa_node: AtomicU8::new(0), _pad: 0,
            free_next: AtomicPtr::new(core::ptr::null_mut()) }
    }
}
static PAGE_INFO_PTR: AtomicPtr<PageInfo> = AtomicPtr::new(core::ptr::null_mut());
static PAGE_INFO_LEN: AtomicUsize = AtomicUsize::new(0);
const EMERGENCY_FRAMES: usize = (1024 * 1024 * 1024) / PAGE_SIZE;
static EMERGENCY_TABLE: [PageInfo; EMERGENCY_FRAMES] = {
    const Z: PageInfo = PageInfo::zero(); [Z; EMERGENCY_FRAMES]
};
// Serialized, immutable in size after first publication: PageInfo pointers must
// remain stable. Unsupported late growth fails explicitly rather than dropping RAM.
fn ensure_page_info_table(max_pfn: usize) {
    let needed = max_pfn.checked_add(1).expect("pmm: PFN overflow");
    let existing = PAGE_INFO_LEN.load(Ordering::Acquire);
    if existing != 0 {
        assert!(needed <= existing, "pmm: metadata growth requires full-map initialization");
        return;
    }
    let bytes = needed.checked_mul(core::mem::size_of::<PageInfo>()).expect("pmm: metadata overflow");
    let pages = bytes.div_ceil(PAGE_SIZE);
    let ptr = if let Some(base) = reserve_boot_pages(pages) {
        unsafe { core::ptr::write_bytes(base as *mut u8, 0, pages * PAGE_SIZE) };
        base as *mut PageInfo
    } else {
        assert!(needed <= EMERGENCY_FRAMES, "pmm: insufficient bootstrap metadata capacity");
        EMERGENCY_TABLE.as_ptr() as *mut PageInfo
    };
    // Metadata pool pages stay allocated and are never added to BOOT_FREE_HEAD.
    let len = if ptr == EMERGENCY_TABLE.as_ptr() as *mut PageInfo { EMERGENCY_FRAMES } else { needed };
    PAGE_INFO_LEN.store(len, Ordering::Relaxed);
    PAGE_INFO_PTR.store(ptr, Ordering::Release);
}
fn pfn(pa: usize) -> usize { pa / PAGE_SIZE }
pub fn page_info(pa: usize) -> Option<&'static PageInfo> {
    if pa == 0 || pa & (PAGE_SIZE - 1) != 0 { return None; }
    let ptr = PAGE_INFO_PTR.load(Ordering::Acquire);
    if ptr.is_null() || pfn(pa) >= PAGE_INFO_LEN.load(Ordering::Relaxed) { return None; }
    Some(unsafe { &*ptr.add(pfn(pa)) })
}
#[derive(Clone, Copy)]
struct NodeRange { base: usize, end: usize }
static NODE_RANGES: [spin::Mutex<NodeRange>; MAX_NODES] = {
    const Z: spin::Mutex<NodeRange> = spin::Mutex::new(NodeRange { base: usize::MAX, end: 0 });
    [Z; MAX_NODES]
};
static NODE_COUNT: AtomicUsize = AtomicUsize::new(0);
fn register_node_range(node: u8, base: usize, end: usize) {
    let mut range = NODE_RANGES[node as usize].lock();
    range.base = range.base.min(base); range.end = range.end.max(end);
    NODE_COUNT.fetch_max(node as usize + 1, Ordering::Relaxed);
}
pub fn node_of(pa: usize) -> u8 {
    for node in 0..NODE_COUNT.load(Ordering::Relaxed) {
        let range = NODE_RANGES[node].lock();
        if pa >= range.base && pa < range.end { return node as u8; }
    }
    0
}
struct BuddyNode {
    free_lists: [AtomicPtr<PageInfo>; MAX_ORDER], free_pages: AtomicUsize, total_pages: AtomicUsize,
}
impl BuddyNode {
    const fn new() -> Self {
        const NP: AtomicPtr<PageInfo> = AtomicPtr::new(core::ptr::null_mut());
        Self { free_lists: [NP; MAX_ORDER], free_pages: AtomicUsize::new(0), total_pages: AtomicUsize::new(0) }
    }
}
static BUDDY: [BuddyNode; MAX_NODES] = { const Z: BuddyNode = BuddyNode::new(); [Z; MAX_NODES] };
static BUDDY_LIVE: AtomicBool = AtomicBool::new(false);
static BUILDING_MAP: AtomicBool = AtomicBool::new(false);
static INIT_REGIONS: spin::Once<()> = spin::Once::new();
unsafe fn pi_to_pa(pi: *const PageInfo) -> usize {
    let base = PAGE_INFO_PTR.load(Ordering::Relaxed);
    (pi as usize - base as usize) / core::mem::size_of::<PageInfo>() * PAGE_SIZE
}
fn order_size(order: usize) -> usize { PAGE_SIZE << order }
// All buddy and boot-list manipulation is covered by PMM_LOCK.
unsafe fn buddy_push(node: u8, pa: usize, order: usize) {
    let pi = page_info(pa).expect("pmm: free block lacks metadata");
    let buddy = &BUDDY[node as usize];
    pi.flags.store(FLAG_FREE | FLAG_BUDDY, Ordering::Relaxed);
    pi.order.store(order as u8, Ordering::Relaxed);
    pi.free_next.store(buddy.free_lists[order].load(Ordering::Relaxed), Ordering::Relaxed);
    buddy.free_lists[order].store(pi as *const _ as *mut _, Ordering::Relaxed);
    buddy.free_pages.fetch_add(1 << order, Ordering::Relaxed);
}
unsafe fn buddy_pop(node: u8, order: usize) -> usize {
    let buddy = &BUDDY[node as usize];
    let head = buddy.free_lists[order].load(Ordering::Relaxed);
    if head.is_null() { return 0; }
    buddy.free_lists[order].store((*head).free_next.load(Ordering::Relaxed), Ordering::Relaxed);
    (*head).flags.fetch_and(!FLAG_FREE, Ordering::Relaxed);
    buddy.free_pages.fetch_sub(1 << order, Ordering::Relaxed);
    pi_to_pa(head)
}
// Unlink in place: no bounded scratch buffer, restoration race, or ABA loop.
unsafe fn buddy_remove(node: u8, pa: usize, order: usize) -> bool {
    let Some(target) = page_info(pa) else { return false };
    let target = target as *const _ as *mut PageInfo;
    let buddy = &BUDDY[node as usize];
    let mut link = &buddy.free_lists[order];
    loop {
        let current = link.load(Ordering::Relaxed);
        if current.is_null() { return false; }
        if current == target {
            link.store((*current).free_next.load(Ordering::Relaxed), Ordering::Relaxed);
            (*current).flags.fetch_and(!FLAG_FREE, Ordering::Relaxed);
            buddy.free_pages.fetch_sub(1 << order, Ordering::Relaxed);
            return true;
        }
        link = &(*current).free_next;
    }
}
#[cfg(not(pmm_host_test))]
fn kernel_phys_range() -> (usize, usize) {
    extern "C" { static _kernel_start: u8; static _end: u8; }
    let offset = crate::mm::phys::PHYS_OFFSET;
    let convert = |va: usize| if va >= offset { va - offset } else { va };
    unsafe { (convert(core::ptr::addr_of!(_kernel_start) as usize), convert(core::ptr::addr_of!(_end) as usize)) }
}
fn is_kernel_page(pa: usize) -> bool {
    #[cfg(not(pmm_host_test))]
    { let (start, end) = kernel_phys_range(); pa < end && pa.saturating_add(PAGE_SIZE) > start }
    #[cfg(pmm_host_test)]
    { let _ = pa; false }
}
#[cfg(pmm_host_test)]
static TEST_MEMORY_BASE: AtomicUsize = AtomicUsize::new(0);
#[cfg(pmm_host_test)]
static TEST_MEMORY_LEN: AtomicUsize = AtomicUsize::new(0);
unsafe fn zero_pages(pa: usize, pages: usize) {
    #[cfg(pmm_host_test)]
    let address = if pa < TEST_MEMORY_LEN.load(Ordering::Relaxed) {
        TEST_MEMORY_BASE.load(Ordering::Relaxed) + pa
    } else { pa };
    #[cfg(not(pmm_host_test))]
    let address = pa;
    core::ptr::write_bytes(address as *mut u8, 0, pages * PAGE_SIZE);
}
unsafe fn buddy_alloc_order_node(node: u8, wanted: usize) -> usize {
    for order in wanted..MAX_ORDER {
        let pa = buddy_pop(node, order);
        if pa == 0 { continue; }
        for split in (wanted..order).rev() { buddy_push(node, pa + order_size(split), split); }
        for page in 0..(1usize << wanted) {
            let pi = page_info(pa + page * PAGE_SIZE).unwrap();
            pi.refcount.store(1, Ordering::Relaxed); pi.flags.store(FLAG_BUDDY, Ordering::Relaxed);
            pi.order.store(wanted as u8, Ordering::Relaxed); pi.numa_node.store(node, Ordering::Relaxed);
        }
        zero_pages(pa, 1 << wanted);
        return pa;
    }
    0
}
unsafe fn buddy_free_page(pa: usize) {
    let pi = page_info(pa).expect("pmm: freed page lacks metadata");
    let node = pi.numa_node.load(Ordering::Relaxed);
    let mut base = pa;
    let mut order = 0;
    zero_pages(pa, 1);
    pi.refcount.store(0, Ordering::Relaxed); pi.flags.store(FLAG_BUDDY, Ordering::Relaxed);
    while order + 1 < MAX_ORDER {
        let other = base ^ order_size(order);
        let Some(info) = page_info(other) else { break };
        if info.flags.load(Ordering::Relaxed) & FLAG_FREE == 0
            || info.order.load(Ordering::Relaxed) as usize != order
            || info.numa_node.load(Ordering::Relaxed) != node { break; }
        if !buddy_remove(node, other, order) { break; }
        base = base.min(other); order += 1;
    }
    buddy_push(node, base, order);
}
fn local_node() -> u8 {
    #[cfg(all(target_arch = "x86_64", not(pmm_host_test)))]
    {
        let cpu = crate::arch::x86_64::gdt::current_cpu_id();
        if let Some(info) = crate::smp::cpu_info(cpu) { return info.node as u8; }
    }
    0
}
pub fn alloc_page() -> Option<usize> {
    // GS/per-CPU state is not available during pre-GDT bootstrap allocation.
    let node = if BUDDY_LIVE.load(Ordering::Acquire) { local_node() } else { 0 };
    alloc_page_on_node(node)
}
pub fn alloc_page_on_node(preferred: u8) -> Option<usize> {
    let _interrupts = InterruptGuard::new(); let _lock = PMM_LOCK.lock();
    if BUDDY_LIVE.load(Ordering::Acquire) {
        let nodes = NODE_COUNT.load(Ordering::Relaxed).max(1);
        for offset in 0..nodes {
            let node = ((preferred as usize + offset) % nodes) as u8;
            let pa = unsafe { buddy_alloc_order_node(node, 0) };
            if pa != 0 { return Some(pa); }
        }
    }
    alloc_boot_pages(1)
}
pub fn alloc_pages_contig(n: usize) -> Option<usize> {
    if n == 0 || n > (1usize << (MAX_ORDER - 1)) { return None; }
    let _interrupts = InterruptGuard::new(); let _lock = PMM_LOCK.lock();
    if BUDDY_LIVE.load(Ordering::Acquire) {
        let order = n.next_power_of_two().trailing_zeros() as usize;
        let nodes = NODE_COUNT.load(Ordering::Relaxed).max(1);
        let preferred = local_node();
        for offset in 0..nodes {
            let node = ((preferred as usize + offset) % nodes) as u8;
            let pa = unsafe { buddy_alloc_order_node(node, order) };
            if pa == 0 { continue; }
            for tail in n..(1usize << order) { unsafe { buddy_free_page(pa + tail * PAGE_SIZE) }; }
            return Some(pa);
        }
    }
    alloc_boot_pages(n)
}
pub fn free_pages_contig(base_pa: usize, n: usize) {
    for page in 0..n { free_page(base_pa + page * PAGE_SIZE); }
}
pub fn free_page(pa: usize) {
    if pa == 0 { return; }
    let _interrupts = InterruptGuard::new(); let _lock = PMM_LOCK.lock();
    assert_eq!(pa & (PAGE_SIZE - 1), 0, "pmm: unaligned free");
    // Bootstrap pages belong to the kernel image but are explicitly allocatable.
    if let Some(index) = pool_index(pa) {
        assert!(index < BUMP.load(Ordering::Relaxed), "pmm: unissued bootstrap page");
        assert!(pool_bit_set_free(index), "pmm: duplicate bootstrap free");
        unsafe { zero_pages(pa, 1) }; boot_push(pa); return;
    }
    assert!(!is_kernel_page(pa), "pmm: attempted kernel-image free");
    let pi = page_info(pa).expect("pmm: unknown physical page");
    let refs = pi.refcount.load(Ordering::Relaxed);
    assert!(refs > 0, "pmm: page refcount underflow");
    if refs == 1 { unsafe { buddy_free_page(pa) }; }
    else { pi.refcount.store(refs - 1, Ordering::Relaxed); }
}
pub fn get_page(pa: usize) {
    let _interrupts = InterruptGuard::new(); let _lock = PMM_LOCK.lock();
    if let Some(pi) = page_info(pa) { pi.refcount.fetch_add(1, Ordering::Relaxed); }
}
pub fn put_page(pa: usize) { free_page(pa); }
pub fn page_refcount(pa: usize) -> u32 {
    page_info(pa).map(|pi| pi.refcount.load(Ordering::Relaxed)).unwrap_or(0)
}
// Page-aligned, exclusive-end usable span, clipped to the PMM's address limit.
fn usable_span(start: u64, length: u64) -> Option<(usize, usize)> {
    let end = start.checked_add(length)?.min(MAX_PA as u64) as usize & !(PAGE_SIZE - 1);
    if start >= MAX_PA as u64 { return None; }
    let start = (start as usize).checked_add(PAGE_SIZE - 1)? & !(PAGE_SIZE - 1);
    let start = start.max(PAGE_SIZE);
    (start < end).then_some((start, end))
}
fn seed_run(start: usize, end: usize, node: u8) {
    let mut pa = start;
    while pa < end {
        let remaining = (end - pa) / PAGE_SIZE;
        let alignment = (pa / PAGE_SIZE).trailing_zeros() as usize;
        let fitting = usize::BITS as usize - 1 - remaining.leading_zeros() as usize;
        let order = alignment.min(fitting).min(MAX_ORDER - 1);
        unsafe { buddy_push(node, pa, order) };
        pa += order_size(order);
    }
    BUDDY[node as usize].total_pages.fetch_add((end - start) / PAGE_SIZE, Ordering::Relaxed);
}
/// Boot-only region registration. Hot-add after publication is not supported.
pub fn pmm_add_region_node(base: usize, size: usize, node: u8) {
    let _interrupts = InterruptGuard::new(); let _lock = PMM_LOCK.lock();
    assert!((node as usize) < MAX_NODES, "pmm: invalid NUMA node");
    assert!(!BUDDY_LIVE.load(Ordering::Acquire), "pmm: late region registration is unsupported");
    let Some((start, end)) = usable_span(base as u64, size as u64) else { return };
    ensure_page_info_table(pfn(end - 1));
    register_node_range(node, start, end);
    let mut pa = start;
    while pa < end {
        let eligible = |address| !is_kernel_page(address) && pool_index(address).is_none()
            && page_info(address).is_some_and(|pi| pi.flags.load(Ordering::Relaxed) == 0);
        if !eligible(pa) { pa += PAGE_SIZE; continue; }
        let run_start = pa;
        while pa < end && eligible(pa) {
            let pi = page_info(pa).unwrap();
            pi.flags.store(FLAG_BUDDY, Ordering::Relaxed); pi.refcount.store(0, Ordering::Relaxed);
            pi.numa_node.store(node, Ordering::Relaxed); pi.order.store(0, Ordering::Relaxed);
            pa += PAGE_SIZE;
        }
        seed_run(run_start, pa, node);
    }
    if !BUILDING_MAP.load(Ordering::Relaxed) { BUDDY_LIVE.store(true, Ordering::Release); }
}
pub fn pmm_add_region(base: usize, size: usize) { pmm_add_region_node(base, size, 0); }
/// Initialize once from the full memory map before concurrent PMM use.
/// # Safety
/// Usable regions must describe exclusively available, identity-addressable
/// RAM. Initialization and all region registration must finish before AP use.
pub unsafe fn init_from_regions(regions: &Regions) {
    INIT_REGIONS.call_once(|| {
        let highest = regions.iter().filter(|r| r.is_usable())
            .filter_map(|r| usable_span(r.start, r.length).map(|(_, end)| end / PAGE_SIZE)).max();
        let Some(frames) = highest else { return };
        {
            let _interrupts = InterruptGuard::new(); let _lock = PMM_LOCK.lock();
            assert!(!BUDDY_LIVE.load(Ordering::Acquire), "pmm: map already published");
            assert!(frames <= MAX_FRAMES);
            ensure_page_info_table(frames - 1);
            BUILDING_MAP.store(true, Ordering::Relaxed);
        }
        for region in regions.iter().filter(|r| r.is_usable()) {
            if let Some((start, end)) = usable_span(region.start, region.length) {
                pmm_add_region_node(start, end - start, 0);
            }
        }
        BUILDING_MAP.store(false, Ordering::Relaxed);
        BUDDY_LIVE.store(true, Ordering::Release);
    });
}
pub fn free_pages() -> usize {
    let _interrupts = InterruptGuard::new(); let _lock = PMM_LOCK.lock();
    BUDDY.iter().map(|n| n.free_pages.load(Ordering::Relaxed)).sum::<usize>()
        + BOOT_FREE_CNT.load(Ordering::Relaxed) + POOL_PAGES - BUMP.load(Ordering::Relaxed)
}
pub fn total_pages() -> usize {
    BUDDY.iter().map(|n| n.total_pages.load(Ordering::Relaxed)).sum::<usize>() + POOL_PAGES
}
pub fn free_pages_node(node: u8) -> usize {
    BUDDY.get(node as usize).map(|n| n.free_pages.load(Ordering::Relaxed)).unwrap_or(0)
}
pub fn total_pages_node(node: u8) -> usize {
    BUDDY.get(node as usize).map(|n| n.total_pages.load(Ordering::Relaxed)).unwrap_or(0)
}
pub fn dump_stats() {
    for node in 0..NODE_COUNT.load(Ordering::Relaxed) {
        log::info!("pmm: node {}: {} / {} pages free", node, free_pages_node(node as u8), total_pages_node(node as u8));
    }
}
pub fn dump_freelist() {
    let mut counts = [[0usize; MAX_ORDER]; MAX_NODES];
    {
        let _interrupts = InterruptGuard::new(); let _lock = PMM_LOCK.lock();
        for node in 0..MAX_NODES {
            for order in 0..MAX_ORDER {
                let mut pi = BUDDY[node].free_lists[order].load(Ordering::Relaxed);
                while !pi.is_null() {
                    counts[node][order] += 1;
                    pi = unsafe { (*pi).free_next.load(Ordering::Relaxed) };
                }
            }
        }
    }
    // Logging occurs outside the PMM lock.
    for node in 0..MAX_NODES {
        for order in 0..MAX_ORDER {
            if counts[node][order] != 0 {
                log::info!("pmm: node={} order={} free_blocks={}", node, order, counts[node][order]);
            }
        }
    }
}
pub fn alloc_pages(n: usize) -> Option<usize> { alloc_pages_contig(n) }
pub fn alloc_pages_aligned(n: usize, align: usize) -> Option<core::ptr::NonNull<u8>> {
    if align == 0 || !align.is_power_of_two() {
        return core::ptr::NonNull::new(alloc_pages_contig(n)? as *mut u8);
    }
    let pa = alloc_pages_contig(n)?;
    if pa & (align - 1) == 0 { return core::ptr::NonNull::new(pa as *mut u8); }
    free_pages_contig(pa, n);
    let total = n.checked_add(align / PAGE_SIZE)?;
    let base = alloc_pages_contig(total)?;
    let Some(aligned) = base.checked_add(align - 1).map(|v| v & !(align - 1)) else {
        free_pages_contig(base, total); return None;
    };
    let prefix = (aligned - base) / PAGE_SIZE;
    if prefix > total || n > total - prefix { free_pages_contig(base, total); return None; }
    free_pages_contig(base, prefix);
    free_pages_contig(aligned + n * PAGE_SIZE, total - prefix - n);
    core::ptr::NonNull::new(aligned as *mut u8)
}
pub fn add_region(base: usize, size: usize) { pmm_add_region(base, size); }
pub fn free_page_table(pa: usize) { free_page(pa); }
#[cfg(all(test, pmm_host_test))]
#[path = "pmm/init_tests.rs"]
mod init_tests;
