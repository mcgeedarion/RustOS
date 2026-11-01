# Security

The full kernel contains security mechanisms and partial Linux-like policy interfaces, but no production-hardening certification is implied. This guide describes selected code at `08a14f6` and calls out limitations that affect how those mechanisms may be documented.

## Policy and credentials

- **LSM registry**: `security/lsm.rs` defines policy contexts, hooks, verdicts, registration, and dispatch. Enforcement depends on the actual call sites, not just registering a hook type.
- **DAC**: `security/dac.rs` evaluates read/write/execute and metadata operations against credentials and capability state. The VFS also has its own permission/preflight helpers.
- **Capability sets**: `security/capset.rs` is declared by the security root. The separate `security/capabilities.rs` is not declared there and must not be treated as the active capability implementation.
- **Process credentials**: `proc/creds.rs` and PCB fields provide identity operations. `proc::current_creds` falls back to root credentials when no current process is found, which requires careful use outside kernel-internal contexts.
- **Namespaces**: `security/ns/` contains mount, network, PID, user, UTS, and isolation helpers. Additional process namespace modules coexist; `security/namespace/user.rs` is not declared by the security root.
- **Cgroups**: `security/cgroups/` is gated by the root `cgroups` feature and implements CPU/memory/PID controller state. Process cgroup bookkeeping and cgroupfs remain separate integration points.

Namespace existence is not proof of isolation. Review default enforcement state, per-object ownership, mount/socket access checks, and privilege transitions at each caller before claiming containment.

## Syscall filtering and restricted interfaces

`seccomp.rs` implements a classic-BPF-style filter interpreter and verdict handling, and syscall dispatch runs policy checks before routing. Its `TRACE` behavior is not a complete tracing service, and `USER_NOTIF` denies with `ENOSYS` when no notification listener is implemented.

`security/bpf.rs` provides a limited map-oriented `bpf` syscall surface. Program loading, a verifier-backed execution engine, and general eBPF attachment are not supported; unsupported operations must not be described as completed BPF support.

`landlock.rs` contains ruleset/path-beneath/domain and selected LSM enforcement logic. That is a subset implementation, not a claim of conformance with every Landlock ABI revision or filesystem operation.

## Memory hardening

- **User copies**: `kernel/uaccess.rs` validates ranges and mappings and coordinates architecture protection while copying. Callers must use these boundaries instead of dereferencing untrusted pointers directly.
- **ASLR**: `security/aslr.rs` generates randomized layouts using the kernel random facility. Layout code and seed sources alone do not establish cryptographic-strength entropy or complete relocation coverage.
- **Canaries**: `security/canary.rs` maintains expected canary state and failure handling. Compiler insertion and all stack-protection call sites require separate verification.
- **SMEP/SMAP**: `security/smep_smap.rs` probes/enables supported x86 protections and provides access-window helpers. Non-x86 behavior and unsupported CPUs are separate paths.
- **PTI**: `security/pti.rs` contains x86 shadow page-table and switch helpers with non-x86 alternatives. End-to-end isolation depends on entry/exit assembly and mappings, not the helper file alone.
- **KASAN-like helpers**: `mm/kasan.rs` is a declared MM module. The root Cargo manifest does not declare a `kasan` feature, despite references elsewhere, so no supported “enable KASAN” command is established.

## Known boundaries

The full x86_64 graph does not compile in the audited revision. Empty clone/core-dump files, RISC-V signal-return limitations, conservative task retention, incomplete userspace-driver integration, and simulated filesystem backends all prevent a general production-security claim.

The documentation and host tests are development aids. Use [production requirements](production_requirements.md) and [acceptance criteria](acceptance-criteria.md) to define the concrete negative tests, fault recovery, resource limits, and runtime evidence needed before exposing the kernel to untrusted workloads.
