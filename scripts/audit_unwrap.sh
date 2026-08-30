#!/bin/bash
# Error Handling Audit Script
# Scans the codebase for unwrap()/expect() calls and generates a report

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
SRC_DIR="$ROOT_DIR/src"

echo "=== RustOS Error Handling Audit ==="
echo "Scanning for unwrap() and expect() calls in security-critical code..."
echo ""

# Files to prioritize for audit (security-critical paths)
PRIORITY_FILES=(
    "ipc/msg.rs"
    "ipc/pipe_scheme.rs"
    "fs/jbd2.rs"
    "fs/ext4.rs"
    "arch/x86_64/gdt.rs"
    "arch/x86_64/idt.rs"
    "arch/x86_64/paging.rs"
    "mm/*.rs"
    "security/seccomp.rs"
    "security/pti.rs"
    "firmware/acpi/*.rs"
    "smp/*.rs"
)

# Count total unwrap/expect calls
TOTAL_UNWRAP=$(grep -rn "unwrap()" "$SRC_DIR" --include="*.rs" 2>/dev/null | wc -l || echo "0")
TOTAL_EXPECT=$(grep -rn "expect(" "$SRC_DIR" --include="*.rs" 2>/dev/null | grep -v "expect_err" | wc -l || echo "0")

echo "Summary:"
echo "  Total unwrap() calls: $TOTAL_UNWRAP"
echo "  Total expect() calls: $TOTAL_EXPECT"
echo ""

# Generate detailed report
REPORT_FILE="$ROOT_DIR/docs/error_handling_audit.md"
cat > "$REPORT_FILE" << EOF
# Error Handling Audit Report

Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")

## Summary

- **Total unwrap() calls**: $TOTAL_UNWRAP
- **Total expect() calls**: $TOTAL_EXPECT
- **Target for production**: 0 in security-critical paths

## Priority Files for Remediation

EOF

# Analyze priority files
for file_pattern in "${PRIORITY_FILES[@]}"; do
    echo "Analyzing: $file_pattern"
    matches=$(find "$SRC_DIR" -name "*.rs" -path "*/$file_pattern" -exec grep -Hn "unwrap()\|expect(" {} \; 2>/dev/null || true)
    if [ -n "$matches" ]; then
        echo "" >> "$REPORT_FILE"
        echo "### $file_pattern" >> "$REPORT_FILE"
        echo '```' >> "$REPORT_FILE"
        echo "$matches" >> "$REPORT_FILE"
        echo '```' >> "$REPORT_FILE"
    fi
done

# List all files with unwrap/expect sorted by count
echo "" >> "$REPORT_FILE"
echo "## All Files Ranked by unwrap()/expect() Count" >> "$REPORT_FILE"
echo "" >> "$REPORT_FILE"

grep -rn "unwrap()\|expect(" "$SRC_DIR" --include="*.rs" 2>/dev/null | \
    cut -d: -f1 | sort | uniq -c | sort -rn | head -30 | \
    while read count file; do
        rel_path="${file#$ROOT_DIR/}"
        echo "- \`$rel_path\`: $count occurrences" >> "$REPORT_FILE"
    done

cat >> "$REPORT_FILE" << EOF

## Remediation Guidelines

1. **Replace unwrap() with Result propagation**
   ```rust
   // Before
   let value = option.unwrap();
   
   // After
   let value = option.ok_or(Error::MissingValue)?;
   ```

2. **Replace expect() with context-aware errors**
   ```rust
   // Before
   let data = get_data().expect("failed to get data");
   
   // After
   let data = get_data().ok_or_else(|| Error::GetDataFailed {
       context: "initialization".to_string()
   })?;
   ```

3. **For test code only**, keep unwrap()/expect() but add #[cfg(test)]

## Next Steps

- [ ] Review each occurrence in priority files
- [ ] Replace with proper error handling
- [ ] Add unit tests for error paths
- [ ] Re-run audit script to verify progress
EOF

echo ""
echo "Report generated: $REPORT_FILE"
echo ""

# Exit with warning if high-priority files have issues
HIGH_PRIORITY_COUNT=0
for file_pattern in "security/*" "mm/*" "arch/x86_64/gdt.rs" "arch/x86_64/idt.rs"; do
    count=$(find "$SRC_DIR" -name "*.rs" -path "*/$file_pattern" -exec grep -c "unwrap()\|expect(" {} \; 2>/dev/null | awk '{s+=$1}END{print s}' || echo "0")
    HIGH_PRIORITY_COUNT=$((HIGH_PRIORITY_COUNT + ${count:-0}))
done

if [ "$HIGH_PRIORITY_COUNT" -gt 0 ]; then
    echo "WARNING: Found $HIGH_PRIORITY_COUNT unwrap()/expect() calls in high-priority security-critical files"
    exit 1
fi

echo "✓ No unwrap()/expect() calls found in high-priority security-critical files"
exit 0
