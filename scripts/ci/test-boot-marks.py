#!/usr/bin/env python3
"""Host fixtures exercise the actual shell entry point, not a parser mock."""

import subprocess
import tempfile
import unittest
from pathlib import Path


PARSER = Path(__file__).resolve().with_name("parse-boot-marks.sh")
ENTRY = "BOOT_MARK label=BOOT_ENTRY ticks=100\n"
MMU = "BOOT_MARK label=BOOT_MMU_ON ticks=140\n"
INITRAMFS = "BOOT_MARK label=BOOT_INITRAMFS_LOADED ticks=180\n"
EXEC = "BOOT_MARK label=BOOT_INIT_EXEC ticks=220\n"
OK = "RustOS: BOOT_MINIMAL_OK\n"
MINIMAL = ENTRY + MMU + OK
FULL = ENTRY + MMU + INITRAMFS + EXEC


class BootMarkTests(unittest.TestCase):
    def run_parser(self, log, mode=None, extra_args=()):
        with tempfile.TemporaryDirectory(prefix="boot-marks-") as directory:
            path = Path(directory) / "boot-marks.log"
            path.write_bytes(log.encode("utf-8"))
            command = ["bash", str(PARSER)]
            if mode is not None:
                command += ["--mode", mode]
            command += list(extra_args) + [str(path)]
            return subprocess.run(
                command, capture_output=True, text=True, check=False, timeout=10,
                cwd=directory,  # The shell wrapper must not depend on repo cwd.
            )

    def assert_valid(self, log, mode=None):
        result = self.run_parser(log, mode)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        self.assertIn("BOOT_PERF OK", result.stdout)
        return result.stdout

    def assert_invalid(self, log, mode=None, diagnostic=None):
        result = self.run_parser(log, mode)
        self.assertEqual(result.returncode, 1, result)
        self.assertEqual(result.stdout, "", "invalid logs must not print a success table")
        self.assertIn("BOOT_PERF ERROR:", result.stderr)
        if diagnostic:
            self.assertIn(diagnostic, result.stderr)

    def test_minimal_requires_only_real_timed_milestones_and_completion(self):
        output = self.assert_valid(MINIMAL, "minimal")
        self.assertRegex(output, r"BOOT_MMU_ON\s+140\s+40\n")
        self.assertRegex(output, r"TOTAL \(ENTRY → MMU_ON\)\s+40\n")
        self.assertNotIn("BOOT_INITRAMFS_LOADED", output)
        self.assertNotIn("INIT_EXEC", output)

    def test_full_default_and_explicit_contract(self):
        for mode in (None, "full"):
            with self.subTest(mode=mode):
                output = self.assert_valid(FULL, mode)
                self.assertRegex(output, r"BOOT_INIT_EXEC\s+220\s+40\n")
                self.assertRegex(output, r"TOTAL \(ENTRY → INIT_EXEC\)\s+120\n")

    def test_current_minimal_shape_uses_first_entry_and_ignores_placeholder(self):
        log = (
            ENTRY
            + "RustOS: boot target [PRIMARY] - entering common kernel_main\n"
            + "BOOT_MARK label=BOOT_ENTRY ticks=120\n"
            + "RustOS: boot-minimal entering common path\n"
            + "RustOS: initrd absent\n"
            + MMU + INITRAMFS + OK
        )
        for newline in ("\n", "\r\n", "\n\r"):
            with self.subTest(newline=repr(newline)):
                output = self.assert_valid(log.replace("\n", newline), "minimal")
                self.assertIn("using the first as baseline", output)
                self.assertRegex(output, r"BOOT_ENTRY\s+100\s+\(baseline\)")
                self.assertRegex(output, r"BOOT_MMU_ON\s+140\s+40\n")
                self.assertNotRegex(output, r"(?m)^BOOT_INITRAMFS_LOADED\s+\d")
                self.assertNotIn("INIT_EXEC", output)

    def test_line_endings_final_unterminated_record_and_decimal_padding(self):
        log = FULL.replace("ticks=100", "ticks=000100").rstrip("\n")
        for newline in ("\n", "\r\n", "\n\r"):
            with self.subTest(newline=repr(newline)):
                self.assert_valid(log.replace("\n", newline))
        self.assert_valid(MINIMAL.rstrip("\n"), "minimal")
        self.assert_valid(MINIMAL.replace("RustOS: BOOT_MINIMAL_OK", "BOOT_MINIMAL_OK"), "minimal")
        self.assert_valid(FULL.replace(" ticks=", "\t ticks="))

    def test_full_does_not_downgrade_to_minimal(self):
        for log in (MINIMAL, ENTRY + MMU + INITRAMFS, FULL + OK):
            for mode in (None, "full"):
                with self.subTest(log=log, mode=mode):
                    self.assert_invalid(log, mode)
        self.assert_invalid("RustOS: boot-minimal entering common path\n" + FULL)
        self.assert_invalid(FULL + OK, "minimal", "full/userspace contract")

    def test_incomplete_fixtures(self):
        for log in ("", OK, ENTRY + OK, MMU + OK, ENTRY + MMU, ENTRY + MMU + INITRAMFS):
            with self.subTest(log=log):
                self.assert_invalid(log, "minimal")
        for missing in (ENTRY, MMU, INITRAMFS, EXEC):
            with self.subTest(missing=missing):
                self.assert_invalid(FULL.replace(missing, ""))

    def test_regressing_ticks_and_out_of_order_milestones(self):
        cases = (
            (ENTRY + MMU.replace("140", "99") + OK, "minimal"),
            (ENTRY + MMU + INITRAMFS.replace("180", "139") + OK, "minimal"),
            (FULL.replace("220", "179"), "full"),
            (MMU + ENTRY + OK, "minimal"),
            (ENTRY + INITRAMFS + MMU + OK, "minimal"),
            (ENTRY + INITRAMFS + MMU + EXEC, "full"),
            (ENTRY + OK + MMU, "minimal"),
            (ENTRY + "BOOT_MARK label=EXTRA ticks=99\n" + MMU + OK, "minimal"),
        )
        for log, mode in cases:
            with self.subTest(log=log, mode=mode):
                self.assert_invalid(log, mode)

    def test_duplicate_rules_and_multiple_boots(self):
        self.assert_valid(ENTRY + ENTRY + MMU + INITRAMFS + EXEC)
        for log in (
            ENTRY * 3 + MMU + OK,
            ENTRY + "BOOT_MARK label=BOOT_ENTRY ticks=99\n" + MMU + OK,
            ENTRY + MMU + "BOOT_MARK label=BOOT_ENTRY ticks=150\n" + OK,
            ENTRY + MMU * 2 + OK,
            ENTRY + MMU + INITRAMFS * 2 + OK,
            MINIMAL + OK,
            MINIMAL * 2,
            MINIMAL + "BOOT_MARK label=EXTRA ticks=200\n",
        ):
            with self.subTest(log=log):
                self.assert_invalid(log, "minimal")
        self.assert_invalid(FULL + EXEC)
        self.assert_invalid(FULL * 2)

    def test_panic_or_failure_before_and_after_success(self):
        failures = (
            "KERNEL PANIC: fixture\n",
            "PANIC_LOC: src/kernel.rs:1:1\n",
            "thread 'main' panicked at fixture\n",
            "Triple fault\n",
            "[DOUBLE PANIC — halting]\n",
            "║ K E R N E L  P A N I C ║\n",
            "rustos: FATAL: AllocatePool failed\n",
            "RustOS: boot failed\n",
            "BOOT_MINIMAL_FAILED\n",
            "BOOT_FAIL\n",
            "USERSPACE_BOOT_UNSUPPORTED\n",
            "ERROR: could not initialize memory\n",
            "[FAIL] boot\n",
        )
        for mode, log in (("minimal", MINIMAL), ("full", FULL)):
            for failure in failures:
                for failed_log in (failure + log, log + failure):
                    with self.subTest(mode=mode, failure=failure, log=failed_log):
                        self.assert_invalid(failed_log, mode, "panic/failure diagnostic")

    def test_malformed_values_are_not_numeric_prefix_matches(self):
        for value in (
            "", "-1", "+1", "1.5", "1e2", "0x64", "100junk", "NaN",
            "18446744073709551616", "9" * 5000, "100\x00", "100\rjunk",
            "100 extra=1", "１００",
        ):
            for mode, log in (("minimal", MINIMAL), ("full", FULL)):
                with self.subTest(value=value[:30], mode=mode):
                    self.assert_invalid(log.replace("ticks=100", "ticks=" + value), mode)
        # A malformed duplicate cannot be silently skipped after valid records.
        self.assert_invalid(MINIMAL + "BOOT_MARK label=BOOT_ENTRY ticks=100junk\n", "minimal")
        self.assert_invalid(ENTRY + MMU + INITRAMFS.replace("180", "bad") + OK, "minimal")

    def test_prefixed_quoted_or_suffixed_evidence_is_not_accepted(self):
        for fake_entry in (
            "prefix " + ENTRY, "NOT_" + ENTRY, " " + ENTRY,
            '"' + ENTRY.rstrip("\n") + '"\n',
            ENTRY.replace("BOOT_MARK", "BOOT_MARKER"),
            ENTRY.replace("label=BOOT_ENTRY", "label=BOOT_ENTRY_EXTRA"),
        ):
            with self.subTest(fake_entry=fake_entry):
                self.assert_invalid(fake_entry + MMU + OK, "minimal")
        for fake_ok in (
            "NOT_BOOT_MINIMAL_OK\n", "expected RustOS: BOOT_MINIMAL_OK\n",
            "RustOS: BOOT_MINIMAL_OKAY\n", "RustOS: BOOT_MINIMAL_OK extra\n",
            '"RustOS: BOOT_MINIMAL_OK"\n',
        ):
            with self.subTest(fake_ok=fake_ok):
                self.assert_invalid(ENTRY + MMU + fake_ok, "minimal")
        self.assert_invalid(FULL.replace("BOOT_INIT_EXEC", "BOOT_INIT_EXEC_EXTRA"))

    def test_u64_range_and_exact_arithmetic(self):
        output = self.assert_valid(
            FULL.replace("100", "18446744073709551612")
            .replace("140", "18446744073709551613")
            .replace("180", "18446744073709551614")
            .replace("220", "18446744073709551615")
        )
        self.assertRegex(output, r"BOOT_INIT_EXEC\s+18446744073709551615\s+1\n")
        self.assertRegex(output, r"TOTAL \(ENTRY → INIT_EXEC\)\s+3\n")
        output = self.assert_valid(
            MINIMAL.replace("100", "0").replace("140", "18446744073709551615"), "minimal"
        )
        self.assertRegex(output, r"TOTAL \(ENTRY → MMU_ON\)\s+18446744073709551615\n")
        self.assert_valid(FULL.replace("140", "100"))

    def test_unrelated_console_output_and_optional_markers(self):
        log = (
            "\x1b[2JUEFI firmware\nCPU Reset (CPU 0)\n"
            + ENTRY + "BOOT_MARK label=EXTRA ticks=120\n" + MMU + OK
            + "qemu-system-x86_64: terminating on signal 15 from pid 123 (timeout)\n"
        )
        self.assert_valid(log, "minimal")
        self.assert_invalid(log.replace("EXTRA ticks=120", "EXTRA ticks=150"), "minimal")

    def test_invalid_invocation_and_missing_file(self):
        for arguments in ([], ["--mode", "typo", "missing.log"], ["a.log", "b.log"]):
            with self.subTest(arguments=arguments):
                result = subprocess.run(
                    ["bash", str(PARSER), *arguments], capture_output=True, text=True,
                    check=False, timeout=10,
                )
                self.assertEqual(result.returncode, 2, result)
                self.assertEqual(result.stdout, "")
        with tempfile.TemporaryDirectory(prefix="boot-marks-") as directory:
            for path in (Path(directory) / "missing.log", Path(directory)):
                with self.subTest(path=path):
                    result = subprocess.run(
                        ["bash", str(PARSER), str(path)], capture_output=True, text=True,
                        check=False, timeout=10,
                    )
                    self.assertEqual(result.returncode, 1, result)
                    self.assertEqual(result.stdout, "")
                    self.assertIn("BOOT_PERF ERROR:", result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
