"""Coverage-driver orchestration tests with fake tools, not real LLVM coverage."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "fuzz-coverage.sh"
TARGETS = ("cpio_newc", "elf64", "ext4_metadata", "vfs_path")
TOOL = r'''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys
root = Path(os.environ["MOCK_ROOT"])
mode = os.environ.get("MOCK_MODE", "success")
name = Path(sys.argv[0]).name
args = sys.argv[1:]
if name == "git":
    print(root)
elif name == "rustc":
    if "-vV" in args:
        print("rustc mocked\nhost: mock-host" if mode != "missing_host" else "rustc mocked")
    else:
        print(root / "toolchain")
elif "--version" in args:
    print(name + " mocked version")
elif name == "cargo":
    target = args[2]
    assert args[:2] == ["fuzz", "coverage"]
    assert os.environ["RUSTFLAGS"] == "-C instrument-coverage"
    with (root / "invocations.jsonl").open("a") as stream:
        stream.write(json.dumps(args) + "\n")
    if mode == "command_failure" and target == "elf64":
        sys.exit(7)
    host = args[args.index("--target") + 1] if "--target" in args else "mock-host"
    # Model upstream's default: a separate root target/<host>/coverage directory.
    build = Path(args[args.index("--target-dir") + 1]) if "--target-dir" in args else root / "target" / host / "coverage"
    raw = root / "fuzz" / "coverage" / target / "raw"
    raw.mkdir(parents=True, exist_ok=True)
    if not (mode == "no_profiles" and target == "elf64"):
        (raw / "input.profraw").write_text("MOCK profile")
    binary = build / host / "release" / target
    binary.parent.mkdir(parents=True, exist_ok=True)
    if not (mode == "no_binary" and target == "elf64"):
        binary.write_text("MOCK binary")
        binary.chmod(0o755)
elif name == "llvm-profdata":
    assert args[0] == "merge"
    Path(args[args.index("-o") + 1]).write_text("MOCK indexed profile")
elif name == "llvm-cov":
    command, binary = args[:2]
    assert Path(binary).is_file()
    target = Path(binary).name
    if mode == "report_failure" and target == "elf64" and command == "report":
        sys.exit(8)
    if command == "report":
        print("MOCK source coverage table")
    elif command == "export":
        value = {} if mode == "bad_summary" and target == "elf64" else {
            "data": [{"totals": {key: {"covered": 2, "count": 4, "percent": 50.0} for key in ("regions", "lines")}}]
        }
        print(json.dumps(value))
    elif command == "show":
        out = Path(next(arg.split("=", 1)[1] for arg in args if arg.startswith("-output-dir=")))
        out.mkdir(parents=True, exist_ok=True)
        (out / "index.html").write_text("MOCK HTML")
'''


@unittest.skipUnless(shutil.which("bash") and shutil.which("timeout"), "requires Bash and timeout")
class FuzzCoverageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="rustos coverage ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / "mock-bin"
        self.llvm = self.root / "toolchain/lib/rustlib/mock-host/bin"
        for directory, names in ((self.bin, ("git", "rustc", "cargo")),
                                 (self.llvm, ("llvm-profdata", "llvm-cov"))):
            directory.mkdir(parents=True)
            for name in names:
                tool = directory / name
                tool.write_text(TOOL)
                tool.chmod(0o755)
        for target in TARGETS:
            (self.root / "fuzz/corpus" / target).mkdir(parents=True)

    def run_driver(self, mode="success"):
        env = dict(os.environ, MOCK_ROOT=str(self.root), MOCK_MODE=mode,
                   PATH=str(self.bin) + os.pathsep + os.environ["PATH"])
        result = subprocess.run(["bash", str(SCRIPT)], env=env, text=True, capture_output=True)
        status = (self.root / "fuzz-coverage-report/status.tsv").read_text()
        return result, status

    def test_explicit_binary_layout_and_reports_for_every_corpus(self):
        result, status = self.run_driver()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(status.count("\treported\n"), len(TARGETS))
        calls = [json.loads(line) for line in (self.root / "invocations.jsonl").read_text().splitlines()]
        self.assertEqual(len(calls), len(TARGETS))
        for args in calls:
            target = args[2]
            self.assertEqual(args[args.index("--target-dir") + 1], str(self.root / "target/fuzz-coverage" / target))
            self.assertEqual(args[args.index("--target") + 1], "mock-host")
            self.assertEqual(args[args.index("--llvm-path") + 1], str(self.llvm))
            out = self.root / "fuzz-coverage-report" / target
            for path in ("coverage.profdata", "report.txt", "coverage.json", "html/index.html"):
                self.assertTrue((out / path).is_file(), path)
        self.assertIn("elf64 regions: 2/4 (50.00%)", result.stdout)
        self.assertTrue((self.root / "fuzz-coverage-report/toolchain.txt").is_file())

    def test_stale_fuzz_binaries_do_not_confuse_resolution(self):
        for directory in ("fuzz/target/old", "fuzz/target/other"):
            path = self.root / directory / "elf64"
            path.parent.mkdir(parents=True)
            path.write_text("STALE binary")
            path.chmod(0o755)
        result, status = self.run_driver()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(status.count("\treported\n"), 4)

    def assert_one_target_failure(self, mode, expected_status):
        result, status = self.run_driver(mode)
        self.assertEqual(result.returncode, 1)
        self.assertIn("elf64\t" + expected_status + "\n", status)
        self.assertEqual(status.count("\treported\n"), 3)

    def test_command_failure_does_not_skip_other_corpora(self):
        self.assert_one_target_failure("command_failure", "coverage-command-failed")

    def test_missing_raw_profiles(self):
        self.assert_one_target_failure("no_profiles", "missing-raw-profiles")

    def test_missing_binary(self):
        self.assert_one_target_failure("no_binary", "missing-coverage-inputs")

    def test_report_failure(self):
        self.assert_one_target_failure("report_failure", "report-generation-failed")

    def test_bad_summary(self):
        self.assert_one_target_failure("bad_summary", "summary-generation-failed")

    def test_empty_corpus_tree(self):
        shutil.rmtree(self.root / "fuzz/corpus")
        result, status = self.run_driver()
        self.assertEqual(result.returncode, 1)
        self.assertEqual(status, "target\tstatus\n")

    def test_missing_llvm_tools_is_recorded(self):
        shutil.rmtree(self.llvm)
        result, status = self.run_driver()
        self.assertEqual(result.returncode, 1)
        self.assertIn("__setup__\tmissing-llvm-tools-preview", status)
        self.assertFalse((self.root / "invocations.jsonl").exists())

    def test_missing_host_triple_is_recorded(self):
        result, status = self.run_driver("missing_host")
        self.assertEqual(result.returncode, 1)
        self.assertIn("__setup__\tmissing-host-triple", status)


if __name__ == "__main__":
    unittest.main()
