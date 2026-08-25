# userspace/Makefile
# Builds all userspace C programs as static musl-libc ELF64 binaries.
#
# See BUILDING.md for detailed build instructions, prerequisites, and examples.

ARCH       ?= x86_64
BUILD_DIR  := build/$(ARCH)

COMMON_CFLAGS := -static -O2 -Wall -Wextra -std=c11

ifeq ($(ARCH),x86_64)
  CC           := musl-gcc
  ARCH_CFLAGS  := -ffreestanding
else ifeq ($(ARCH),aarch64)
  CC           := aarch64-linux-musl-gcc
  ARCH_CFLAGS  := -march=armv8-a
else ifeq ($(ARCH),riscv64)
  CC           := riscv64-linux-musl-gcc
  ARCH_CFLAGS  := -march=rv64gc
else
  $(error Unsupported ARCH=$(ARCH). Use x86_64, aarch64, or riscv64)
endif

CFLAGS := $(COMMON_CFLAGS) $(ARCH_CFLAGS) -fno-stack-protector

# Wayland compositor requires special CFLAGS for security and compatibility.
# See wayland target comments below for detailed rationale.
WAYLAND_CFLAGS := $(COMMON_CFLAGS) $(ARCH_CFLAGS) -D_GNU_SOURCE -idirafter /usr/include -idirafter /usr/include/$(shell $(CC) -dumpmachine) -fstack-protector-strong

# List every subdirectory that contains a C program here.
PROGRAMS := init hello smoke kmtest wayland

.PHONY: all clean install help check-tools $(PROGRAMS)

# ============================================================================
# Default target and tool validation
# ============================================================================

all: check-tools $(PROGRAMS)

help:
	@echo "RustOS Userspace Build System"
	@echo ""
	@echo "Usage: make [ARCH=x86_64|aarch64|riscv64] [target]"
	@echo ""
	@echo "Available targets:"
	@echo "  all      - Build all userspace binaries (default)"
	@echo "  install  - Install binaries to DESTDIR (default: build/<arch>/rootfs)"
	@echo "  clean    - Remove all build artifacts"
	@echo "  help     - Show this help message"
	@echo ""
	@echo "See BUILDING.md for detailed instructions and examples."
	@echo ""
	@echo "Examples:"
	@echo "  make ARCH=aarch64                 # Build AArch64 binaries"
	@echo "  make ARCH=riscv64                 # Build RISC-V 64-bit binaries"
	@echo "  make                              # Build x86_64 binaries"
	@echo "  make install DESTDIR=/tmp/rootfs  # Install to custom location"

check-tools:
	@command -v $(CC) >/dev/null 2>&1 || { \
		echo "Error: $(CC) not found. Please install the required toolchain."; \
		echo "For AArch64: Build musl from source targeting aarch64-linux-musl or provide aarch64-linux-musl-gcc"; \
        echo "For x86_64: apt install musl-tools"; \
        echo "For riscv64: apt install musl-tools or build musl from source"; \
		exit 1; \
	}

# ============================================================================
# Build directory setup
# ============================================================================

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

# ============================================================================
# Program build rules (generic pattern + special cases)
# ============================================================================

# Standard build rule for simple programs
$(BUILD_DIR)/init: init/init.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -o $@ $<

$(BUILD_DIR)/hello: hello/hello.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -o $@ $<

# Minimal QEMU smoke binary:
#   - prints a well-known marker to the serial console
#   - returns 0 on success
#   - can be extended later with basic invariants (e.g. /proc/1 exists).
$(BUILD_DIR)/smoke: smoke/smoke.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -o $@ $<

# Kernel test runner:
#   - calls SYS_KMTEST_LIST to discover registered tests
#   - calls SYS_KMTEST_RUN per test (or a named subset from argv)
#   - exits non-zero on any failure, making it CI-friendly
#   - _GNU_SOURCE required for snprintf in strict -std=c11 mode with musl
$(BUILD_DIR)/kmtest: kmtest/kmtest.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -D_GNU_SOURCE -o $@	$<

# The Wayland compositor is a privileged userspace process.
# It is installed as /usr/bin/rustos-compositor in the initramfs.
# The kernel spawns it as PID 2 via execve() after init (PID 1) starts.
# See BUILDING.md for detailed CFLAGS rationale.
$(BUILD_DIR)/rustos-compositor: wayland/compositor.c | $(BUILD_DIR)
	$(CC) $(WAYLAND_CFLAGS) -o $@ $<

# ============================================================================
# Convenience targets (map to actual binaries)
# ============================================================================

init: $(BUILD_DIR)/init

hello: $(BUILD_DIR)/hello

smoke: $(BUILD_DIR)/smoke

kmtest: $(BUILD_DIR)/kmtest

wayland: $(BUILD_DIR)/rustos-compositor

# ============================================================================
# Installation and cleanup
# ============================================================================

# Copies built binaries into a staging rootfs tree.
# See BUILDING.md for detailed layout and usage.
DESTDIR ?= $(BUILD_DIR)/rootfs
install: all
	mkdir -p $(DESTDIR)/bin $(DESTDIR)/usr/bin
	cp $(BUILD_DIR)/init                $(DESTDIR)/init
	cp $(BUILD_DIR)/hello               $(DESTDIR)/bin/hello
	cp $(BUILD_DIR)/smoke               $(DESTDIR)/bin/smoke
	cp $(BUILD_DIR)/kmtest              $(DESTDIR)/bin/kmtest
	cp $(BUILD_DIR)/rustos-compositor   $(DESTDIR)/usr/bin/rustos-compositor

clean:
	rm -rf build/
