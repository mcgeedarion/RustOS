# RustOS Documentation Index

This directory contains the current project documentation. Outdated one-off implementation summaries and fictional release/install notes have been removed; use the files below as the maintained source of truth.

## Authoritative references

| File | Use it for |
| --- | --- |
| `status.md` | Current subsystem maturity, feature gates, and roadmap priorities. |
| `architecture.md` | Architecture support, boot contracts, and code organization rules. |
| `milestones.md` | M1-M5 product milestones and supported boot sentinels. |
| `syscalls.md` | Syscall implementation status, EFAULT-safety expectations, and intentional ENOSYS stubs. |
| `production_requirements.md` | Release-readiness requirements and current gaps. |

## Developer workflow

| File | Use it for |
| --- | --- |
| `getting-started.md` | Local setup and first validation commands. |
| `ci.md` | CI workflow overview and expected gates. |
| `code_coverage.md` | Coverage collection guidance. |
| `rustdoc_generation.md` | API documentation generation. |
| `fault_inject.md` | Fault-injection feature, fault IDs, and test workflow. |
| `ATOMIC_ORDERING_GUIDELINES.md` | Atomic ordering conventions. |
| `ERROR_HANDLING_AUDIT.md` | Error-handling audit status and remaining follow-up. |

## Boot, performance, and architecture notes

| File | Use it for |
| --- | --- |
| `boot-image-size.md` | Boot image size tracking. |
| `boot-optimization-checklist.md` | Boot performance optimization checklist. |
| `boot-perf.md` | Boot marker format and performance collection policy. |
| `compiler-optimizations.md` | Compiler/profile optimization notes. |
| `syscall-perf.md` | Syscall profiling workflow and data freshness policy. |
| `architecture-improvements.md` | Follow-up architecture cleanup ideas. |

## Architecture Decision Records

ADRs live in `docs/adr/` and record accepted design direction. Keep them as historical decisions; update living status in `status.md` rather than editing ADR history unless the decision itself changes.
