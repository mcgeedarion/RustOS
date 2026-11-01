# Debugging

RustOS offers early serial output, panic/oops formatting, optional trace code, syscall counters, and incomplete interactive/GDB integration. These surfaces are not equally usable in every profile, and the full graph's compile failures limit current runtime debugging.

## Early output and panic records

`console/` and architecture serial implementations provide output before full TTY initialization. `src/lib.rs` owns the non-test panic handler; the panic parser and its host tests live under `scripts/ci/`.

Use the log from the actual command being investigated and keep compiler, firmware, kernel serial, and userspace records distinct. The full smoke parser rejects recognized panic signatures instead of allowing a later success marker to mask a panic.

## Debug features

| Feature/surface | Actual boundary |
| --- | --- |
| `debug` | Enables `gdbstub` and `trace`, plus command/oops paths selected by cfg |
| `gdbstub` / `debugstub` | Selects `debug/gdbstub`, whose `mod.rs` is currently empty |
| `trace` | Selects trace/ftrace source; compiler instrumentation remains a separate requirement |
| `kmtest` | Selects in-kernel test and oops-related code in full builds |
| `syscall-trace` | Selects cumulative syscall counters/ticks in `syscall/profile.rs` |
| `boot_debug` | Enables particular guarded diagnostic calls, not every possible log output |
| `debugcon` | x86-specific QEMU-port output source; do not infer real-hardware behavior from a source comment |

The source files for GDB RSP parsing, targets, sessions, breakpoints, and transport remain in the tree but are not declared by the empty GDB module root. Several GDB architecture files are also empty; no working in-kernel remote debugger is established.

## QEMU debugging versus an in-kernel stub

`cargo xtask debug` uses QEMU's debugging launch mode and is distinct from the `gdbstub` feature. Read `cargo xtask help` for supported arguments and keep the kernel/profile choice explicit.

```bash
cargo xtask debug --arch x86_64 --features boot_minimal
```

A host GDB session against QEMU can inspect guest state without proving RustOS's own remote protocol implementation works. Symbol lookup also depends on whether the selected profile retained debug information.

## Trace, profiling, and interactive commands

`debug/trace.rs` owns trace-ring behavior, and `debug/ftrace.rs` provides function-entry/exit hooks. `syscall/profile.rs` records call counts and cumulative hardware ticks, not full latency distributions; see [syscall performance](syscall-perf.md).

The debug command parser recognizes process/memory inspection, backtrace, and memory dumping commands. Its source references `tty::serial_ldisc`, which is not declared by the TTY root, so the advertised REPL is not a validated usable interface.

`fs/proc_debug.rs` implements process memory/register/control backing descriptors with opener/target state. These are security-sensitive facilities and require access checks and lifetime review, not just documentation of their names.

## Useful reproduction sequence

```bash
cargo xtask check --arch x86_64 --features boot_minimal
cargo xtask check --arch x86_64
cargo xtask lint-modules
python3 -m unittest discover -s scripts/ci/tests -p 'test_panic_output.py'
```

The first command isolates the small graph; the second intentionally reproduces full integration failures at this revision. Do not add features or weaken the parser until the failing command's contract is understood.
