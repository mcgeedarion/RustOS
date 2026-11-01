//! Production PMM tests with host locks, fake RAM, and no privileged operations.
extern crate alloc;
extern crate self as spin;
extern crate self as log;
#[macro_export]
macro_rules! info { ($($args:tt)*) => {{}}; }
pub struct Mutex<T>(std::sync::Mutex<T>);
impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self { Self(std::sync::Mutex::new(value)) }
    pub fn lock(&self) -> std::sync::MutexGuard<'_, T> { self.0.lock().unwrap() }
}
pub struct Once<T>(std::sync::OnceLock<T>);
impl<T> Once<T> {
    pub const fn new() -> Self { Self(std::sync::OnceLock::new()) }
    pub fn call_once(&self, f: impl FnOnce() -> T) -> &T { self.0.get_or_init(f) }
}
pub mod allocator {
    pub struct InterruptGuard;
    impl InterruptGuard { pub fn new() -> Self { Self } }
}
#[path = "../../src/mm/boot_memory.rs"]
pub mod boot_memory;
pub mod mm { pub use crate::boot_memory; }
#[path = "../../src/mm/pmm.rs"]
mod pmm;
