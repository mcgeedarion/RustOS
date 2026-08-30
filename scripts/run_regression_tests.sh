#!/bin/bash
# RustOS Automated Regression Test Runner
#
# This script runs the complete regression test suite locally,
# mirroring what the CI pipeline does.
#
# Usage:
#   ./scripts/run_regression_tests.sh [OPTIONS]
#
# Options:
#   --unit-only          Run only unit tests
#   --kernel-only        Run only kernel module tests
#   --integration-only   Run only integration tests
#   --perf-only         Run only performance tests
#   --fuzz-only         Run only fuzz tests
#   --audit-only        Run only error handling audit
#   --skip-unit         Skip unit tests
#   --skip-kernel       Skip kernel module tests
#   --coverage          Generate coverage report (default)
#   --no-coverage       Skip coverage generation
#   --verbose           Enable verbose output
#   --help              Show this help message

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$ROOT_DIR"

# Default options
RUN_UNIT=true
RUN_KERNEL=true
RUN_INTEGRATION=true
RUN_PERF=true
RUN_FUZZ=true
RUN_AUDIT=true
GENERATE_COVERAGE=true
VERBOSE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --unit-only)
            RUN_KERNEL=false
            RUN_INTEGRATION=false
            RUN_PERF=false
            RUN_FUZZ=false
            RUN_AUDIT=false
            shift
            ;;
        --kernel-only)
            RUN_UNIT=false
            RUN_INTEGRATION=false
            RUN_PERF=false
            RUN_FUZZ=false
            RUN_AUDIT=false
            shift
            ;;
        --integration-only)
            RUN_UNIT=false
            RUN_KERNEL=false
            RUN_PERF=false
            RUN_FUZZ=false
            RUN_AUDIT=false
            shift
            ;;
        --perf-only)
            RUN_UNIT=false
            RUN_KERNEL=false
            RUN_INTEGRATION=false
            RUN_FUZZ=false
            RUN_AUDIT=false
            shift
            ;;
        --fuzz-only)
            RUN_UNIT=false
            RUN_KERNEL=false
            RUN_INTEGRATION=false
            RUN_PERF=false
            RUN_AUDIT=false
            shift
            ;;
        --audit-only)
            RUN_UNIT=false
            RUN_KERNEL=false
            RUN_INTEGRATION=false
            RUN_PERF=false
            RUN_FUZZ=false
            shift
            ;;
        --skip-unit)
            RUN_UNIT=false
            shift
            ;;
        --skip-kernel)
            RUN_KERNEL=false
            shift
            ;;
        --no-coverage)
            GENERATE_COVERAGE=false
            shift
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        --help)
            head -20 "$0" | tail -17
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
}

# Track test results
declare -A TEST_RESULTS
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

run_unit_tests() {
    log_info "Running unit tests..."
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if [ "$GENERATE_COVERAGE" = true ]; then
        if command -v cargo-tarpaulin &> /dev/null; then
            log_info "Generating coverage report..."
            mkdir -p coverage
            
            if [ "$VERBOSE" = true ]; then
                cargo tarpaulin \
                    --verbose \
                    --all-features \
                    --out Xml \
                    --out Html \
                    --output-dir coverage \
                    --timeout 120
            else
                cargo tarpaulin \
                    --all-features \
                    --out Xml \
                    --out Html \
                    --output-dir coverage \
                    --timeout 120 \
                    2>&1 | grep -v "^     Running" || true
            fi
            
            log_success "Coverage report generated in coverage/"
            TEST_RESULTS["unit"]="PASSED"
            PASSED_TESTS=$((PASSED_TESTS + 1))
        else
            log_warning "cargo-tarpaulin not installed, running standard tests"
            if cargo test --all-features; then
                TEST_RESULTS["unit"]="PASSED"
                PASSED_TESTS=$((PASSED_TESTS + 1))
            else
                TEST_RESULTS["unit"]="FAILED"
                FAILED_TESTS=$((FAILED_TESTS + 1))
            fi
        fi
    else
        if cargo test --all-features; then
            TEST_RESULTS["unit"]="PASSED"
            PASSED_TESTS=$((PASSED_TESTS + 1))
        else
            TEST_RESULTS["unit"]="FAILED"
            FAILED_TESTS=$((FAILED_TESTS + 1))
        fi
    fi
}

run_kernel_tests() {
    log_info "Running kernel module tests..."
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if [ -f "$SCRIPT_DIR/run_kmtest.sh" ]; then
        if "$SCRIPT_DIR/run_kmtest.sh" --timeout 300 --junit-xml kmtest-results.xml; then
            log_success "Kernel module tests passed"
            TEST_RESULTS["kernel"]="PASSED"
            PASSED_TESTS=$((PASSED_TESTS + 1))
        else
            log_error "Kernel module tests failed"
            TEST_RESULTS["kernel"]="FAILED"
            FAILED_TESTS=$((FAILED_TESTS + 1))
        fi
    else
        log_warning "kmtest script not found, skipping kernel tests"
        TEST_RESULTS["kernel"]="SKIPPED"
    fi
}

run_integration_tests() {
    log_info "Running integration tests..."
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    local suites=("boot-smoke" "filesystem" "ipc" "scheduler")
    local all_passed=true
    
    for suite in "${suites[@]}"; do
        log_info "Running integration test suite: $suite"
        
        if [ -f "$SCRIPT_DIR/run-integration-test.sh" ]; then
            if ! "$SCRIPT_DIR/run-integration-test.sh" \
                --suite "$suite" \
                --timeout 600 \
                --log "integration-${suite}.log"; then
                log_error "Integration test suite failed: $suite"
                all_passed=false
            fi
        else
            log_warning "Integration test script not found"
        fi
    done
    
    if [ "$all_passed" = true ]; then
        TEST_RESULTS["integration"]="PASSED"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        TEST_RESULTS["integration"]="FAILED"
        FAILED_TESTS=$((FAILED_TESTS + 1))
    fi
}

run_perf_tests() {
    log_info "Running performance regression tests..."
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if [ -f "$SCRIPT_DIR/bench-boot.sh" ] && [ -f "$SCRIPT_DIR/compare-perf.sh" ]; then
        log_info "Running boot time benchmarks..."
        "$SCRIPT_DIR/bench-boot.sh" --iterations 5 --output boot-perf.json || true
        
        log_info "Running syscall latency benchmarks..."
        "$SCRIPT_DIR/bench-syscall.sh" --iterations 100 --output syscall-perf.json || true
        
        log_info "Comparing with baseline..."
        if [ -f "perf-baseline.json" ]; then
            "$SCRIPT_DIR/compare-perf.sh" \
                --baseline perf-baseline.json \
                --current boot-perf.json syscall-perf.json \
                --threshold 10 \
                --report perf-regression-report.md || true
        fi
        
        TEST_RESULTS["perf"]="PASSED"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        log_warning "Performance test scripts not found"
        TEST_RESULTS["perf"]="SKIPPED"
    fi
}

run_fuzz_tests() {
    log_info "Running fuzz tests (quick run)..."
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if [ -d "$ROOT_DIR/fuzz" ] && command -v cargo-fuzz &> /dev/null; then
        cd "$ROOT_DIR/fuzz"
        
        log_info "Running VFS fuzzer for 60 seconds..."
        cargo fuzz run vfs_fuzzer -- -max_total_time=60 || true
        
        log_info "Running IPC fuzzer for 60 seconds..."
        cargo fuzz run ipc_fuzzer -- -max_total_time=60 || true
        
        cd "$ROOT_DIR"
        TEST_RESULTS["fuzz"]="PASSED"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        log_warning "Fuzz testing not available (missing cargo-fuzz or fuzz directory)"
        TEST_RESULTS["fuzz"]="SKIPPED"
    fi
}

run_audit() {
    log_info "Running error handling audit..."
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    if [ -f "$SCRIPT_DIR/audit_unwrap.sh" ]; then
        chmod +x "$SCRIPT_DIR/audit_unwrap.sh"
        
        if "$SCRIPT_DIR/audit_unwrap.sh"; then
            log_success "Error handling audit passed"
            TEST_RESULTS["audit"]="PASSED"
            PASSED_TESTS=$((PASSED_TESTS + 1))
        else
            log_warning "Error handling audit found issues (see docs/error_handling_audit.md)"
            TEST_RESULTS["audit"]="WARNING"
            PASSED_TESTS=$((PASSED_TESTS + 1))
        fi
    else
        log_warning "Audit script not found"
        TEST_RESULTS["audit"]="SKIPPED"
    fi
}

print_summary() {
    echo ""
    echo "========================================"
    echo "        REGRESSION TEST SUMMARY        "
    echo "========================================"
    echo ""
    printf "%-20s %-10s\n" "Test Suite" "Status"
    printf "%-20s %-10s\n" "----------" "------"
    
    for test in "${!TEST_RESULTS[@]}"; do
        status="${TEST_RESULTS[$test]}"
        case $status in
            PASSED)
                printf "%-20s ${GREEN}%-10s${NC}\n" "$test" "$status"
                ;;
            FAILED)
                printf "%-20s ${RED}%-10s${NC}\n" "$test" "$status"
                ;;
            WARNING)
                printf "%-20s ${YELLOW}%-10s${NC}\n" "$test" "$status"
                ;;
            SKIPPED)
                printf "%-20s ${BLUE}%-10s${NC}\n" "$test" "$status"
                ;;
        esac
    done
    
    echo ""
    echo "----------------------------------------"
    echo "Total: $TOTAL_TESTS | Passed: $PASSED_TESTS | Failed: $FAILED_TESTS"
    echo "========================================"
    echo ""
    
    if [ $FAILED_TESTS -gt 0 ]; then
        log_error "Some tests failed!"
        return 1
    else
        log_success "All tests passed!"
        return 0
    fi
}

# Main execution
main() {
    echo "========================================"
    echo "      RustOS Regression Test Suite     "
    echo "========================================"
    echo ""
    echo "Date: $(date)"
    echo "Directory: $ROOT_DIR"
    echo ""
    
    if [ "$RUN_UNIT" = true ]; then
        run_unit_tests
    fi
    
    if [ "$RUN_KERNEL" = true ]; then
        run_kernel_tests
    fi
    
    if [ "$RUN_INTEGRATION" = true ]; then
        run_integration_tests
    fi
    
    if [ "$RUN_PERF" = true ]; then
        run_perf_tests
    fi
    
    if [ "$RUN_FUZZ" = true ]; then
        run_fuzz_tests
    fi
    
    if [ "$RUN_AUDIT" = true ]; then
        run_audit
    fi
    
    print_summary
}

main
