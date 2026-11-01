//! Production exit.rs and proc_table.rs, with mocked platform/PCB services.
//! No actual wait4 syscall, machine context switch, or SMP exit is tested.
extern crate alloc;
extern crate self as spin;
use std::sync::atomic::{AtomicUsize, Ordering};
pub struct Mutex<T>(std::sync::Mutex<T>);
impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self { Self(std::sync::Mutex::new(value)) }
    pub fn lock(&self) -> std::sync::MutexGuard<'_, T> { self.0.lock().unwrap() }
}
#[macro_export]
macro_rules! serial_println { ($($args:tt)*) => {{}}; }
pub mod allocator { pub struct InterruptGuard; impl InterruptGuard { pub fn new() -> Self { Self } } }
pub mod arch {
    pub mod api { pub trait Cpu { fn halt(); } }
    pub struct Arch;
    impl api::Cpu for Arch { fn halt() { panic!("unexpected idle loop") } }
}
pub mod sync { pub mod spinlock { pub use crate::Mutex as SpinLock; } }
static CURRENT: AtomicUsize = AtomicUsize::new(1);
static TASK_DROPS: AtomicUsize = AtomicUsize::new(0);
static WAKES: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static NOTICES: Mutex<Vec<(usize, usize, i32)>> = Mutex::new(Vec::new());
static STACKS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static TID_WAKES: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
#[derive(Debug)]
struct Scheduled;

#[path = "../../src/proc/proc_table.rs"]
pub mod real_table;
#[path = "../../src/proc/exit.rs"]
pub mod real_exit;
pub mod proc {
    pub use crate::real_table as proc_table;
    pub mod process {
        use std::sync::Arc;
        use crate::Mutex;
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub enum State { Ready, Running, Blocked, Zombie }
        #[derive(Clone, Default)]
        pub struct Sched;
        #[derive(Clone, Default)]
        pub struct Ns { pub net: u64, pub mnt: u64, pub uts: u64 }
        #[derive(Clone)]
        pub struct Pcb {
            pub pid: usize, pub ppid: usize, pub tgid: usize, pub state: State,
            pub exit_code: i32, pub clear_child_tid_va: usize, pub kstack_top: usize,
            pub vfork_parent: usize, pub user_pagetable: usize, pub vmas: Vec<usize>,
            pub ns: Ns, pub task: *mut super::task_types::Task, pub sched: Sched,
        }
        unsafe impl Send for Pcb {}
        pub type Process = Pcb;
        pub struct ProcLock {
            pub pid: u32, pub tgid: u32, pub state_atom: std::sync::atomic::AtomicU8,
            pub inner: Mutex<Pcb>,
        }
        impl ProcLock {
            pub fn new(pcb: Pcb) -> Arc<Self> {
                let state = pcb.state as u8;
                Arc::new(Self { pid: pcb.pid as u32, tgid: pcb.tgid as u32,
                    state_atom: std::sync::atomic::AtomicU8::new(state), inner: Mutex::new(pcb) })
            }
            pub fn load_state(&self) -> State {
                match self.state_atom.load(std::sync::atomic::Ordering::Acquire) {
                    0 => State::Ready, 1 => State::Running, 2 => State::Blocked, _ => State::Zombie,
                }
            }
            pub fn set_state(&self, pcb: &mut Pcb, state: State) {
                pcb.state = state;
                self.state_atom.store(state as u8, std::sync::atomic::Ordering::Release);
            }
        }
    }
    pub mod task_types {
        pub struct Task { pub pid: u32, pub sched: super::process::Sched, pub pcb: *mut super::process::Pcb }
        impl Task { pub fn new(pcb: *mut super::process::Pcb) -> Self {
            assert!(!pcb.is_null()); Self { pid: 0, sched: Default::default(), pcb }
        } }
        impl Drop for Task { fn drop(&mut self) { crate::TASK_DROPS.fetch_add(1, crate::Ordering::SeqCst); } }
    }
    pub mod scheduler {
        pub use crate::real_table::{with_proc, with_proc_mut, with_procs_ro};
        pub fn current_pid() -> usize { crate::CURRENT.load(crate::Ordering::SeqCst) }
        pub fn wake_pid(pid: usize) { crate::WAKES.lock().push(pid); }
        pub fn schedule() { std::panic::panic_any(crate::Scheduled); }
    }
    pub mod thread {
        pub fn tgid_of(pid: usize) -> usize { super::scheduler::with_proc(pid, |p| p.tgid).unwrap_or(pid) }
        pub fn unregister_thread(_: usize) {}
    }
    pub mod futex {
        pub fn robust_list_on_exit(_: usize) {}
        pub fn futex_clear_pid(_: usize) {}
        pub fn futex_wake_addr(va: usize, count: usize) { crate::TID_WAKES.lock().push((va, count)); }
    }
    pub mod signal {
        pub fn altstack_clear_pid(_: usize) {}
        pub fn group_pending_clear(_: usize) {}
        pub fn send_sigchld(parent: usize, child: usize, status: i32) {
            assert!(super::scheduler::with_proc(child, |p| p.state == super::process::State::Zombie).unwrap());
            crate::NOTICES.lock().push((parent, child, status));
        }
    }
    pub mod namespace {
        pub type NsId = u64;
        pub const INIT_NS: NsId = 0;
        pub fn drop_mount_ns(_: NsId) {}
        pub fn drop_uts_ns(_: NsId) {}
    }
    pub mod net_ns { pub fn destroy_net_ns(_: u64) {} }
    pub mod cgroup { pub fn cgroup_exit(_: usize) {} }
    pub mod wait {
        use super::{scheduler, signal};
        include!(env!("RUSTOS_WAIT_HELPERS"));
    }
}
pub mod mm { pub mod kstack { pub fn free_kstack(top: usize) { crate::STACKS.lock().push(top); } } }
pub mod init { pub mod service_manager { pub fn on_process_exit(_: usize, _: i32) {} } }
pub mod fs { pub mod process_fd { pub fn proc_fd_free(_: usize) {} } }
pub mod ipc { pub fn endpoint_cleanup_pid(_: usize) {} }
pub mod syscall {
    pub fn proc_name_clear(_: usize) { panic!("obsolete recursive cleanup hook called") }
    pub mod scheme { pub fn cleanup_pid(_: usize) {} }
    pub mod driver { pub fn cleanup_pid(_: usize) {} }
}
pub mod uaccess {
    pub fn copy_to_user(dst: usize, bytes: &[u8]) -> Result<(), ()> {
        if dst < 4096 { return Err(()); }
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), dst as *mut u8, bytes.len()) };
        Ok(())
    }
}
fn pcb(pid: usize, parent: usize) -> proc::process::Pcb {
    proc::process::Pcb { pid, ppid: parent, tgid: pid, state: proc::process::State::Running,
        exit_code: 0, clear_child_tid_va: 0, kstack_top: 0x100000 + pid * 0x10000,
        vfork_parent: 0, user_pagetable: pid * 4096, vmas: vec![pid],
        ns: Default::default(), task: core::ptr::null_mut(), sched: Default::default() }
}
fn reaches_scheduler(f: impl FnOnce()) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    assert!(result.unwrap_err().is::<Scheduled>(), "exit failed before scheduler handoff");
}
#[test]
fn production_exit_publishes_status_retains_task_and_allows_logical_unlink() {
    use proc::process::State;
    real_table::enqueue(pcb(1, 0));
    let mut child = pcb(2, 1);
    let mut tid = 2u32;
    let tid_va = &mut tid as *mut u32 as usize;
    child.clear_child_tid_va = tid_va;
    real_table::enqueue(child);
    CURRENT.store(2, Ordering::SeqCst);
    reaches_scheduler(|| { real_exit::sys_exit(42); });
    assert_eq!(tid, 0);
    assert!(TID_WAKES.lock().contains(&(tid_va, 1)));
    assert!(real_table::with_proc(2, |p| p.state == State::Zombie && p.exit_code == 42 << 8).unwrap());
    assert!(real_table::with_proc(2, |p| p.task.is_null() && p.kstack_top == 0).unwrap());
    assert_eq!(real_table::with_proc(2, |p| (p.user_pagetable, p.vmas.len())), Some((8192, 1)));
    assert!(NOTICES.lock().contains(&(1, 2, 42 << 8)));
    assert!(WAKES.lock().contains(&1));
    assert_eq!(real_exit::retired_task_count(), 1);
    real_table::remove(2);
    real_table::remove(2);
    assert!(real_table::find_proc_lock(2).is_none());
    assert_eq!(TASK_DROPS.load(Ordering::SeqCst), 0);
    assert_eq!(real_exit::retired_task_count(), 1);

    real_table::enqueue(pcb(3, 1));
    CURRENT.store(3, Ordering::SeqCst);
    reaches_scheduler(|| { real_exit::sys_exit_group(7); });
    assert_eq!(real_table::with_proc(3, |p| p.exit_code), Some(7 << 8));
    assert_eq!(real_exit::retired_task_count(), 2);
    real_table::remove(3);
    assert_eq!(TASK_DROPS.load(Ordering::SeqCst), 0);
    assert_eq!(real_table::proc_count(), 1);
}
