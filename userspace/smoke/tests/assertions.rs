//! Host-only regression tests for the actual smoke assertion control flow.
//!
//! Run from the repository root with the pinned Rust toolchain:
//! rustc --edition=2021 --test userspace/smoke/tests/assertions.rs -o /tmp/smoke-assertions
//! /tmp/smoke-assertions
//!
//! This deliberately bypasses the freestanding binary and kernel build.
//! No real syscalls or child processes are used; this does not validate RustOS
//! syscall implementations, architecture assembly, or the kernel boot path.

#[cfg(not(target_arch = "x86_64"))]
compile_error!("smoke assertion regressions require an x86_64 host test target");

use core::ffi::c_int;
use std::cell::RefCell;

#[path = "../src/checks.rs"]
mod checks;

const O_RDONLY: usize = 0;

struct Mock {
    forks: [isize; 2],
    opens: [isize; 2],
    closes: [isize; 2],
    read: isize,
    write: isize,
    wait_error: Option<isize>,
    wait_status: c_int,
    errno_pass: bool,
    fork_calls: usize,
    open_calls: usize,
    close_calls: usize,
    read_calls: usize,
    waited_pids: Vec<isize>,
    failures: Vec<String>,
}

impl Default for Mock {
    fn default() -> Self {
        Self {
            forks: [100, 101],
            opens: [3, 4],
            closes: [0, 0],
            read: 0,
            write: 13,
            wait_error: None,
            wait_status: 42 << 8,
            errno_pass: true,
            fork_calls: 0,
            open_calls: 0,
            close_calls: 0,
            read_calls: 0,
            waited_pids: Vec::new(),
            failures: Vec::new(),
        }
    }
}

thread_local! {
    static MOCK: RefCell<Mock> = RefCell::new(Mock::default());
}

fn run_with(mock: Mock) -> (bool, Mock) {
    MOCK.with(|state| *state.borrow_mut() = mock);
    let passed = checks::run_tests();
    let state = MOCK.with(|state| state.replace(Mock::default()));
    (passed, state)
}

fn check(name: &str, ok: bool) -> bool {
    if !ok {
        MOCK.with(|state| state.borrow_mut().failures.push(name.into()));
    }
    ok
}

fn test_errno_checks() -> bool {
    MOCK.with(|state| state.borrow().errno_pass)
}

fn write_str(_fd: usize, _message: &str) {}

unsafe fn sys_write(_fd: usize, _buf: *const u8, _count: usize) -> isize {
    MOCK.with(|state| state.borrow().write)
}

unsafe fn sys_open(_path: *const u8, _flags: usize, _mode: usize) -> isize {
    MOCK.with(|state| {
        let mut state = state.borrow_mut();
        let result = state.opens[state.open_calls];
        state.open_calls += 1;
        result
    })
}

unsafe fn sys_close(_fd: usize) -> isize {
    MOCK.with(|state| {
        let mut state = state.borrow_mut();
        let result = state.closes[state.close_calls];
        state.close_calls += 1;
        result
    })
}

unsafe fn sys_read(_fd: usize, _buf: *mut u8, _count: usize) -> isize {
    MOCK.with(|state| {
        let mut state = state.borrow_mut();
        state.read_calls += 1;
        state.read
    })
}

unsafe fn sys_fork() -> isize {
    MOCK.with(|state| {
        let mut state = state.borrow_mut();
        let result = state.forks[state.fork_calls];
        state.fork_calls += 1;
        result
    })
}

unsafe fn sys_wait4(pid: isize, status: *mut c_int, _options: usize) -> isize {
    MOCK.with(|state| {
        let mut state = state.borrow_mut();
        state.waited_pids.push(pid);
        // SAFETY: checks::run_tests passes a pointer to its live local status.
        unsafe { *status = state.wait_status };
        state.wait_error.unwrap_or(pid)
    })
}

unsafe fn sys_exit_group(status: usize) -> ! {
    panic!("unexpected child/exit path in parent-only mock: {status}");
}

#[test]
fn successful_run_checks_both_children_and_eof() {
    let (passed, state) = run_with(Mock::default());
    assert!(passed);
    assert!(state.failures.is_empty());
    assert_eq!(state.fork_calls, 2);
    assert_eq!(state.waited_pids, [100, 101]);
    assert_eq!(state.open_calls, 2);
    assert_eq!(state.close_calls, 2);
    assert_eq!(state.read_calls, 1);
}

#[test]
fn either_fork_failure_fails_the_run_and_is_not_waited_on() {
    for index in 0..2 {
        for errno in [-38, -11] {
            let mut mock = Mock::default();
            mock.forks[index] = errno; // ENOSYS and EAGAIN.
            let expected_pid = mock.forks[1 - index];
            let (passed, state) = run_with(mock);
            assert!(!passed, "fork {index} returning {errno} passed");
            assert_eq!(
                state.failures,
                [if index == 0 { "fork" } else { "second fork" }]
            );
            assert_eq!(state.fork_calls, 2);
            assert_eq!(state.waited_pids, [expected_pid]);
        }
    }
}

#[test]
fn both_fork_failures_cannot_produce_success() {
    let (passed, state) = run_with(Mock {
        forks: [-38, -11],
        ..Mock::default()
    });
    assert!(!passed);
    assert_eq!(state.failures, ["fork", "second fork"]);
    assert!(state.waited_pids.is_empty());
}

#[test]
fn either_open_failure_fails_the_run() {
    for index in 0..2 {
        let mut mock = Mock::default();
        mock.opens[index] = -24;
        let (passed, state) = run_with(mock);
        assert!(!passed, "open {index} failure passed");
        assert_eq!(state.open_calls, 2);
        assert_eq!(state.close_calls, 1);
        assert_eq!(state.read_calls, usize::from(index == 0));
    }
}

#[test]
fn dev_null_requires_exact_eof() {
    for result in [1, -9] {
        let (passed, state) = run_with(Mock {
            read: result,
            ..Mock::default()
        });
        assert!(!passed, "/dev/null read returning {result} passed");
        assert_eq!(state.failures, ["read /dev/null returns EOF"]);
        assert_eq!(state.close_calls, 2);
    }
}

#[test]
fn either_close_failure_fails_the_run() {
    for index in 0..2 {
        let mut mock = Mock::default();
        mock.closes[index] = -9;
        let (passed, state) = run_with(mock);
        assert!(!passed, "close {index} failure passed");
        assert_eq!(state.close_calls, 2);
    }
}

#[test]
fn wait_errors_and_wrong_exit_status_fail() {
    for mock in [
        Mock {
            wait_error: Some(-10),
            ..Mock::default()
        },
        Mock {
            wait_status: 7 << 8,
            ..Mock::default()
        },
    ] {
        let (passed, state) = run_with(mock);
        assert!(!passed);
        assert_eq!(state.waited_pids, [100, 101]);
    }
}

#[test]
fn existing_write_and_errno_failures_are_preserved() {
    for mock in [
        Mock {
            write: -9,
            ..Mock::default()
        },
        Mock {
            errno_pass: false,
            ..Mock::default()
        },
    ] {
        assert!(!run_with(mock).0);
    }
}
