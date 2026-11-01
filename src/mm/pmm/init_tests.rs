use super::*;
#[test]
fn spans_reject_overflow_and_keep_only_whole_supported_pages() {
    assert_eq!(usable_span(1, 3 * PAGE_SIZE as u64), Some((PAGE_SIZE, 3 * PAGE_SIZE)));
    assert_eq!(usable_span(u64::MAX - 1, 8), None);
    assert_eq!(usable_span(MAX_PA as u64, PAGE_SIZE as u64), None);
    assert_eq!(usable_span(100, 100), None);
}

#[test]
fn production_initialization_bootstrap_contiguity_and_accounting() {
    use crate::mm::boot_memory::{Region, RegionKind};
    let mut memory = vec![0xA5u8; 12 * 1024 * 1024];
    TEST_MEMORY_BASE.store(memory.as_mut_ptr() as usize, Ordering::Relaxed);
    TEST_MEMORY_LEN.store(memory.len(), Ordering::Relaxed);

    let baseline = free_pages();
    let early = alloc_pages_contig(3).expect("pre-GDT contiguous boot allocation");
    assert_eq!(early & (PAGE_SIZE - 1), 0);
    assert_eq!(free_pages(), baseline - 3);
    free_pages_contig(early, 3);
    assert_eq!(free_pages(), baseline);
    {
        let _lock = PMM_LOCK.lock();
        let cursor = BUMP.load(Ordering::Relaxed);
        assert!(reserve_boot_pages(POOL_PAGES + 1).is_none());
        assert_eq!(BUMP.load(Ordering::Relaxed), cursor);
    }

    let mut regions = Regions::new();
    // A small low region must not truncate metadata for later high RAM.
    regions.push(Region { start: 0x100000, length: 0x100000, kind: RegionKind::Usable });
    regions.push(Region { start: 0x400000, length: 0x600000, kind: RegionKind::Usable });
    // Overlap must not seed the same physical page twice.
    regions.push(Region { start: 0x400000, length: 0x100000, kind: RegionKind::Usable });
    regions.push(Region { start: 0x200000, length: 0x200000, kind: RegionKind::Reserved });
    unsafe { init_from_regions(&regions) };
    assert!(BUDDY_LIVE.load(Ordering::Acquire));
    assert!(PAGE_INFO_LEN.load(Ordering::Relaxed) >= 0xA00000 / PAGE_SIZE);
    assert_eq!(total_pages_node(0), 1792);
    assert_eq!(free_pages_node(0), 1792);

    let before = free_pages();
    let block = alloc_pages_contig(3).expect("higher-order lists must exist immediately after init");
    for page in 0..3 { assert_eq!(page_refcount(block + page * PAGE_SIZE), 1); }
    assert_eq!(free_pages(), before - 3);
    free_pages_contig(block, 3);
    assert_eq!(free_pages(), before);

    let mut live = std::collections::BTreeSet::new();
    for _ in 0..1792 {
        let pa = alloc_page().unwrap();
        assert!((0x100000..0x200000).contains(&pa) || (0x400000..0xA00000).contains(&pa));
        assert!(live.insert(pa), "duplicate live allocation");
    }
    assert!(live.iter().any(|&pa| pa >= 0x400000));
    for pa in live { free_page(pa); }
    assert_eq!(free_pages_node(0), 1792);

    // Build more than 512 non-coalescing order-0 free entries. Missing-target
    // and deep-target searches must not lose any of those list entries.
    let pages: Vec<_> = (0..1792).map(|_| alloc_page().unwrap()).collect();
    for &pa in pages.iter().step_by(2) { free_page(pa); }
    assert_eq!(free_pages_node(0), 896);
    {
        let _lock = PMM_LOCK.lock();
        assert!(!unsafe { buddy_remove(0, pages[1], 0) });
        assert_eq!(free_pages_node(0), 896);
        assert!(unsafe { buddy_remove(0, pages[0], 0) });
        unsafe { buddy_push(0, pages[0], 0) };
    }
    for &pa in pages.iter().skip(1).step_by(2) { free_page(pa); }
    assert_eq!(free_pages_node(0), 1792);

    // Exercise actual production alloc/free under concurrent host threads.
    let live = std::sync::Mutex::new(std::collections::BTreeSet::new());
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let live = &live;
            scope.spawn(move || {
                for _ in 0..512 {
                    let pa = alloc_page().unwrap();
                    assert!((0x100000..0x200000).contains(&pa) || (0x400000..0xA00000).contains(&pa));
                    assert!(live.lock().unwrap().insert(pa), "duplicate concurrent allocation");
                    std::thread::yield_now();
                    let mut tracked = live.lock().unwrap();
                    assert!(tracked.remove(&pa));
                    free_page(pa);
                }
            });
        }
    });
    assert!(live.lock().unwrap().is_empty());
    assert_eq!(free_pages_node(0), 1792);

    let mut reachable = 0usize;
    let mut seen = std::collections::BTreeSet::new();
    for order in 0..MAX_ORDER {
        let mut pi = BUDDY[0].free_lists[order].load(Ordering::Relaxed);
        while !pi.is_null() {
            let pa = unsafe { pi_to_pa(pi) };
            assert!(seen.insert(pa));
            assert_eq!(pa & (order_size(order) - 1), 0);
            reachable += 1usize << order;
            pi = unsafe { (*pi).free_next.load(Ordering::Relaxed) };
        }
    }
    assert_eq!(reachable, free_pages_node(0));
    unsafe { init_from_regions(&regions) };
    assert_eq!(free_pages_node(0), 1792, "repeat initialization must not duplicate memory");
    TEST_MEMORY_LEN.store(0, Ordering::Relaxed);
    TEST_MEMORY_BASE.store(0, Ordering::Relaxed);
}
