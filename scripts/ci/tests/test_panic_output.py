import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from panic_output import validate

PANIC = """firmware boot noise
KERNEL PANIC: kernel: no initramfs registered
PANIC_LOC: src/userspace_boot.rs:48:9
PANIC_ARCH: x86_64
PANIC_FAULT_ADDR: 0x0
--- REGISTER DUMP ---
  (not available in this profile)
--- BACKTRACE ---
  (not available in this profile)
--- END PANIC ---
"""


class PanicOutputTests(unittest.TestCase):
    def test_complete_panic_and_crlf(self):
        self.assertEqual(validate(PANIC, "x86_64"), [])
        self.assertEqual(validate(PANIC.replace("\n", "\r\n"), "x86_64"), [])
        self.assertEqual(validate(PANIC.replace("x86_64", "aarch64"), "aarch64"), [])

    def test_missing_fields_fail(self):
        for line in PANIC.splitlines():
            if line.startswith(("KERNEL PANIC:", "PANIC_", "---")):
                with self.subTest(line=line):
                    self.assertTrue(validate(PANIC.replace(line + "\n", ""), "x86_64"))

    def test_wrong_or_duplicated_architecture_fails(self):
        self.assertTrue(validate(PANIC, "aarch64"))
        self.assertTrue(validate(PANIC.replace("x86_64", "x86_64x86_64"), "x86_64"))
        self.assertTrue(validate(PANIC + "PANIC_ARCH: x86_64\n", "x86_64"))

    def test_multiple_panics_fail(self):
        self.assertTrue(validate(PANIC + PANIC, "x86_64"))

    def test_malformed_fault_address_fails(self):
        self.assertTrue(validate(PANIC.replace("0x0", "not-an-address"), "x86_64"))

    def test_out_of_order_sections_fail(self):
        reordered = PANIC.replace("--- REGISTER DUMP ---", "TEMP")
        reordered = reordered.replace("--- BACKTRACE ---", "--- REGISTER DUMP ---")
        reordered = reordered.replace("TEMP", "--- BACKTRACE ---")
        self.assertTrue(validate(reordered, "x86_64"))


if __name__ == "__main__":
    unittest.main()
