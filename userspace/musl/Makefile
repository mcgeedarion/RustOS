# Build musl libc for RustOS
#
# Prerequisites (Debian/Ubuntu):
#   apt install musl-tools clang lld python3
#
# Usage:
#   make help      — show this help message
#   make x86_64    — build libmusl.a + libc.so for x86_64
#   make all       — build all supported architectures
#   make sysroot   — install musl headers into sysroot/<arch>/include/
#   make clean     — remove build artifacts
#   make distclean — remove everything including sysroot

MUSL_VERSION   ?= 1.2.5
MUSL_SRC       ?= musl-$(MUSL_VERSION)
MUSL_TARBALL   ?= $(MUSL_SRC).tar.gz
MUSL_URL       := https://musl.libc.org/releases/$(MUSL_TARBALL)
MUSL_SHA256    ?=

TOPDIR         := $(shell pwd)
SYSROOT        := $(TOPDIR)/sysroot
PATCH_DIR      := $(TOPDIR)/patches
VERBOSE        ?= 0
JOBS           ?= $(shell nproc 2>/dev/null || echo 4)

X86_CC         ?= clang --target=x86_64-unknown-linux-musl

ifeq ($(VERBOSE),1)
  ECHO = @true
  HIDE =
else
  ECHO = @echo
  HIDE = @
endif

$(MUSL_TARBALL):
	$(ECHO) "[DOWNLOAD] Fetching musl-$(MUSL_VERSION)..."
	$(HIDE)wget -q $(MUSL_URL) -O $(MUSL_TARBALL)
	$(ECHO) "[VERIFY] Validating tarball checksum..."
	$(HIDE)if [ -n "$(MUSL_SHA256)" ]; then \
		echo "$(MUSL_SHA256)  $(MUSL_TARBALL)" | sha256sum -c -; \
	else \
		echo "[WARN] No MUSL_SHA256 specified, skipping verification"; \
	fi

$(MUSL_SRC)/.patched: $(MUSL_TARBALL)
	$(ECHO) "[EXTRACT] Extracting musl-$(MUSL_VERSION)..."
	$(HIDE)set -e; \
	tar -xzf $(MUSL_TARBALL); \
	if [ -d "$(PATCH_DIR)" ] && [ $$(find $(PATCH_DIR) -name "*.patch" -type f 2>/dev/null | wc -l) -gt 0 ]; then \
		echo "[PATCH] Applying patches..."; \
		cd $(MUSL_SRC) && \
		for p in $(PATCH_DIR)/*.patch; do \
			echo "  Applying $$p..."; \
			patch -p1 < $$p || exit 1; \
		done; \
	else \
		echo "[PATCH] No patches to apply"; \
	fi
	$(HIDE)touch $@

BUILD_X86 := build-x86_64

x86_64: $(MUSL_SRC)/.patched
	$(ECHO) "[BUILD] x86_64 musl libc..."
	$(HIDE)mkdir -p $(BUILD_X86)
	$(HIDE)cd $(BUILD_X86) && \
	  ../$(MUSL_SRC)/configure \
	    CC="$(X86_CC)" \
	    LDFLAGS="-fuse-ld=lld" \
	    --target=x86_64 \
	    --prefix=$(SYSROOT)/x86_64 \
	    --syslibdir=$(SYSROOT)/x86_64/lib \
	    --disable-shared \
	    $(MUSL_EXTRA_FLAGS) && \
	  $(MAKE) -f $(TOPDIR)/rustos_config.mak ARCH=x86_64 -j$(JOBS) install
	$(ECHO) "[OK] x86_64 build complete"

all: x86_64
	$(ECHO) "[OK] All supported architectures built successfully"

sysroot: all
	$(ECHO) "[INSTALL] Installing musl headers into sysroot..."
	$(HIDE)mkdir -p $(SYSROOT)/x86_64/include/rustos
	$(HIDE)cp -r $(BUILD_X86)/include/.  $(SYSROOT)/x86_64/include/rustos/
	$(ECHO) "[OK] Headers installed"

clean:
	$(ECHO) "[CLEAN] Removing build artifacts..."
	$(HIDE)rm -rf $(BUILD_X86)
	$(ECHO) "[OK] Build artifacts removed"

distclean: clean
	$(ECHO) "[DISTCLEAN] Removing sysroot and downloaded sources..."
	$(HIDE)rm -rf $(SYSROOT) $(MUSL_SRC) $(MUSL_TARBALL)
	$(ECHO) "[OK] Sysroot removed"

help:
	@echo "RustOS musl libc Build System"
	@echo ""
	@echo "Usage: make [target] [VARIABLE=value]"
	@echo ""
	@echo "Targets:"
	@echo "  help               — Display this help message"
	@echo "  x86_64             — Build musl for x86_64 architecture"
	@echo "  all                — Build for all supported architectures (default)"
	@echo "  sysroot            — Install headers into sysroot/<arch>/include/"
	@echo "  clean              — Remove build directories"
	@echo "  distclean          — Remove everything including sysroot"

.PHONY: all x86_64 sysroot clean distclean help
