#!/bin/bash
# Production Requirements Verification Script
# 
# This script verifies that the production requirements have been properly implemented.
# Run this as part of CI to ensure quality gates are met.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(dirname "$SCRIPT_DIR")"

echo "=== RustOS Production Requirements Verification ==="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

pass_count=0
fail_count=0
warn_count=0

pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((pass_count++))
}

fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((fail_count++))
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
    ((warn_count++))
}

# =============================================================================
# 1. Memory Management Verification
# =============================================================================
echo "--- Memory Management ---"

# Check OOM handler exists
if [ -f "$WORKSPACE_DIR/src/mm/oom_handler.rs" ]; then
    pass "OOM handler module exists"
    
    # Check for key functions
    if grep -q "pub fn handle_oom()" "$WORKSPACE_DIR/src/mm/oom_handler.rs"; then
        pass "handle_oom() function implemented"
    else
        fail "handle_oom() function missing"
    fi
    
    if grep -q "pub fn alloc_page_with_oom()" "$WORKSPACE_DIR/src/mm/oom_handler.rs"; then
        pass "alloc_page_with_oom() function implemented"
    else
        fail "alloc_page_with_oom() function missing"
    fi
    
    if grep -q "MemoryPressure" "$WORKSPACE_DIR/src/mm/oom_handler.rs"; then
        pass "Memory pressure detection implemented"
    else
        fail "Memory pressure detection missing"
    fi
else
    fail "OOM handler module missing (src/mm/oom_handler.rs)"
fi

# Check error handling framework
if [ -f "$WORKSPACE_DIR/src/error_production.rs" ]; then
    pass "Error handling framework exists"
    
    if grep -q "try_opt!" "$WORKSPACE_DIR/src/error_production.rs"; then
        pass "try_opt! macro defined"
    else
        fail "try_opt! macro missing"
    fi
    
    if grep -q "try_result!" "$WORKSPACE_DIR/src/error_production.rs"; then
        pass "try_result! macro defined"
    else
        fail "try_result! macro missing"
    fi
else
    fail "Error handling framework missing (src/error_production.rs)"
fi

# Check mm::init() calls oom_handler::init()
if grep -q "oom_handler::init()" "$WORKSPACE_DIR/src/mm/mod.rs"; then
    pass "OOM handler integrated into mm::init()"
else
    fail "OOM handler not integrated into mm::init()"
fi

echo ""

# =============================================================================
# 2. Filesystem Verification
# =============================================================================
echo "--- Filesystems ---"

# Check ext4 implementation
if [ -f "$WORKSPACE_DIR/src/fs/ext4.rs" ]; then
    pass "ext4 filesystem driver exists"
    
    # Count unwrap() calls in ext4.rs
    unwrap_count=$(grep -c "\.unwrap()" "$WORKSPACE_DIR/src/fs/ext4.rs" 2>/dev/null || echo "0")
    if [ "$unwrap_count" -eq 0 ]; then
        pass "No unwrap() calls in ext4.rs"
    else
        warn "Found $unwrap_count unwrap() calls in ext4.rs"
    fi
    
    # Count expect() calls
    expect_count=$(grep -c "\.expect(" "$WORKSPACE_DIR/src/fs/ext4.rs" 2>/dev/null || echo "0")
    if [ "$expect_count" -eq 0 ]; then
        pass "No expect() calls in ext4.rs"
    else
        warn "Found $expect_count expect() calls in ext4.rs"
    fi
else
    fail "ext4 filesystem driver missing"
fi

# Check jbd2 journaling layer
if [ -f "$WORKSPACE_DIR/src/fs/jbd2.rs" ]; then
    pass "jbd2 journaling layer exists"
else
    warn "jbd2 journaling layer missing (optional for read-only support)"
fi

echo ""

# =============================================================================
# 3. SMP Support Verification
# =============================================================================
echo "--- SMP Support ---"

if [ -f "$WORKSPACE_DIR/src/smp/mod.rs" ]; then
    pass "SMP module exists"
    
    if grep -q "pub static ONLINE_CPUS" "$WORKSPACE_DIR/src/smp/mod.rs"; then
        pass "Online CPU tracking implemented"
    else
        fail "Online CPU tracking missing"
    fi
    
    if grep -q "pub fn ap_entry" "$WORKSPACE_DIR/src/smp/mod.rs"; then
        pass "AP entry point implemented"
    else
        fail "AP entry point missing"
    fi
    
    if grep -q "CoreType" "$WORKSPACE_DIR/src/smp/mod.rs"; then
        pass "Heterogeneous core classification implemented"
    else
        warn "Heterogeneous core classification missing"
    fi
else
    fail "SMP module missing"
fi

# Check for IPI support
if [ -f "$WORKSPACE_DIR/src/smp/ipi.rs" ]; then
    pass "IPI support module exists"
else
    warn "IPI support module missing"
fi

# Check for per-CPU data
if [ -f "$WORKSPACE_DIR/src/smp/percpu.rs" ]; then
    pass "Per-CPU data support exists"
else
    warn "Per-CPU data support missing"
fi

echo ""

# =============================================================================
# 4. IPC Verification
# =============================================================================
echo "--- IPC ---"

if [ -f "$WORKSPACE_DIR/src/proc/futex.rs" ]; then
    pass "Futex implementation exists"
    
    # Check for key futex operations
    ops_found=0
    for op in "FUTEX_WAIT" "FUTEX_WAKE" "FUTEX_LOCK_PI" "FUTEX_UNLOCK_PI" "FUTEX_WAIT_BITSET"; do
        if grep -q "$op" "$WORKSPACE_DIR/src/proc/futex.rs"; then
            ((ops_found++))
        fi
    done
    
    if [ $ops_found -ge 5 ]; then
        pass "All major futex operations implemented ($ops_found/5)"
    else
        warn "Some futex operations missing ($ops_found/5)"
    fi
    
    # Check for unwrap() in futex.rs
    unwrap_count=$(grep -c "\.unwrap()" "$WORKSPACE_DIR/src/proc/futex.rs" 2>/dev/null || echo "0")
    if [ "$unwrap_count" -eq 0 ]; then
        pass "No unwrap() calls in futex.rs"
    else
        warn "Found $unwrap_count unwrap() calls in futex.rs"
    fi
else
    fail "Futex implementation missing"
fi

# Check pipe implementation
if [ -f "$WORKSPACE_DIR/src/fs/pipe.rs" ]; then
    pass "Pipe implementation exists"
else
    warn "Pipe implementation missing"
fi

echo ""

# =============================================================================
# 5. Error Handling Audit
# =============================================================================
echo "--- Error Handling Audit ---"

# Find all unwrap() calls in src/
total_unwrap=$(find "$WORKSPACE_DIR/src" -name "*.rs" -exec grep -l "\.unwrap()" {} \; 2>/dev/null | wc -l)
total_expect=$(find "$WORKSPACE_DIR/src" -name "*.rs" -exec grep -l "\.expect(" {} \; 2>/dev/null | wc -l)

echo "Files with unwrap(): $total_unwrap"
echo "Files with expect(): $total_expect"

if [ "$total_unwrap" -lt 20 ]; then
    pass "Unwrap usage is limited ($total_unwrap files)"
else
    warn "High unwrap usage ($total_unwrap files) - consider refactoring"
fi

if [ "$total_expect" -lt 20 ]; then
    pass "Expect usage is limited ($total_expect files)"
else
    warn "High expect usage ($total_expect files) - consider refactoring"
fi

# List files with most unwrap() calls
echo ""
echo "Top files by unwrap() count:"
find "$WORKSPACE_DIR/src" -name "*.rs" -exec grep -H "\.unwrap()" {} \; 2>/dev/null | \
    cut -d: -f1 | sort | uniq -c | sort -rn | head -5

echo ""

# =============================================================================
# 6. Documentation Verification
# =============================================================================
echo "--- Documentation ---"

doc_files=(
    "docs/status.md"
    "docs/architecture.md"
    "docs/milestones.md"
    "docs/syscalls.md"
    "docs/production_requirements.md"
)

for doc in "${doc_files[@]}"; do
    if [ -f "$WORKSPACE_DIR/$doc" ]; then
        pass "Documentation exists: $doc"
    else
        warn "Documentation missing: $doc"
    fi
done

echo ""

# =============================================================================
# Summary
# =============================================================================
echo "=== Verification Summary ==="
echo -e "${GREEN}Passed:${NC} $pass_count"
echo -e "${RED}Failed:${NC} $fail_count"
echo -e "${YELLOW}Warnings:${NC} $warn_count"
echo ""

if [ $fail_count -eq 0 ]; then
    echo -e "${GREEN}All critical checks passed!${NC}"
    exit 0
else
    echo -e "${RED}Some critical checks failed. Please review and fix.${NC}"
    exit 1
fi
