//! Host tests of the real injection engine, not PMM/VMM/syscall integration.
#![allow(dead_code)]

#[path = "../../../src/fault_inject/mod.rs"]
mod fault_inject;

use fault_inject::FaultPoint;

#[test]
fn unarmed_point_never_fires() {
    let point = FaultPoint::new("host");
    for _ in 0..10 {
        assert!(!point.check());
    }
    assert_eq!(point.trigger_count(), 0);
}

#[test]
fn immediate_failure_is_one_shot() {
    let point = FaultPoint::new("host");
    point.arm(0);
    assert!(point.check());
    assert!(!point.check());
    assert_eq!(point.trigger_count(), 1);
}

#[test]
fn delayed_failure_respects_success_budget() {
    let point = FaultPoint::new("host");
    point.arm(3);
    for _ in 0..3 {
        assert!(!point.check());
    }
    assert!(point.check());
    assert!(!point.check());
    assert_eq!(point.trigger_count(), 1);
}

#[test]
fn explicit_disarm_prevents_failure() {
    let point = FaultPoint::new("host");
    point.arm(0);
    point.disarm();
    assert!(!point.check());
    assert_eq!(point.trigger_count(), 0);
}

#[test]
fn persistent_failure_continues_until_disarmed() {
    let point = FaultPoint::new("host");
    point.arm(1);
    assert!(!point.check_persistent());
    for _ in 0..4 {
        assert!(point.check_persistent());
    }
    point.disarm();
    assert!(!point.check_persistent());
    assert_eq!(point.trigger_count(), 4);
}

#[test]
fn rearming_and_resetting_counters_are_independent() {
    let point = FaultPoint::new("host");
    point.arm(0);
    assert!(point.check());
    point.arm(0);
    assert!(point.check());
    assert_eq!(point.trigger_count(), 2);
    point.reset_triggers();
    assert_eq!(point.trigger_count(), 0);
    assert!(!point.check());
}


/// Emit clearly scoped host-engine markers for the catalogued fault classes.
/// These markers are not a guest-kernel recovery claim: the PMM/VMM/syscall
/// injection callsites and post-recovery guest workloads are not integrated yet.
#[test]
fn catalogued_fault_points_report_host_recovery_boundary() {
    let scenarios = [
        ("pmm_alloc_oom_once", "pmm::alloc_frame"),
        ("pmm_contiguous_alloc_once", "pmm::alloc_frames_contiguous"),
        ("vmm_map_once", "vmm::map_page"),
        ("vmm_reserve_once", "vmm::reserve_region"),
        ("syscall_emfile_once", "syscall::fd_exhaustion"),
        ("syscall_eagain_once", "syscall::transient_eagain"),
    ];

    for (scenario, site) in scenarios {
        let point = FaultPoint::new(site);
        point.arm(0);
        assert!(point.check(), "{scenario} should fire once");
        println!("RUSTOS_FAULT marker=fired scenario={scenario} scope=host_engine");
        assert_eq!(point.trigger_count(), 1);

        assert!(!point.check(), "{scenario} must disarm after its one-shot fault");
        println!("RUSTOS_FAULT marker=recovered scenario={scenario} scope=host_engine");
        assert!(!point.check(), "{scenario} should remain healthy after recovery");
        println!("RUSTOS_FAULT marker=post_recovery_pass scenario={scenario} scope=host_engine");
    }
}
