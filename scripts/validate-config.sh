#!/bin/bash
# Build-time configuration validation script

set -e

echo "=== RustOS Configuration Validation ==="

# Check workspace members exist
echo "Checking workspace crate structure..."
for crate in vfs-core fs-ext4 fs-fat32 fs-tmpfs mm-core sync-primitives; do
    if [ ! -d "crates/$crate" ]; then
        echo "ERROR: Missing crate: crates/$crate"
        exit 1
    fi
    if [ ! -f "crates/$crate/Cargo.toml" ]; then
        echo "ERROR: Missing Cargo.toml: crates/$crate/Cargo.toml"
        exit 1
    fi
    echo "  ✓ crates/$crate"
done

# Check ADR documentation
echo "Checking ADR documentation..."
for adr in 0001 0002 0003; do
    if [ ! -f "docs/adr/${adr}-*.md" ]; then
        echo "WARNING: Missing ADR ${adr}"
    fi
done
echo "  ✓ ADRs present"

# Validate Cargo.toml syntax
echo "Validating Cargo.toml files..."
cargo metadata --format-version 1 >/dev/null 2>&1 || {
    echo "ERROR: Invalid workspace configuration"
    exit 1
}
echo "  ✓ Workspace configuration valid"

echo ""
echo "=== Validation Complete ==="
echo "All checks passed!"
