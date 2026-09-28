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
