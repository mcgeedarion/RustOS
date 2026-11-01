#!/usr/bin/env python3
'''Apply Task 3 from a pinned baseline; refuse drift; dry-run by default.'''
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

BASE = "b6bd4d6013175b578a09a54402448eee8ee44340"
CHECKS = "userspace/smoke/src/checks.rs"
MAIN = "userspace/smoke/src/main.rs"
ASSERTIONS = "userspace/smoke/tests/assertions.rs"
XTASK = "xtask/src/main.rs"
EXPECTED = {
    CHECKS: "89b35ce57e0b4edeb3d4fa474cf13c276a7a553b",
    MAIN: "63ae69097cf7a15c148ef30280a0ccbad07a3ae5",
    ASSERTIONS: "56049d13e4b4d7ddb2e52d1c61ebff53e35c9ca2",
    XTASK: "8beb977c7c13cb03822b6bdd338a1c51743cdaad",
}


def blob_sha(data):
    return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()


def build(originals):
    output = {name: data.decode("utf-8") for name, data in originals.items()}

    def replace(name, old, new, count=1):
        if output[name].count(old) != count:
            raise ValueError(f"{name}: anchor count differs from reviewed baseline")
        output[name] = output[name].replace(old, new)

    def domain(start, end, flag):
        text = output[CHECKS]
        if text.count(start) != 1 or text.count(end) != 1:
            raise ValueError("ambiguous domain boundaries")
        left, right = text.index(start), text.index(end)
        if left >= right or "pass &=" not in text[left:right]:
            raise ValueError("invalid domain range")
        block = text[left:right].replace("pass &=", flag + " &=")
        output[CHECKS] = (text[:left] + f"    let mut {flag} = true;\n" + block
                          + f"    pass &= {flag};\n\n" + text[right:])

    replace(CHECKS, "pub(super) fn run_tests() -> bool {\n    let mut pass = true;", r'''#[derive(Debug, Clone, Copy)]
pub(super) struct SmokeResults {
    pub(super) pass: bool,
    pub(super) fs_pass: bool,
    pub(super) ipc_pass: bool,
}

impl SmokeResults {
    /// Shared by the guest entry point and host marker-gating regressions.
    pub(super) fn emit_pass_markers(&self, mut emit: impl FnMut(&str)) {
        if !self.pass {
            return;
        }
        if self.fs_pass {
            emit("RUSTOS_SMOKE marker=fs_roundtrip_pass\n");
        }
        if self.ipc_pass {
            emit("RUSTOS_SMOKE marker=ipc_roundtrip_pass\n");
        }
    }
}

// Keep the boolean API used by existing host assertion tests.
#[allow(dead_code)]
pub(super) fn run_tests() -> bool {
    run_tests_with_domains().pass
}

pub(super) fn run_tests_with_domains() -> SmokeResults {
    let mut pass = true;''')
    domain("    // Exercise create/write/seek/read/rename/unlink on a regular file.",
           "    // Exercise kernel-backed pipe IPC with a bounded in-pipe payload.", "fs_pass")
    domain("    // Exercise kernel-backed pipe IPC with a bounded in-pipe payload.",
           "    pass &= test_errno_checks();", "ipc_pass")
    replace(CHECKS, "    pass &= test_errno_checks();\n    pass\n}",
            "    pass &= test_errno_checks();\n    SmokeResults { pass, fs_pass, ipc_pass }\n}")
    replace(MAIN, "    let pass = checks::run_tests();\n\n    if pass {",
            "    let results = checks::run_tests_with_domains();\n\n    if results.pass {\n        results.emit_pass_markers(|line| write_str(1, line));")
    replace(XTASK, "|RUSTOS_EXEC marker=smoke_tests_passed|",
            "|RUSTOS_EXEC marker=smoke_tests_passed|RUSTOS_SMOKE marker=fs_roundtrip_pass|RUSTOS_SMOKE marker=ipc_roundtrip_pass|")
    replace(XTASK, '            && has_exact_line("RUSTOS_EXEC marker=smoke_tests_passed")\n',
            '            && has_exact_line("RUSTOS_EXEC marker=smoke_tests_passed")\n'
            '            && has_exact_line("RUSTOS_SMOKE marker=fs_roundtrip_pass")\n'
            '            && has_exact_line("RUSTOS_SMOKE marker=ipc_roundtrip_pass")\n')
    output[XTASK] += "\n#[cfg(test)]\nmod smoke_marker_tests;\n"
    replace(ASSERTIONS, "    opens: [isize; 4],", "    opens: [isize; 6],")
    replace(ASSERTIONS, "            opens: [3, 4, 5, 6],", "            opens: [3, 4, 5, -2, 6, -2],")
    replace(ASSERTIONS, "    pipe_result: isize,",
            "    pipe_result: isize,\n    file_corrupt: bool,\n    pipe_corrupt: bool,\n    pipe_write_error: Option<isize>,")
    replace(ASSERTIONS, "            pipe_result: 0,",
            "            pipe_result: 0,\n            file_corrupt: false,\n            pipe_corrupt: false,\n            pipe_write_error: None,")
    replace(ASSERTIONS, '''unsafe fn sys_write(_fd: usize, _buf: *const u8, count: usize) -> isize {
    MOCK.with(|state| {
        let result = state.borrow().write;
        if result < 0 { result } else { count as isize }
    })
}''', '''unsafe fn sys_write(fd: usize, _buf: *const u8, count: usize) -> isize {
    MOCK.with(|state| {
        let state = state.borrow();
        let result = if fd == 9 {
            state.pipe_write_error.unwrap_or(state.write)
        } else {
            state.write
        };
        if result < 0 { result } else { count as isize }
    })
}''')
    replace(ASSERTIONS, '''            6 => {
                let n = FILE_DATA.len().min(count);
                unsafe { core::ptr::copy_nonoverlapping(FILE_DATA.as_ptr(), buf, n) };
                n as isize
            }''', '''            5 | 6 => {
                let n = FILE_DATA.len().min(count);
                unsafe { core::ptr::copy_nonoverlapping(FILE_DATA.as_ptr(), buf, n) };
                if state.file_corrupt && n != 0 {
                    unsafe { *buf ^= 0xff };
                }
                n as isize
            }''')
    replace(ASSERTIONS, '''                unsafe { core::ptr::copy_nonoverlapping(PIPE_DATA.as_ptr(), buf, n) };
                n as isize''', '''                unsafe { core::ptr::copy_nonoverlapping(PIPE_DATA.as_ptr(), buf, n) };
                if state.pipe_corrupt && n != 0 {
                    unsafe { *buf ^= 0xff };
                }
                n as isize''')
    replace(ASSERTIONS, "assert_eq!(state.open_calls, 4);", "assert_eq!(state.open_calls, 6);", 2)
    replace(ASSERTIONS, "assert_eq!(state.read_calls, 3);", "assert_eq!(state.read_calls, 4);")
    replace(ASSERTIONS, "assert_eq!(state.read_calls, if index == 0 { 3 } else { 2 });",
            "assert_eq!(state.read_calls, if index == 0 { 4 } else { 3 });")
    output[ASSERTIONS] += HOST_TESTS
    output["xtask/src/smoke_marker_tests.rs"] = XTASK_TESTS
    return {name: text.encode("utf-8") for name, text in output.items()}


def preflight(root, originals, desired):
    changes = {}
    for name, data in desired.items():
        path = root / name
        if Path(name).is_absolute() or ".." in Path(name).parts or path.is_symlink():
            raise ValueError(f"{name}: unsafe path")
        if not path.resolve().is_relative_to(root):
            raise ValueError(f"{name}: target outside repository")
        old = path.read_bytes() if path.exists() else None
        if old != originals.get(name) and old != data:
            raise ValueError(f"{name}: source drift; refusing to overwrite")
        if old != data:
            changes[name] = (old, data)
    return changes


def write_changes(root, changes):
    written = []
    try:
        for name, (old, data) in changes.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            mode = path.stat().st_mode & 0o777 if path.exists() else 0o644
            with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
                temporary = Path(stream.name)
                stream.write(data)
            try:
                temporary.chmod(mode)
                os.replace(temporary, path)
            finally:
                temporary.unlink(missing_ok=True)
            written.append(name)
    except BaseException:
        for name in reversed(written):
            path = root / name
            old = changes[name][0]
            if old is None:
                path.unlink(missing_ok=True)
            else:
                path.write_bytes(old)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    root = Path.cwd().resolve()
    originals = {}
    for name, expected in EXPECTED.items():
        data = subprocess.check_output(["git", "show", f"{BASE}:{name}"])
        if blob_sha(data) != expected:
            raise ValueError(f"{name}: baseline blob hash mismatch")
        originals[name] = data
    desired = build(originals)
    changes = preflight(root, originals, desired)
    if args.apply:
        write_changes(root, changes)
    print(json.dumps({"applied": args.apply, "changed": sorted(changes),
                      "desired_blob_shas": {name: blob_sha(data) for name, data in desired.items()}}, indent=2))


HOST_TESTS = r'''
fn run_with_domains(mock: Mock) -> (checks::SmokeResults, Vec<String>, Mock) {
    MOCK.with(|state| *state.borrow_mut() = mock);
    let results = checks::run_tests_with_domains();
    let mut markers = Vec::new();
    results.emit_pass_markers(|line| markers.push(line.to_owned()));
    let state = MOCK.with(|state| state.replace(Mock::default()));
    (results, markers, state)
}

#[test]
fn successful_domains_emit_exact_standalone_markers_in_order() {
    let (result, markers, _) = run_with_domains(Mock::default());
    assert!(result.pass && result.fs_pass && result.ipc_pass);
    assert_eq!(markers, [
        "RUSTOS_SMOKE marker=fs_roundtrip_pass\n",
        "RUSTOS_SMOKE marker=ipc_roundtrip_pass\n",
    ]);
}

#[test]
fn filesystem_failures_are_domain_local_and_suppress_all_markers() {
    let mut create = Mock::default();
    create.opens[2] = -24;
    let mut close_original = Mock::default();
    close_original.closes[2] = -9;
    let mut close_renamed = Mock::default();
    close_renamed.closes[3] = -9;
    for mock in [
        create, close_original, close_renamed,
        Mock { rename_result: -2, ..Mock::default() },
        Mock { unlink_result: -2, ..Mock::default() },
        Mock { file_corrupt: true, ..Mock::default() },
    ] {
        let (result, markers, state) = run_with_domains(mock);
        assert!(!result.pass && !result.fs_pass && result.ipc_pass);
        assert!(markers.is_empty());
        assert_eq!(state.pipe_calls, 1, "IPC must still run after an FS failure");
    }
}

#[test]
fn ipc_failures_are_domain_local_and_suppress_all_markers() {
    let mut close_read = Mock::default();
    close_read.closes[4] = -9;
    let mut close_write = Mock::default();
    close_write.closes[5] = -9;
    for mock in [
        close_read, close_write,
        Mock { pipe_result: -24, ..Mock::default() },
        Mock { pipe_corrupt: true, ..Mock::default() },
        Mock { pipe_write_error: Some(-9), ..Mock::default() },
    ] {
        let (result, markers, _) = run_with_domains(mock);
        assert!(!result.pass && result.fs_pass && !result.ipc_pass);
        assert!(markers.is_empty());
    }
}

#[test]
fn unrelated_failures_suppress_domain_markers_even_when_both_domains_pass() {
    let mut close_null = Mock::default();
    close_null.closes[0] = -9;
    for mock in [
        close_null,
        Mock { forks: [-38, 101], ..Mock::default() },
        Mock { errno_pass: false, ..Mock::default() },
        Mock { read: 1, ..Mock::default() },
    ] {
        let (result, markers, _) = run_with_domains(mock);
        assert!(!result.pass && result.fs_pass && result.ipc_pass);
        assert!(markers.is_empty());
    }
}

#[test]
fn both_domain_failures_are_retained() {
    let (result, markers, _) = run_with_domains(Mock {
        file_corrupt: true,
        pipe_corrupt: true,
        ..Mock::default()
    });
    assert!(!result.pass && !result.fs_pass && !result.ipc_pass);
    assert!(markers.is_empty());
}

#[test]
fn marker_emitter_checks_each_domain_flag_and_overall_pass() {
    for pass in [false, true] {
        for fs_pass in [false, true] {
            for ipc_pass in [false, true] {
                let result = checks::SmokeResults { pass, fs_pass, ipc_pass };
                let mut markers = Vec::new();
                result.emit_pass_markers(|line| markers.push(line.to_owned()));
                let mut expected = Vec::new();
                if pass && fs_pass {
                    expected.push("RUSTOS_SMOKE marker=fs_roundtrip_pass\n".to_owned());
                }
                if pass && ipc_pass {
                    expected.push("RUSTOS_SMOKE marker=ipc_roundtrip_pass\n".to_owned());
                }
                assert_eq!(markers, expected);
            }
        }
    }
}
'''

XTASK_TESTS = r'''//! Host-only contract tests; not QEMU or guest execution evidence.
use super::{serial_has_smoke_marker, smoke_marker_regex};
use std::{fs, path::PathBuf, sync::atomic::{AtomicU64, Ordering}};

const FS: &str = "RUSTOS_SMOKE marker=fs_roundtrip_pass";
const IPC: &str = "RUSTOS_SMOKE marker=ipc_roundtrip_pass";
const OLD_CONTRACT: &str = concat!(
    "BOOT_INIT_EXEC\n",
    "USERSPACE_SMOKE_START\n",
    "RUSTOS_EXEC marker=smoke_started\n",
    "USERSPACE_SMOKE_PASS\n",
    "RUSTOS_EXEC marker=smoke_tests_passed\n",
    "RUSTOS_EXEC marker=smoke_child_reaped pid=3 exit=0\n",
    "RUSTOS_BENCH marker=complete\n",
    "RUSTOS_EXEC marker=bench_child_reaped pid=4 exit=0\n",
    "RUSTOS_EXEC marker=init_complete exit=0\n",
    "FULL_OS_USERSPACE_OK\n",
);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct SerialLog(PathBuf);
impl SerialLog {
    fn new(text: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "rustos-task3-{}-{}.log", std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::write(&path, text).expect("write serial fixture");
        Self(path)
    }
    fn passes(&self) -> bool {
        serial_has_smoke_marker(&self.0, true).expect("validate fixture")
    }
}
impl Drop for SerialLog {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn full_contract_requires_both_domain_records() {
    assert!(!SerialLog::new(OLD_CONTRACT).passes());
    assert!(!SerialLog::new(&format!("{OLD_CONTRACT}{FS}\n")).passes());
    assert!(!SerialLog::new(&format!("{OLD_CONTRACT}{IPC}\n")).passes());
    assert!(SerialLog::new(&format!("{OLD_CONTRACT}{FS}\n{IPC}\n")).passes());
}

#[test]
fn domain_records_must_be_exact_standalone_lines() {
    for (missing, present) in [(FS, IPC), (IPC, FS)] {
        for malformed in [
            format!("prefix {missing}"), format!("\"{missing}\""),
            format!(" {missing}"), format!("{missing} trailing"),
            format!("{missing}_extra"),
        ] {
            assert!(!SerialLog::new(&format!("{OLD_CONTRACT}{present}\n{malformed}\n")).passes());
        }
    }
}

#[test]
fn full_contract_accepts_crlf_domain_records() {
    let log = format!("{OLD_CONTRACT}{FS}\n{IPC}\n").replace('\n', "\r\n");
    assert!(SerialLog::new(&log).passes());
}

#[test]
fn advertised_full_contract_lists_both_markers_but_minimal_does_not() {
    for marker in [FS, IPC] {
        assert!(smoke_marker_regex(true).split('|').any(|item| item == marker));
        assert!(!smoke_marker_regex(false).contains(marker));
    }
}

#[test]
fn domain_records_alone_cannot_satisfy_the_full_contract() {
    assert!(!SerialLog::new(&format!("{FS}\n{IPC}\n")).passes());
}
'''

if __name__ == "__main__":
    main()
