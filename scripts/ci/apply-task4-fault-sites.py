#!/usr/bin/env python3
'''Apply Task 4 in guarded phases from a pinned immutable baseline.'''
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

BASE = "76fc820e432d0a06d556ce034c46cc4097e69386"
MM = "src/mm/mod.rs"
SYSCALL = "src/syscall/mod.rs"
KM = "src/kmtest/mod.rs"
KM_TESTS = "src/kmtest/fault_inject_tests.rs"
FAULT = "src/fault_inject/mod.rs"
PMM = "src/mm/pmm.rs"
MAP = "src/mm/mmap/mapping.rs"
PIPE = "src/fs/pipe.rs"
IPC = "src/ipc/mod.rs"
DOC = "docs/fault-recovery.md"
MARKERS = "src/fault_inject/markers.rs"
EXPECTED = {
    MM: "d5d39887c430737cc698bb792b862bb649f60a86",
    SYSCALL: "fe4eb89106840f85e194bd87b9e9394ffa3ba26f",
    KM: "ef3aa3a6e9cc746d869ee8b34cb21770b9c0fd60",
    KM_TESTS: "1cdc9c286a13e09e7a0cfed6ab978cc70c237af8",
    FAULT: "3baddb74df5333103c89bf4608cfe8a5203e4333",
    PMM: "f2cdf4748e4cf84c6f030df11ddbe347a71089a0",
    MAP: "d7e599197ef4de39f87bd0782b2e5957c93dbddd",
    PIPE: "9cc222a633410982f69bbe1a29aaf4f08d4b518c",
    IPC: "4deb69aeed9b0d154a030c3c55238db1966bf629",
    DOC: "25022f34f1ace1f92c05b4f6fda9fc51468d2535",
}


def blob_sha(data):
    return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()


def replace(output, name, old, new, count=1):
    if output[name].count(old) != count:
        raise ValueError(f"{name}: reviewed anchor count mismatch")
    output[name] = output[name].replace(old, new)


def build(originals):
    b = {name: data.decode("utf-8") for name, data in originals.items()}
    replace(b, MM, "pub mod pmm;\n", '''pub mod pmm;
#[cfg(feature = "fault-inject")]
pub mod pmm_fault;
#[cfg(feature = "fault-inject")]
pub mod vmm_fault;
''')
    replace(b, SYSCALL, "pub mod errno;\n", '''pub mod errno;
#[cfg(feature = "fault-inject")]
pub mod fault;
''')
    replace(b, KM, "pub mod fs;\n", '''#[cfg(feature = "fault-inject")]
mod fault_inject_tests;
pub mod fs;
''')
    replace(b, KM, "pub fn register_entry(name:", '''pub fn reset_registry() {
    REGISTRY.lock().clear();
}

pub fn register_entry(name:''')
    replace(b, KM, "    oom_recovery::register();\n", '''    oom_recovery::register();
    #[cfg(feature = "fault-inject")]
    fault_inject_tests::register();
''')
    replace(b, KM_TESTS, "pub fn register_tests(runner: &mut crate::kmtest::TestRunner) {", "pub fn register() {")
    replace(b, KM_TESTS, "runner.register(", "crate::kmtest::register_entry(", 7)
    replace(b, KM_TESTS, "    FAULT_VMM_MAP, FAULT_VMM_REGION,", "    FAULT_VMM_COW, FAULT_VMM_MAP, FAULT_VMM_REGION,")
    replace(b, KM_TESTS, "        &FAULT_VMM_MAP,\n", "        &FAULT_VMM_COW,\n        &FAULT_VMM_MAP,\n")
    replace(b, KM_TESTS, "pub fn register() {\n", '''pub fn register() {
    crate::kmtest::register_entry("fault_inject::vmm_cow_hook", test_vmm_cow_hook);
''')
    b[KM_TESTS] += '''
// Hook-only check; not guest recovery or a post-recovery workload.
pub fn test_vmm_cow_hook() -> Result<(), &'static str> {
    reset_all();
    FAULT_VMM_COW.arm(0);
    if !crate::mm::vmm_fault::check_cow_fault() {
        return Err("CoW hook did not consume the armed point");
    }
    if crate::mm::vmm_fault::check_cow_fault() || FAULT_VMM_COW.trigger_count() != 1 {
        return Err("CoW hook violated the one-shot contract");
    }
    Ok(())
}
'''
    replace(b, FAULT, '    pub static FAULT_VMM_MAP: FaultPoint = FaultPoint::new("vmm::map_page");', '''    pub static FAULT_VMM_MAP: FaultPoint = FaultPoint::new("vmm::map_page");

    /// Existing CoW hook; not one of the six catalogued scenarios.
    pub static FAULT_VMM_COW: FaultPoint = FaultPoint::new("vmm::cow_fault");''')
    replace(b, FAULT, "pub use points::*;\n", '''pub use points::*;

mod markers;
pub use markers::{mark_fired, mark_post_recovery, mark_recovered};
''')
    replace(b, MAP, "use super::vma::{insert_vma, page_align_up};",
            "use super::vma::{insert_vma, page_align_up, remove_vma};")
    b[MARKERS] = MARKER_SOURCE
    full = dict(b)
    replace(full, PMM, "pub fn alloc_page() -> Option<usize> {\n", '''pub fn alloc_page() -> Option<usize> {
    #[cfg(feature = "fault-inject")]
    if crate::mm::pmm_fault::check_alloc_frame() {
        crate::fault_inject::mark_fired("pmm_alloc_oom_once");
        // No allocator mutation occurred, so there is nothing to unwind.
        crate::fault_inject::mark_recovered("pmm_alloc_oom_once");
        return None;
    }
''')
    replace(full, PMM, "pub fn alloc_pages_contig(n: usize) -> Option<usize> {\n", '''pub fn alloc_pages_contig(n: usize) -> Option<usize> {
    #[cfg(feature = "fault-inject")]
    if n != 0 && crate::mm::pmm_fault::check_alloc_contiguous() {
        crate::fault_inject::mark_fired("pmm_contiguous_alloc_once");
        // No frames or allocator metadata have been changed.
        crate::fault_inject::mark_recovered("pmm_contiguous_alloc_once");
        return None;
    }
''')
    replace(full, MAP, "    // Allocate a virtual address region.", '''    #[cfg(feature = "fault-inject")]
    if crate::mm::vmm_fault::check_alloc_region() {
        crate::fault_inject::mark_fired("vmm_reserve_once");
        // Fail before next_va, MAP_FIXED VMAs, or page tables are changed.
        crate::fault_inject::mark_recovered("vmm_reserve_once");
        return -12;
    }

    // Allocate a virtual address region.''')
    replace(full, MAP, '''        // Map each page; on failure roll back and remove the VMA.
        let mut mapped = 0usize;
        for page_va in (va..va + len).step_by(PAGE) {
            match crate::mm::pmm::alloc_page() {
                Some(pa) => {
                    <Arch as Paging>::map_page(user_cr3, page_va, pa, pte_flags);
                    mapped += 1;
                },
                None => {
                    // Roll back partially mapped pages.
                    for rollback_va in (va..va + mapped * PAGE).step_by(PAGE) {
                        if let Some(pa) = <Arch as Paging>::virt_to_phys(user_cr3, rollback_va) {
                            <Arch as Paging>::unmap_page(user_cr3, rollback_va);
                            crate::mm::pmm::free_page(pa);
                        }
                    }
                    // Remove the inserted VMA and shoot down TLB entries.
                    remove_vma(pid, va, len);
                    return -12;
                },
            }
        }''', '''        // Real and injected map errors share the same rollback path.
        let mut mapped = 0usize;
        for page_va in (va..va + len).step_by(PAGE) {
            let pa = match crate::mm::pmm::alloc_page() {
                Some(pa) => pa,
                None => {
                    rollback_anonymous_map(pid, user_cr3, va, len, mapped);
                    return -12;
                },
            };
            #[cfg(feature = "fault-inject")]
            let injected = crate::mm::vmm_fault::check_map_page();
            #[cfg(not(feature = "fault-inject"))]
            let injected = false;
            #[cfg(feature = "fault-inject")]
            if injected {
                crate::fault_inject::mark_fired("vmm_map_once");
            }
            if injected || !<Arch as Paging>::map_page(user_cr3, page_va, pa, pte_flags) {
                // This frame was not installed; prior frames need invalidation.
                crate::mm::pmm::free_page(pa);
                let cleaned = rollback_anonymous_map(pid, user_cr3, va, len, mapped);
                #[cfg(feature = "fault-inject")]
                if injected && cleaned {
                    crate::fault_inject::mark_recovered("vmm_map_once");
                }
                let _ = cleaned;
                return -12;
            }
            mapped += 1;
        }''')
    replace(full, MAP, "#[inline]\nfn va_end_of", '''// Allocation-free rollback of installed data frames and the inserted VMA.
// Empty intermediate page-table ownership remains with the paging backend.
fn rollback_anonymous_map(pid: usize, cr3: usize, va: usize, len: usize, mapped: usize) -> bool {
    let mut complete = true;
    for rollback_va in (va..va + mapped * PAGE).step_by(PAGE) {
        if let Some(pa) = <Arch as Paging>::unmap_page(cr3, rollback_va) {
            crate::smp::ipi::tlb_shootdown(rollback_va as u64, (rollback_va + PAGE) as u64, 0);
            crate::mm::pmm::free_page(pa);
        } else {
            complete = false;
        }
    }
    remove_vma(pid, va, len);
    crate::smp::ipi::tlb_shootdown(va as u64, (va + len) as u64, 0);
    if !complete {
        crate::kprintln!("RUSTOS_EXEC marker=vmm_rollback_failed");
    }
    complete
}

#[inline]
fn va_end_of''')
    replace(full, PIPE, '''        if (open_count + 2) as u64 > soft {
            PIPE_TABLE.lock().remove(read_bfd);''', '''        #[cfg(feature = "fault-inject")]
        let injected_emfile = crate::syscall::fault::check_fdtable().is_err();
        #[cfg(not(feature = "fault-inject"))]
        let injected_emfile = false;
        #[cfg(feature = "fault-inject")]
        if injected_emfile {
            crate::fault_inject::mark_fired("syscall_emfile_once");
        }
        if injected_emfile || (open_count + 2) as u64 > soft {
            PIPE_TABLE.lock().remove(read_bfd);''')
    replace(full, PIPE, '''            crate::fs::scheme_fd::scheme_fd_close(scheme_write_bfd);
            return EMFILE;''', '''            crate::fs::scheme_fd::scheme_fd_close(scheme_write_bfd);
            // Release local Arc owners too; no process descriptors were installed.
            drop(scheme);
            drop(pipe_arc);
            #[cfg(feature = "fault-inject")]
            if injected_emfile {
                crate::fault_inject::mark_recovered("syscall_emfile_once");
            }
            return EMFILE;''')
    replace(full, IPC, '''    let msg = match ENDPOINTS
        .lock()
        .get_mut(&endpoint.0)
        .and_then(|ep| ep.to_server.pop_front())
    {
        Some(msg) => msg,
        None => return -11, // EAGAIN; scheduler integration can block later.
    };''', '''    // Release the endpoint-table lock before emitting guest evidence.
    let queued = {
        let mut endpoints = ENDPOINTS.lock();
        endpoints.get_mut(&endpoint.0).and_then(|ep| ep.to_server.pop_front())
    };
    let msg = match queued {
        Some(msg) => msg,
        None => {
            #[cfg(feature = "fault-inject")]
            if crate::syscall::fault::check_resource().is_err() {
                crate::fault_inject::mark_fired("syscall_eagain_once");
                // No payload was consumed or process state changed.
                crate::fault_inject::mark_recovered("syscall_eagain_once");
            }
            return -11; // EAGAIN; scheduler integration can block later.
        },
    };''')
    heading = "## Current evidence boundary\n"
    if full[DOC].count(heading) != 1:
        raise ValueError("ambiguous evidence boundary")
    full[DOC] = full[DOC].split(heading)[0].replace(
        "acceptance contract for future guest fault-integration work.",
        "acceptance contract for guest fault-integration work.") + EVIDENCE
    return ({name: text.encode("utf-8") for name, text in b.items()},
            {name: text.encode("utf-8") for name, text in full.items()})


def preflight(root, originals, blockers, full, phase):
    desired = blockers if phase == "blockers" else full
    changes = {}
    for name, data in desired.items():
        path = root / name
        if Path(name).is_absolute() or ".." in Path(name).parts or path.is_symlink():
            raise ValueError(f"{name}: unsafe target")
        if not path.resolve().is_relative_to(root):
            raise ValueError(f"{name}: target outside repository")
        old = path.read_bytes() if path.exists() else None
        if old not in (originals.get(name), blockers.get(name), full.get(name)):
            raise ValueError(f"{name}: source drift; refusing overwrite")
        # Never downgrade a completed full application during a blocker recheck.
        if old == full.get(name):
            continue
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
    parser.add_argument("--phase", choices=("blockers", "all"), default="all")
    args = parser.parse_args()
    root = Path.cwd().resolve()
    originals = {}
    for name, expected in EXPECTED.items():
        data = subprocess.check_output(["git", "show", f"{BASE}:{name}"])
        if blob_sha(data) != expected:
            raise ValueError(f"{name}: immutable baseline hash mismatch")
        originals[name] = data
    blockers, full = build(originals)
    changes = preflight(root, originals, blockers, full, args.phase)
    if args.apply:
        write_changes(root, changes)
    print(json.dumps({"applied": args.apply, "phase": args.phase, "changed": sorted(changes),
                      "full_blob_shas": {name: blob_sha(data) for name, data in full.items()}}, indent=2))


MARKER_SOURCE = r'''//! Standalone guest fault protocol; host unit tests never emit guest evidence.
use core::fmt;

const SCENARIOS: &[&str] = &[
    "pmm_alloc_oom_once", "pmm_contiguous_alloc_once", "vmm_map_once",
    "vmm_reserve_once", "syscall_emfile_once", "syscall_eagain_once",
];

struct Record<'a> {
    marker: &'static str,
    scenario: &'a str,
    workload: Option<&'a str>,
}

impl fmt::Display for Record<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RUSTOS_FAULT marker={} scenario={}", self.marker, self.scenario)?;
        if let Some(workload) = self.workload {
            write!(f, " workload={} exit=0", workload)?;
        }
        Ok(())
    }
}

fn emit(record: Record<'_>) {
    #[cfg(not(test))]
    crate::kprintln!("{}", record);
    #[cfg(test)]
    let _ = record;
}

pub fn mark_fired(scenario: &'static str) -> bool {
    if !SCENARIOS.contains(&scenario) {
        return false;
    }
    emit(Record { marker: "fired", scenario, workload: None });
    true
}

pub fn mark_recovered(scenario: &'static str) -> bool {
    if !SCENARIOS.contains(&scenario) {
        return false;
    }
    emit(Record { marker: "recovered", scenario, workload: None });
    true
}

/// The caller must supply a real, checked guest follow-up workload result.
/// Production fault sites must not call this merely because an errno returned.
pub fn mark_post_recovery(scenario: &'static str, workload: &str, exit: i32) -> bool {
    if !SCENARIOS.contains(&scenario) || exit != 0 || workload.is_empty()
        || !workload.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    {
        return false;
    }
    emit(Record { marker: "post_recovery_pass", scenario, workload: Some(workload) });
    true
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use self::std::format;

    #[test]
    fn exact_fired_records_for_every_scenario() {
        for &scenario in SCENARIOS {
            let record = Record { marker: "fired", scenario, workload: None };
            assert_eq!(format!("{}\n", record), format!("RUSTOS_FAULT marker=fired scenario={}\n", scenario));
        }
    }

    #[test]
    fn exact_recovered_records_for_every_scenario() {
        for &scenario in SCENARIOS {
            let record = Record { marker: "recovered", scenario, workload: None };
            assert_eq!(format!("{}\n", record), format!("RUSTOS_FAULT marker=recovered scenario={}\n", scenario));
        }
    }

    #[test]
    fn exact_post_recovery_record() {
        let record = Record { marker: "post_recovery_pass", scenario: SCENARIOS[0], workload: Some("alloc_retry") };
        assert_eq!(format!("{}\n", record), "RUSTOS_FAULT marker=post_recovery_pass scenario=pmm_alloc_oom_once workload=alloc_retry exit=0\n");
    }

    #[test]
    fn unknown_scenarios_cannot_emit_records() {
        assert!(!mark_fired("unknown"));
        assert!(!mark_recovered("unknown"));
        assert!(!mark_post_recovery("unknown", "retry", 0));
    }

    #[test]
    fn post_recovery_rejects_failure_and_malformed_workloads() {
        for exit in [-1, 1, 127] {
            assert!(!mark_post_recovery(SCENARIOS[0], "retry", exit));
        }
        for workload in ["", "two words", "line\nbreak", "bad=token", "nonascii-é"] {
            assert!(!mark_post_recovery(SCENARIOS[0], workload, 0));
        }
    }

    #[test]
    fn valid_inputs_are_accepted_without_emitting_guest_lines_in_host_tests() {
        for &scenario in SCENARIOS {
            assert!(mark_fired(scenario));
            assert!(mark_recovered(scenario));
            assert!(mark_post_recovery(scenario, "retry.roundtrip-1", 0));
        }
    }
}
'''

EVIDENCE = '''## Current evidence boundary

Production sites now call the feature-gated protocol helpers at these boundaries:

| Scenario | Site | Recovered boundary |
|---|---|---|
| pmm_alloc_oom_once | alloc_page() | Before allocator mutation; no unwind needed |
| pmm_contiguous_alloc_once | Nonzero alloc_pages_contig() | Before frame allocation or metadata mutation |
| vmm_map_once | Anonymous sys_mmap() page loop | Current unmapped frame freed; earlier mappings removed and invalidated before returning data frames; inserted VMA removed |
| vmm_reserve_once | Before the sys_mmap() reservation closure | next_va, VMAs, and page tables remain untouched |
| syscall_emfile_once | pipe2 RLIMIT_NOFILE branch | Pipe registrations removed, scheme backing resources closed, local Arc owners dropped; process descriptors not installed |
| syscall_eagain_once | sys_ipc_recv() empty-queue branch | Queue lock released; no payload consumed or process state changed |

Fired is emitted only when the corresponding production hook's check() consumes
an armed fault. Natural failures do not claim injected evidence. Recovered is
emitted only after the site's bounded cleanup (or its pre-mutation boundary).
A missing rollback unmap suppresses VMM recovered and emits a failure marker.
The EAGAIN scenario is deliberately limited to the existing empty-queue path,
not interrupted delivery of a queued payload.

These callsites and compile checks do not substantiate a booted guest recovery
claim. VMM data-frame/VMA rollback does not establish reclamation of empty
intermediate page-table pages, restoration of displaced MAP_FIXED mappings, or
restoration of the nonfixed next_va cursor. The stronger catalogued acceptance
requirements still need real workload and ownership evidence.

mark_post_recovery accepts only a catalogued scenario, a nonempty token-shaped
workload name, and exit zero. Those checks cannot establish that a workload
actually ran: that responsibility belongs to the guest harness. No production
injection site automatically calls it. An errno return or one-shot disarming
must never be promoted to post_recovery_pass.

scripts/ci/tests/fault_engine.rs records scope=host_engine. The registered kmtest
fault suite exercises hooks, and the marker unit tests exercise formatting and
input rejection. Neither is a guest recovery workload. Host unit tests emit no
guest protocol lines through the helpers. FAULT_VMM_COW supports the existing
CoW hook but is not an additional catalogue scenario.

.github/workflows/fault-inject.yml retains its unsupported readiness guard.
Do not enable guest integration until an arm/control harness executes the six
scenarios and retains real QEMU logs, metadata, and post-recovery workload results.
The strict parser's --require-fault-scenarios mode validates evidence; it neither
arms faults nor creates follow-up workloads.
'''

if __name__ == "__main__":
    main()
