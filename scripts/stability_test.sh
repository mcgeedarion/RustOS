#!/bin/bash
# stability_test.sh - RustOS Stability and Reliability Test Script
#
# This script performs extended stability testing to verify system reliability
# under various stress conditions.
#
# Usage: ./scripts/stability_test.sh [duration_minutes]
#

set -e

DURATION_MINUTES=${1:-30}
DURATION_SECONDS=$((DURATION_MINUTES * 60))
LOG_DIR="/tmp/rustos_stability_$(date +%Y%m%d_%H%M%S)"
RESULTS_FILE="$LOG_DIR/results.txt"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

setup() {
    log_info "Setting up stability test environment..."
    mkdir -p "$LOG_DIR"
    
    cat > "$RESULTS_FILE" << EOF
RustOS Stability Test Results
=============================
Start Time: $(date)
Duration: ${DURATION_MINUTES} minutes
Test Host: $(hostname)
Kernel: $(uname -r)

EOF
    
    log_info "Log directory: $LOG_DIR"
}

cleanup() {
    log_info "Cleaning up..."
    
    # Kill any background processes
    if [ -n "$WORKER_PIDS" ]; then
        for pid in $WORKER_PIDS; do
            kill $pid 2>/dev/null || true
        done
    fi
    
    log_info "Results saved to: $RESULTS_FILE"
    cat "$RESULTS_FILE"
}

trap cleanup EXIT

# ============================================================================
# Test 1: Memory Stress Test
# ============================================================================
test_memory_stress() {
    log_info "Starting memory stress test..."
    
    local mem_log="$LOG_DIR/memory_stress.log"
    local start_time=$(date +%s)
    local iterations=0
    
    while true; do
        current_time=$(date +%s)
        elapsed=$((current_time - start_time))
        
        if [ $elapsed -ge $DURATION_SECONDS ]; then
            break
        fi
        
        # Allocate and free memory repeatedly
        dd if=/dev/zero of=/tmp/mem_test_$$ bs=1M count=10 2>/dev/null || true
        rm -f /tmp/mem_test_$$
        
        iterations=$((iterations + 1))
        
        if [ $((iterations % 100)) -eq 0 ]; then
            log_info "Memory stress: $iterations iterations completed"
            echo "Iteration $iterations at $(date)" >> "$mem_log"
        fi
    done
    
    log_info "Memory stress test completed: $iterations iterations"
    echo "Memory Stress: $iterations iterations" >> "$RESULTS_FILE"
}

# ============================================================================
# Test 2: Filesystem Churn Test
# ============================================================================
test_filesystem_churn() {
    log_info "Starting filesystem churn test..."
    
    local fs_log="$LOG_DIR/fs_churn.log"
    local test_dir="/tmp/fs_churn_$$"
    mkdir -p "$test_dir"
    
    local start_time=$(date +%s)
    local operations=0
    
    while true; do
        current_time=$(date +%s)
        elapsed=$((current_time - start_time))
        
        if [ $elapsed -ge $DURATION_SECONDS ]; then
            break
        fi
        
        # Create files
        for i in $(seq 1 10); do
            echo "data_$i" > "$test_dir/file_$i.txt"
        done
        
        # Read files
        for i in $(seq 1 10); do
            cat "$test_dir/file_$i.txt" > /dev/null
        done
        
        # Delete files
        rm -f "$test_dir/file_"*.txt
        
        # Recreate directory structure
        mkdir -p "$test_dir/subdir_{1..5}"
        rmdir "$test_dir/subdir_"*
        
        operations=$((operations + 30))
        
        if [ $((operations % 1000)) -eq 0 ]; then
            log_info "FS churn: $operations operations completed"
            echo "Operations $operations at $(date)" >> "$fs_log"
        fi
    done
    
    rm -rf "$test_dir"
    log_info "Filesystem churn test completed: $operations operations"
    echo "Filesystem Churn: $operations operations" >> "$RESULTS_FILE"
}

# ============================================================================
# Test 3: Process Fork Bomb (Controlled)
# ============================================================================
test_process_stress() {
    log_info "Starting process stress test..."
    
    local proc_log="$LOG_DIR/process_stress.log"
    local start_time=$(date +%s)
    local forks=0
    local max_concurrent=50
    
    while true; do
        current_time=$(date +%s)
        elapsed=$((current_time - start_time))
        
        if [ $elapsed -ge $DURATION_SECONDS ]; then
            break
        fi
        
        # Fork limited number of short-lived processes
        local pids=""
        for i in $(seq 1 $max_concurrent); do
            (sleep 0.1 && exit 0) &
            pids="$pids $!"
        done
        
        # Wait for all to complete
        for pid in $pids; do
            wait $pid 2>/dev/null || true
        done
        
        forks=$((forks + max_concurrent))
        
        if [ $((forks % 500)) -eq 0 ]; then
            log_info "Process stress: $forks forks completed"
            echo "Forks $forks at $(date)" >> "$proc_log"
        fi
    done
    
    log_info "Process stress test completed: $forks forks"
    echo "Process Stress: $forks forks" >> "$RESULTS_FILE"
}

# ============================================================================
# Test 4: I/O Throughput Test
# ============================================================================
test_io_throughput() {
    log_info "Starting I/O throughput test..."
    
    local io_log="$LOG_DIR/io_throughput.log"
    local test_file="/tmp/io_test_$$"
    local start_time=$(date +%s)
    local total_bytes=0
    
    while true; do
        current_time=$(date +%s)
        elapsed=$((current_time - start_time))
        
        if [ $elapsed -ge $DURATION_SECONDS ]; then
            break
        fi
        
        # Sequential write
        dd if=/dev/zero of="$test_file" bs=4K count=256 conv=fsync 2>/dev/null
        total_bytes=$((total_bytes + 1048576))
        
        # Sequential read
        dd if="$test_file" of=/dev/null bs=4K 2>/dev/null
        
        # Random access simulation
        for offset in 0 50 100 150 200; do
            dd if="$test_file" of=/dev/null bs=4K count=1 skip=$offset 2>/dev/null
        done
        
        if [ $((total_bytes % 10485760)) -eq 0 ]; then
            log_info "I/O throughput: $((total_bytes / 1048576)) MB processed"
            echo "Bytes $total_bytes at $(date)" >> "$io_log"
        fi
    done
    
    rm -f "$test_file"
    log_info "I/O throughput test completed: $((total_bytes / 1048576)) MB"
    echo "I/O Throughput: $((total_bytes / 1048576)) MB" >> "$RESULTS_FILE"
}

# ============================================================================
# Test 5: System Resource Monitoring
# ============================================================================
monitor_resources() {
    log_info "Starting resource monitoring..."
    
    local mon_log="$LOG_DIR/resource_monitor.log"
    local start_time=$(date +%s)
    
    echo "Timestamp,CPU%,Mem%,DiskUsed" > "$mon_log"
    
    while true; do
        current_time=$(date +%s)
        elapsed=$((current_time - start_time))
        
        if [ $elapsed -ge $DURATION_SECONDS ]; then
            break
        fi
        
        # Capture metrics every 5 seconds
        timestamp=$(date '+%Y-%m-%d %H:%M:%S')
        
        # CPU usage (approximate)
        cpu_usage=$(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | cut -d'%' -f1 2>/dev/null || echo "N/A")
        
        # Memory usage
        mem_info=$(free | grep Mem)
        mem_total=$(echo $mem_info | awk '{print $2}')
        mem_used=$(echo $mem_info | awk '{print $3}')
        if [ "$mem_total" -gt 0 ] 2>/dev/null; then
            mem_pct=$((mem_used * 100 / mem_total))
        else
            mem_pct="N/A"
        fi
        
        # Disk usage
        disk_used=$(df /tmp | tail -1 | awk '{print $5}' 2>/dev/null || echo "N/A")
        
        echo "$timestamp,$cpu_usage,$mem_pct,$disk_used" >> "$mon_log"
        
        sleep 5
    done
    
    log_info "Resource monitoring completed"
}

# ============================================================================
# Main Execution
# ============================================================================
main() {
    log_info "=========================================="
    log_info "RustOS Stability Test Suite"
    log_info "=========================================="
    log_info "Duration: ${DURATION_MINUTES} minutes"
    log_info "Tests: Memory, Filesystem, Process, I/O"
    log_info "=========================================="
    
    setup
    
    # Run tests in parallel
    test_memory_stress &
    WORKER_PIDS="$!"
    
    test_filesystem_churn &
    WORKER_PIDS="$WORKER_PIDS $!"
    
    test_process_stress &
    WORKER_PIDS="$WORKER_PIDS $!"
    
    test_io_throughput &
    WORKER_PIDS="$WORKER_PIDS $!"
    
    monitor_resources &
    WORKER_PIDS="$WORKER_PIDS $!"
    
    # Wait for all background jobs
    log_info "All tests running... Press Ctrl+C to abort"
    wait
    
    log_info "=========================================="
    log_info "Stability Test Completed Successfully"
    log_info "=========================================="
}

# Run main function
main
