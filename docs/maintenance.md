# Documentation maintenance

The docs are source-revision-scoped and distinguish implementation, integration, compilation, and runtime evidence. This refresh uses `08a14f6` as its source baseline; a later documentation-only commit does not invalidate that source snapshot.

## Update checklist

- **Inventory**: Enumerate tracked Rust, C, and assembly files with `git ls-files`; account for every file in [module-map.md](module-map.md).
- **Reachability**: Inspect `mod`, `#[path]`, `include!`, `global_asm!`, build-script inputs, cfg gates, and workspace membership. “No direct declaration” is not enough if a file is textually included.
- **Contracts**: Follow actual callers through entry, ownership, teardown, and failure paths. Separate source comments about intent from implemented behavior.
- **Features**: Compare all documented switches with the correct package manifest. Root and userspace features are not interchangeable.
- **Validation**: Record command, target/profile/features, source revision, observed result, and what was not tested.
- **Navigation**: Check local Markdown paths/anchors and keep existing document filenames stable unless deliberately migrating their links.
- **Scope**: A documentation refresh should not silently alter source, Cargo manifests, lockfiles, workflow definitions, or generated artifacts.

## Required repository guards

```bash
cargo xtask roadmap-check
cargo xtask lint-modules
bash scripts/ci/check-stubs.sh
git diff --check
git diff --name-only
```

The roadmap guard expects literal topics in status, syscalls, milestones, architecture, and fault-injection docs. Preserve meaningful explanations of those topics rather than adding empty keywords to satisfy a string check.

## Status vocabulary

| Term | Meaning |
| --- | --- |
| Implemented source | Actual implementation body exists |
| Declared/included | A source edge exists, possibly conditional |
| Unregistered | No current declaration/include connects the identified implementation |
| Stub/no-op | Explicit placeholder, simulated operation, or accepted action with no corresponding effect |
| Check passed | Exact type-check invocation succeeded |
| Build passed | Exact compile/link/package invocation succeeded |
| Host test passed | Specified host harness passed, with its substitutes disclosed |
| Guest passed | Actual guest workload and required parser/metadata contract passed |
| Not run | No execution claim; neither passing nor failing evidence |

Do not use “complete,” “production ready,” “POSIX compliant,” “supported hardware,” or numeric performance claims without the corresponding evidence. In particular, static inventories and literal-topic guards cannot certify implementation completeness.

## Audit reproducibility

The module index is a lexical snapshot with selected symbol names for navigation, not a generated stable ABI reference. Regenerate its paths/counts after source changes and review conditional/duplicate/empty files explicitly.

Keep [source-audit.md](source-audit.md), [status.md](status.md), [CI](ci.md), and [coverage](code_coverage.md) consistent. If validation is blocked, record the primary blocker and stop short of inventing a successful runtime result.
