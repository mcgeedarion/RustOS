# RustOS parser fuzzing

This is a separate `cargo-fuzz` workspace. Targets include the production modules
by source path so fuzzing runs the same parser code used by the kernel, rather
than a duplicate test implementation.

| Target | Production code | Main exercise |
|---|---|---|
| `cpio_newc` | `src/init/initramfs/mod.rs` | newc headers, entry iteration, lengths, alignment, file lookup |
| `elf64` | `src/exec/elf.rs` | ELF64 header validation, program-header bounds, load metadata |
| `vfs_path` | `src/fs/vfs/symlink.rs` | path normalization, missing/non-directory components, symlink depth and `O_NOFOLLOW` |
| `ext4_metadata` | `src/fs/ext4.rs` | production superblock decode, checked little-endian reads, block offsets, directory-entry metadata |

The fuzz-only ext4 entry point calls the same internal decoders as the mount and
directory lookup paths. It intentionally does not mount an arbitrary disk image
or allocate memory using disk-provided size fields.

## Local use

Install a nightly toolchain and `cargo-fuzz`, then run:

```sh
cargo fuzz run cpio_newc -- -max_len=4096 -max_total_time=60 -dict=fuzz/dictionaries/cpio_newc.dict
cargo fuzz run elf64 -- -max_len=4096 -max_total_time=60 -dict=fuzz/dictionaries/elf64.dict
cargo fuzz run vfs_path -- -max_len=4096 -max_total_time=60 -dict=fuzz/dictionaries/vfs_path.dict
cargo fuzz run ext4_metadata -- -max_len=4096 -max_total_time=60 -dict=fuzz/dictionaries/ext4_metadata.dict
```

Seed files live under `fuzz/corpus/<target>/`. LibFuzzer sanitizer instrumentation
is enabled by the `cargo-fuzz` build. Crashes and minimized reproducers must be
committed as regression fixtures. A passing bounded campaign is useful evidence
for these parser inputs only. It does not prove a full-kernel boot, filesystem
write support, or userspace syscall compatibility.
