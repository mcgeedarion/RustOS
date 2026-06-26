//! UTS namespace  —  hostname and NIS domainname isolation.
//!
//! ## Linux syscall semantics modelled
//!
//!   clone(CLONE_NEWUTS) / unshare(CLONE_NEWUTS)
//!   sethostname(2) / gethostname(2)
//!
//! A `UtsNamespace` stores the five `utsname` fields that vary per-namespace.
//! The initial namespace (`INIT_UTS_NS`) is initialised at boot from
//! compile-time constants and the detected CPU architecture string.

use alloc::string::String;
use alloc::sync::Arc;
use crate::sync::spinlock::SpinLock;

#[derive(Clone, Debug)]
pub struct UtsNamespace {
    pub sysname:    String,    // always "RustOS"
    pub nodename:   String,    // hostname
    pub release:    String,    // kernel release string
    pub version:    String,    // build timestamp / extra info
    pub machine:    String,    // "x86_64" | "aarch64"
    pub domainname: String,    // NIS domainname
}

impl UtsNamespace {
    pub fn new_init() -> Self {
        UtsNamespace {
            sysname:    String::from("RustOS"),
            nodename:   String::from("rustos"),
            release:    String::from(env!("CARGO_PKG_VERSION")),
            version:    String::from("#1 SMP"),
            machine:    String::from({
                #[cfg(target_arch = "x86_64")]  { "x86_64" }
                #[cfg(target_arch = "aarch64")] { "aarch64" }
                #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))] { "unknown" }
            }),
            domainname: String::from("(none)"),
        }
    }
}

pub type UtsNsRef = Arc<SpinLock<UtsNamespace>>;

pub fn new_init_ns() -> UtsNsRef {
    Arc::new(SpinLock::new(UtsNamespace::new_init()))
}

pub fn clone_ns(src: &UtsNsRef) -> UtsNsRef {
    Arc::new(SpinLock::new(src.lock().clone()))
}
