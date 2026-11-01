"""Auto-discovered strict smoke parser fixtures (stdlib only)."""

import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("parse_smoke_log", Path(__file__).resolve().parents[1] / "parse_smoke_log.py")
smoke = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(smoke)


def bench():
    return {
        "schema_version": 2, "benchmark": "getpid", "benchmark_version": 2,
        "architecture": "x86_64",
        "environment": dict(zip(smoke.ENV_KEYS, (
            "RustOS", "QEMU guest", "release", "CLOCK_MONOTONIC", "raw_linux_compatible",
            "linux", "x86_64", "rustc nightly", "QEMU emulator version test",
        ))),
        "warmup_iterations": 1000, "measured_iterations": 8192, "successful_calls": 8192,
        "operation_counts": {"getpid": 8192, "clock_gettime": 512},
        "elapsed_ns": 819200, "mean_ns_per_call": 100, "sample_count": 256,
        "sample_batch_size": 32,
        "latency_ns_per_call": {"min": 50, "p50": 100, "p90": 120, "p99": 140, "max": 200},
        "pid_checksum": 27576,
    }


def fixture(value=None):
    records = []
    for event in smoke.ORDERED:
        if event == "benchmark_json":
            records.append(json.dumps(bench() if value is None else value, separators=(",", ":")))
        elif event in ("smoke_child_reaped", "bench_child_reaped"):
            pid = 3 if event == "smoke_child_reaped" else 4
            records.append(f"RUSTOS_EXEC marker={event} pid={pid} exit=0")
        else:
            records.append(event)
    return "\n".join(records) + "\n"


def chain(scenario):
    return (
        f"RUSTOS_FAULT marker=fired scenario={scenario}\n"
        f"RUSTOS_FAULT marker=recovered scenario={scenario}\n"
        f"RUSTOS_FAULT marker=post_recovery_pass scenario={scenario} workload=retry_roundtrip exit=0\n"
    )


class ParseSmokeLogTests(unittest.TestCase):
    def validate(self, text, **kwargs):
        return smoke.validate(io.StringIO(text), **kwargs)

    def reject(self, text, **kwargs):
        with self.assertRaises(smoke.InvalidLog):
            self.validate(text, **kwargs)

    def test_valid_log(self):
        self.assertEqual(self.validate(fixture())["schema_version"], 2)

    def test_uart_line_endings_and_final_unterminated_line(self):
        for separator in ("\n", "\r\n", "\n\r", "\r\n\r"):
            with self.subTest(separator=repr(separator)):
                self.validate(separator.join(fixture().splitlines()))

    def test_embedded_cr_is_not_repaired(self):
        self.reject(fixture().replace("marker=fs_roundtrip_pass", "marker=fs_round\rtrip_pass"))

    def test_every_milestone_required(self):
        for record in fixture().splitlines():
            with self.subTest(record=record[:70]):
                self.reject(fixture().replace(record + "\n", "", 1))

    def test_prefixed_quoted_suffixed_or_indented_markers_do_not_count(self):
        for record in fixture().splitlines():
            for replacement in ("prefix " + record, '"' + record + '"', " " + record, record + " suffix"):
                with self.subTest(record=record[:40], replacement=replacement[:50]):
                    self.reject(fixture().replace(record, replacement, 1))

    def test_duplicate_records_rejected(self):
        for record in fixture().splitlines():
            with self.subTest(record=record[:50]):
                self.reject(fixture().replace(record + "\n", record + "\n" + record + "\n", 1))

    def test_reordered_records_rejected(self):
        records = fixture().splitlines()
        for index in range(len(records) - 1):
            bad = records.copy()
            bad[index:index + 2] = reversed(bad[index:index + 2])
            with self.subTest(index=index):
                self.reject("\n".join(bad))

    def test_failures_before_and_after_success(self):
        for failure in ("Kernel panic", "K E R N E L P A N I C", "SMOKE FAIL: file", "[init] TEST FAIL: x",
                        "RUSTOS_EXEC marker=anything_failed", "RUSTOS_BENCH marker=clock_failed phase=end",
                        "RUSTOS_SMOKE marker=fs_roundtrip_failed", "USERSPACE_BOOT_UNSUPPORTED"):
            with self.subTest(failure=failure):
                self.reject(failure + "\n" + fixture())
                self.reject(fixture() + failure)

    def test_malformed_extra_success_record_rejected(self):
        self.reject(fixture() + "RUSTOS_EXEC marker=smoke_started trailing\n")
        self.reject(fixture() + "FULL_OS_USERSPACE_OK trailing\n")

    def test_empty_and_minimal_logs(self):
        for text in ("", "BOOT_MINIMAL_OK\n", fixture() + "RustOS: BOOT_MINIMAL_OK\n"):
            self.reject(text)

    def test_bad_child_pid_and_exit(self):
        for pid in ("0", "-1", "+1", "1x", "18446744073709551616", "9" * 5000):
            self.reject(fixture().replace("pid=3 exit=0", f"pid={pid} exit=0"))
        for code in ("1", "-1", "00", "0 trailing"):
            self.reject(fixture().replace("pid=3 exit=0", f"pid=3 exit={code}"))
        self.reject(fixture().replace("pid=4 exit=0", "pid=3 exit=0"))

    def test_json_required_top_level_fields_and_extras(self):
        for key in smoke.KEYS:
            value = bench()
            del value[key]
            with self.subTest(key=key):
                self.reject(fixture(value))
        value = bench()
        value["unexpected"] = 1
        self.reject(fixture(value))

    def test_json_duplicate_keys_and_nonfinite(self):
        self.reject(fixture().replace('"schema_version":2', '"schema_version":2,"schema_version":2'))
        for value in ("NaN", "Infinity", "-Infinity"):
            self.reject(fixture().replace('"elapsed_ns":819200', '"elapsed_ns":' + value))

    def test_json_malformed(self):
        self.reject(fixture().replace('"benchmark":"getpid"', '"benchmark":getpid'))

    def test_numeric_fields_reject_wrong_types_ranges_and_counts(self):
        for key in ("schema_version", "benchmark_version", "warmup_iterations", "measured_iterations",
                    "successful_calls", "elapsed_ns", "mean_ns_per_call", "sample_count", "sample_batch_size", "pid_checksum"):
            for bad in (True, None, -1, 1.5, "2", 1 << 64):
                value = bench()
                value[key] = bad
                with self.subTest(key=key, bad=bad):
                    self.reject(fixture(value))
        for key in ("warmup_iterations", "measured_iterations", "successful_calls", "sample_count", "sample_batch_size"):
            value = bench()
            value[key] += 1
            self.reject(fixture(value))

    def test_percentile_monotonicity_types_and_missing_fields(self):
        for key in smoke.PERCENTILES:
            value = bench()
            del value["latency_ns_per_call"][key]
            self.reject(fixture(value))
            for bad in (-1, True, "1", 1.2):
                value = bench()
                value["latency_ns_per_call"][key] = bad
                self.reject(fixture(value))
        for left, right in zip(smoke.PERCENTILES, smoke.PERCENTILES[1:]):
            value = bench()
            value["latency_ns_per_call"][left] = value["latency_ns_per_call"][right] + 1
            self.reject(fixture(value))

    def test_equal_percentiles_and_zero_resolution_valid(self):
        value = bench()
        value["latency_ns_per_call"] = dict.fromkeys(smoke.PERCENTILES, 0)
        value["elapsed_ns"] = value["mean_ns_per_call"] = 0
        self.validate(fixture(value))

    def test_operation_counts(self):
        for name in ("getpid", "clock_gettime"):
            value = bench()
            value["operation_counts"][name] += 1
            self.reject(fixture(value))

    def test_mean_consistency(self):
        value = bench()
        value["mean_ns_per_call"] = 99
        self.reject(fixture(value))
        value["elapsed_ns"] = 8192 * 201
        value["mean_ns_per_call"] = 201
        self.reject(fixture(value))

    def test_environment_required(self):
        for name in smoke.ENV_KEYS:
            for missing in (True, False):
                value = bench()
                if missing:
                    del value["environment"][name]
                else:
                    value["environment"][name] = "  "
                self.reject(fixture(value))

    def test_metadata_cross_check(self):
        good = {"result": "passed", "architecture": "x86_64", "contract": "full_userspace_exec"}
        self.validate(fixture(), metadata=good)
        for name, bad in (("result", "failed"), ("architecture", "aarch64"), ("contract", "minimal_boot")):
            value = dict(good, **{name: bad})
            self.reject(fixture(), metadata=value)
            value = good.copy()
            del value[name]
            self.reject(fixture(), metadata=value)

    def test_explicit_architecture(self):
        self.validate(fixture(), arch="x86_64")
        self.reject(fixture(), arch="aarch64")

    def test_all_catalogued_fault_chains(self):
        self.validate(fixture() + "".join(map(chain, smoke.SCENARIOS)), require_fault_scenarios=smoke.SCENARIOS)

    def test_fault_chains_may_interleave(self):
        a, b = smoke.SCENARIOS[:2]
        first, second = chain(a).splitlines(), chain(b).splitlines()
        text = "\n".join(item for pair in zip(first, second) for item in pair)
        self.validate(fixture() + text, require_fault_scenarios=(a, b))

    def test_fault_chain_missing_or_out_of_order(self):
        scenario = smoke.SCENARIOS[0]
        records = chain(scenario).splitlines()
        for index in range(3):
            self.reject(fixture() + "\n".join(records[:index] + records[index + 1:]), require_fault_scenarios=(scenario,))
        self.reject(fixture() + "\n".join(reversed(records)), require_fault_scenarios=(scenario,))
        self.reject(fixture() + chain(scenario) + chain(scenario), require_fault_scenarios=(scenario,))

    def test_fault_host_scope_wrong_scenario_and_nonzero_exit(self):
        scenario = smoke.SCENARIOS[0]
        for text in (chain(scenario).replace("exit=0", "exit=1"),
                     chain(scenario).replace("scenario=" + scenario, "scenario=unknown"),
                     chain(scenario).replace("scenario=" + scenario, "scenario=" + scenario + " scope=host_engine"),
                     chain(scenario).replace(" workload=retry_roundtrip exit=0", "")):
            self.reject(fixture() + text, require_fault_scenarios=(scenario,))
        self.reject(fixture() + chain(smoke.SCENARIOS[1]), require_fault_scenarios=(scenario,))

    def test_scenario_csv_validation(self):
        self.assertEqual(smoke.parse_scenarios(None), ())
        self.assertEqual(smoke.parse_scenarios(",".join(smoke.SCENARIOS)), smoke.SCENARIOS)
        for csv in ("", "unknown", "pmm_alloc_oom_once,", "pmm_alloc_oom_once,pmm_alloc_oom_once"):
            with self.assertRaises(smoke.InvalidLog):
                smoke.parse_scenarios(csv)

    def test_cli_success_and_no_success_on_late_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "serial.log"
            meta = Path(directory) / "metadata.json"
            meta.write_text(json.dumps({"result": "passed", "architecture": "x86_64", "contract": "full_userspace_exec"}))
            for text, expected in ((fixture(), 0), (fixture() + "Kernel panic\n", 1)):
                path.write_bytes(text.replace("\n", "\n\r").encode())
                output, errors = io.StringIO(), io.StringIO()
                with contextlib.redirect_stdout(output), contextlib.redirect_stderr(errors):
                    status = smoke.main([str(path), "--metadata", str(meta), "--arch", "x86_64"])
                self.assertEqual(status, expected)
                self.assertEqual("SMOKE_LOG OK" in output.getvalue(), expected == 0)
                self.assertEqual("SMOKE_LOG ERROR" in errors.getvalue(), expected != 0)

    def test_cli_bad_metadata_and_missing_file(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "missing"
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(smoke.main([str(path)]), 1)
            path.write_text(fixture())
            meta = Path(directory) / "meta"
            for text in ("not json", "null", "[]", '{"result":"passed","result":"passed"}'):
                meta.write_text(text)
                with contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(smoke.main([str(path), "--metadata", str(meta)]), 1)


if __name__ == "__main__":
    unittest.main()
