# rustos_config.mak — included by the musl Makefile after configure.
#
# Overrides the sysdep layer so musl issues syscalls the way RustOS expects:
#
#   x86_64:  syscall instruction, number in rax, args in rdi,rsi,rdx,r10,r8,r9
#            errno returned as negative value in rax
#   aarch64: svc #0 instruction, number in x8, args in x0-x5
#            errno returned as negative value in x0
#
# Additionally pins the sysroot and disables features that require
# kernel support RustOS has not yet implemented.

# ── syscall ABI ────────────────────────────────────────────────────────────────

# Use the architecture-specific assembly syscall wrappers we ship under
# arch/<arch>/syscall_shim.s rather than musl's generic C fallback.
SYSCALL_STYLE := asm

# ── sysroot path ───────────────────────────────────────────────────────────────

# Point at our pre-built sysroot so headers resolve correctly.
SYSROOT := $(CURDIR)/userspace/musl/sysroot

# ── disabled features ──────────────────────────────────────────────────────────

# No kernel-side inotify support yet.
HAVE_INOTIFY := 0

# No in-kernel POSIX message queue support.
HAVE_POSIX_MQ := 0

# Note: x86_64 requires assembly implementations of longjmp;
#       aarch64 uses C implementations instead.
SYSDEP_FILES   := syscall.s setjmp.s longjmp.s clone.s
