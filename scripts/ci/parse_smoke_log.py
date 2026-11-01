#!/usr/bin/env python3
"""Strict single-boot full-kernel smoke evidence validation (stdlib only)."""

import argparse
import json
import re
import sys
from pathlib import Path

ARCHES = ("x86_64", "aarch64", "riscv64")
SCENARIOS = (
    "pmm_alloc_oom_once", "pmm_contiguous_alloc_once", "vmm_map_once",
    "vmm_reserve_once", "syscall_emfile_once", "syscall_eagain_once",
)
KEYS = (
    "schema_version", "benchmark", "benchmark_version", "architecture", "environment",
    "warmup_iterations", "measured_iterations", "successful_calls", "operation_counts",
    "elapsed_ns", "mean_ns_per_call", "sample_count", "sample_batch_size",
    "latency_ns_per_call", "pid_checksum",
)
ENV_KEYS = (
    "guest", "execution_context", "build_profile", "clock", "syscall_abi",
    "host_os", "host_arch", "compiler_version", "qemu_version",
)
PERCENTILES = ("min", "p50", "p90", "p99", "max")
ORDERED = (
    "BOOT_INIT_EXEC", "USERSPACE_SMOKE_START", "RUSTOS_EXEC marker=smoke_started",
    "RUSTOS_SMOKE marker=fs_roundtrip_pass", "RUSTOS_SMOKE marker=ipc_roundtrip_pass",
    "USERSPACE_SMOKE_PASS", "RUSTOS_EXEC marker=smoke_tests_passed",
    "smoke_child_reaped", "benchmark_json", "RUSTOS_BENCH marker=complete",
    "bench_child_reaped", "RUSTOS_EXEC marker=init_complete exit=0", "FULL_OS_USERSPACE_OK",
)
FAILURE = re.compile(
    r"kernel\s+panic|K E R N E L\s+P A N I C|\bpanic(?:ked)?\b"
    r"|SMOKE FAIL:|TEST FAIL:|\b[A-Za-z0-9_]+_failed\b"
    r"|\bUSERSPACE_BOOT_UNSUPPORTED\b", re.IGNORECASE,
)
CHILD = re.compile(r"RUSTOS_EXEC marker=(smoke_child_reaped|bench_child_reaped) pid=([1-9][0-9]*) exit=0")
FAULT = re.compile(
    r"RUSTOS_FAULT marker=(fired|recovered|post_recovery_pass) scenario=([a-z0-9_]+)"
    r"(?: workload=([A-Za-z0-9_.-]+) exit=0)?"
)
U64_MAX = (1 << 64) - 1


class InvalidLog(Exception):
    """The log cannot substantiate the requested execution contract."""


def require(ok, message):
    if not ok:
        raise InvalidLog(message)


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, f"duplicate JSON key: {key}")
        value[key] = item
    return value


def decode_json(text):
    def invalid_constant(value):
        raise InvalidLog(f"non-finite JSON constant: {value}")
    try:
        return json.loads(text, object_pairs_hook=unique_object, parse_constant=invalid_constant)
    except (ValueError, RecursionError) as error:
        raise InvalidLog(f"invalid JSON: {error}") from error


def keys(value, expected, context):
    require(isinstance(value, dict), f"{context} must be an object")
    require(set(value) == set(expected), f"{context} keys must be: {', '.join(expected)}")


def uint(value, context):
    require(type(value) is int and 0 <= value <= U64_MAX, f"{context} must be a u64 integer")
    return value


def validate_benchmark(value, arch=None):
    keys(value, KEYS, "benchmark")
    for name in ("schema_version", "benchmark_version", "warmup_iterations", "measured_iterations",
                 "successful_calls", "elapsed_ns", "mean_ns_per_call", "sample_count",
                 "sample_batch_size", "pid_checksum"):
        uint(value[name], name)
    require(value["schema_version"] == 2, "schema_version must be 2")
    require(value["benchmark"] == "getpid" and value["benchmark_version"] == 2,
            "expected getpid benchmark_version 2")
    require(value["architecture"] in ARCHES, "unsupported benchmark architecture")
    if arch is not None:
        require(value["architecture"] == arch, "benchmark architecture mismatch")
    for name, expected in (("warmup_iterations", 1000), ("measured_iterations", 8192),
                           ("successful_calls", 8192), ("sample_count", 256), ("sample_batch_size", 32)):
        require(value[name] == expected, f"{name} must be {expected}")
    keys(value["operation_counts"], ("getpid", "clock_gettime"), "operation_counts")
    for name, expected in (("getpid", 8192), ("clock_gettime", 512)):
        require(uint(value["operation_counts"][name], name) == expected, f"wrong {name} operation count")
    require(value["mean_ns_per_call"] == value["elapsed_ns"] // 8192,
            "mean_ns_per_call must be elapsed_ns // measured_iterations")
    keys(value["latency_ns_per_call"], PERCENTILES, "latency_ns_per_call")
    latency = [uint(value["latency_ns_per_call"][name], name) for name in PERCENTILES]
    require(latency == sorted(latency), "latency requires min <= p50 <= p90 <= p99 <= max")
    require(latency[0] <= value["mean_ns_per_call"] <= latency[-1],
            "mean outside batch-average sample bounds")
    require(value["pid_checksum"] > 0, "pid_checksum must be positive")
    keys(value["environment"], ENV_KEYS, "environment")
    for name in ENV_KEYS:
        item = value["environment"][name]
        require(isinstance(item, str) and bool(item.strip()), f"environment.{name} must be nonempty")
    for name, expected in (("guest", "RustOS"), ("execution_context", "QEMU guest"),
                           ("build_profile", "release"), ("clock", "CLOCK_MONOTONIC"),
                           ("syscall_abi", "raw_linux_compatible")):
        require(value["environment"][name] == expected, f"wrong environment.{name}")
    return value


def parse_scenarios(csv):
    if csv is None:
        return ()
    values = tuple(part.strip() for part in csv.split(","))
    require(all(values), "fault scenario CSV contains an empty entry")
    require(len(set(values)) == len(values), "duplicate requested fault scenario")
    require(set(values) <= set(SCENARIOS), "unknown requested fault scenario")
    return values


def validate(lines, metadata=None, require_fault_scenarios=(), arch=None):
    requested = tuple(require_fault_scenarios)
    require(len(set(requested)) == len(requested) and set(requested) <= set(SCENARIOS),
            "invalid requested fault scenarios")
    seen, faults, child_pids = {}, {}, {}
    benchmark = None
    for number, raw in enumerate(lines, 1):
        # UART boundary CRs are tolerated; embedded CRs cannot repair a record.
        line = raw.rstrip("\n").strip("\r")
        require(not FAILURE.search(line), f"line {number}: failure diagnostic: {line}")
        require(line not in ("BOOT_MINIMAL_OK", "RustOS: BOOT_MINIMAL_OK"),
                f"line {number}: minimal boot is not full-kernel evidence")
        event = line if line in ORDERED else None
        match = CHILD.fullmatch(line)
        if match:
            event, pid = match.groups()
            require(len(pid) <= 20 and int(pid) <= U64_MAX, f"line {number}: PID outside u64")
            child_pids[event] = int(pid)
        elif line.startswith(("RUSTOS_EXEC marker=smoke_child_reaped", "RUSTOS_EXEC marker=bench_child_reaped")):
            raise InvalidLog(f"line {number}: malformed/nonzero child reap")
        if line.startswith("{"):
            value = decode_json(line)
            if isinstance(value, dict) and ("schema_version" in value or "benchmark" in value):
                benchmark = validate_benchmark(value, arch)
                event = "benchmark_json"
        if event is None and any(line.startswith(item) for item in ORDERED
                                 if item not in ("benchmark_json", "smoke_child_reaped", "bench_child_reaped")):
            raise InvalidLog(f"line {number}: malformed full-kernel milestone")
        if event is not None:
            require(event not in seen, f"line {number}: duplicate {event}")
            require(len(seen) < len(ORDERED) and event == ORDERED[len(seen)],
                    f"line {number}: out-of-order {event}")
            seen[event] = number
        elif line.startswith(("RUSTOS_SMOKE ", "RUSTOS_BENCH ")):
            raise InvalidLog(f"line {number}: malformed/unknown smoke or benchmark marker")
        if line.startswith("RUSTOS_FAULT"):
            match = FAULT.fullmatch(line)
            require(match is not None, f"line {number}: malformed guest fault record (host scope is not evidence)")
            marker, scenario, workload = match.groups()
            require(scenario in SCENARIOS, f"line {number}: unknown fault scenario")
            require((marker == "post_recovery_pass") == (workload is not None),
                    f"line {number}: workload/exit fields only belong to post_recovery_pass")
            state = faults.get(scenario, 0)
            require(state < 3 and marker == ("fired", "recovered", "post_recovery_pass")[state],
                    f"line {number}: duplicate/out-of-order fault record for {scenario}")
            faults[scenario] = state + 1
    require(len(seen) == len(ORDERED), f"missing milestones: {', '.join(ORDERED[len(seen):])}")
    require(child_pids["smoke_child_reaped"] != child_pids["bench_child_reaped"],
            "smoke and benchmark must reap distinct children")
    for scenario in requested:
        require(faults.get(scenario) == 3, f"missing fault recovery chain: {scenario}")
    if metadata is not None:
        require(isinstance(metadata, dict), "metadata must be a JSON object")
        require(metadata.get("result") == "passed", "metadata result must be passed")
        require(metadata.get("architecture") == benchmark["architecture"], "metadata architecture mismatch")
        require(metadata.get("contract") == "full_userspace_exec", "metadata contract mismatch")
    return benchmark


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--arch", choices=ARCHES)
    parser.add_argument("--require-fault-scenarios", metavar="CSV")
    args = parser.parse_args(argv)
    try:
        requested = parse_scenarios(args.require_fault_scenarios)
        metadata = decode_json(args.metadata.read_text(encoding="utf-8")) if args.metadata else None
        if args.metadata is not None:
            require(isinstance(metadata, dict), "metadata must be a JSON object")
        with args.log.open(encoding="utf-8", errors="replace", newline="\n") as stream:
            benchmark = validate(stream, metadata, requested, args.arch)
    except (OSError, UnicodeError, InvalidLog) as error:
        print(f"SMOKE_LOG ERROR: {error}", file=sys.stderr)
        return 1
    print(f"SMOKE_LOG OK: full_userspace_exec architecture={benchmark['architecture']} schema_version=2")
    return 0


if __name__ == "__main__":
    sys.exit(main())
