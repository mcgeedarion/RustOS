//! Kernel test suites — compiled only when `feature = "kmtest"`.
//!
//! Each sub-module exposes a `pub fn register()` that calls
//! `kmtest::register!(name, fn)` for every test it owns.  The top-level
//! `init()` function here calls all of them in order; it is invoked once
//! from `kernel_main` before the scheduler starts.
//!
//! ## Adding a new suite
//! 1. Create `src/kmtest/<suite>.rs`.
//! 2. Add `pub mod <suite>;` below.
//! 3. Call `<suite>::register()` inside `init()`.

extern crate alloc;

use alloc::vec::Vec;
use spin::Mutex;

pub mod fs;
pub mod ipc;
pub mod mm;
pub mod panic_format_tests;
pub mod proc;
pub mod sync;

pub type KmTestResult = Result<(), &'static str>;

#[derive(Clone, Copy)]
pub struct KmTestEntry {
    pub name: &'static str,
    pub run: fn() -> KmTestResult,
}

static REGISTRY: Mutex<Vec<KmTestEntry>> = Mutex::new(Vec::new());

pub fn register_entry(name: &'static str, run: fn() -> KmTestResult) {
    let mut registry = REGISTRY.lock();
    if let Some(entry) = registry.iter_mut().find(|entry| entry.name == name) {
        entry.run = run;
        return;
    }
    registry.push(KmTestEntry { name, run });
}

pub fn reset_registry() {
    REGISTRY.lock().clear();
}

pub fn with_registry<R>(f: impl FnOnce(&[KmTestEntry]) -> R) -> R {
    let registry = REGISTRY.lock();
    f(&registry)
}

pub fn registry_snapshot() -> Vec<KmTestEntry> {
    REGISTRY.lock().clone()
}

macro_rules! register {
    ($name:expr, $run:path $(,)?) => {
        $crate::kmtest::register_entry($name, $run)
    };
}

pub(crate) use register;

/// Register all suites with the kmtest harness.
/// Called once from kernel_main when `feature = "kmtest"` is active.
pub fn init() {
    reset_registry();
    mm::register();
    proc::register();
    fs::register();
    sync::register();
    ipc::register();
    panic_format_tests::register();
}
