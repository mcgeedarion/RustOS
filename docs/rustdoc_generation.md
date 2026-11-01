# Generating API documentation

The Markdown guides describe subsystem contracts and limitations, while rustdoc describes the selected Rust compilation graph. There is no checked-in dedicated rustdoc-generation script or rustdoc workflow at `08a14f6`.

## Minimal-profile rustdoc

The following command passed during the audit and generated the selected library documentation. It emitted two existing intra-doc-link warnings from source comments and does not document the excluded full subsystems.

```bash
cargo doc -p rustos-kernel --lib --no-deps \
  --target x86_64-unknown-uefi -Z build-std=core,alloc \
  --no-default-features --features boot_minimal
```

The output is `target/x86_64-unknown-uefi/doc/rustos_kernel/index.html`. Treat generated HTML as a build artifact rather than a Markdown documentation replacement.

## Portable crates

```bash
cargo doc -p scheme-api -p vfs-core -p mm-core -p sync-primitives \
  --no-deps --target x86_64-unknown-linux-gnu \
  -Z build-std=std,panic_abort
```

This is a scoped generation command, not a recorded audit result. Workspace re-exports and feature gates influence which API appears.

## Full graph and feature conflicts

Default full-kernel rustdoc is limited by the same unresolved module/type integration as the full compile check. Do not hide those failures by replacing a full documentation claim with a minimal-profile output.

Do not use `--all-features`: `boot_minimal` and `userspace_boot` are explicitly mutually exclusive. Root `[package.metadata.docs.rs] all-features = true` conflicts with that contract and remains a source/configuration issue outside this docs-only change.

## Keeping generated and handwritten docs aligned

Update [the module index](module-map.md), the appropriate subsystem guide, and validation results when declarations, includes, features, or exported contracts change. Do not edit generated HTML manually or infer implemented behavior solely from an exported type's name.
