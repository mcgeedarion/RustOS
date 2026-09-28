#!/usr/bin/env python3
"""Validate one structured panic from a captured serial log."""

import argparse
from pathlib import Path
import re


def validate(text: str, arch: str) -> list[str]:
    lines = text.replace("\r", "").splitlines()
    fields = [
        r"KERNEL PANIC: .+",
        r"PANIC_LOC: (?:.+:\d+:\d+|unknown)",
        rf"PANIC_ARCH: {re.escape(arch)}",
        r"PANIC_FAULT_ADDR: 0x[0-9a-fA-F]+",
        r"--- REGISTER DUMP ---",
        r"--- BACKTRACE ---",
        r"--- END PANIC ---",
    ]
    errors = []
    positions = []
    for pattern in fields:
        matches = [i for i, line in enumerate(lines) if re.fullmatch(pattern, line)]
        if len(matches) != 1:
            errors.append(f"expected exactly one field matching {pattern!r}; got {len(matches)}")
        else:
            positions.append(matches[0])
    for prefix in ["KERNEL PANIC:", "PANIC_LOC:", "PANIC_ARCH:", "PANIC_FAULT_ADDR:"]:
        if sum(line.startswith(prefix) for line in lines) != 1:
            errors.append(f"missing or duplicate {prefix} field")
    if positions != sorted(positions):
        errors.append("panic fields are out of order")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path)
    parser.add_argument("--arch", required=True, choices=["x86_64", "aarch64"])
    args = parser.parse_args()
    errors = validate(args.log.read_text(errors="replace"), args.arch)
    if errors:
        for error in errors:
            print(f"PANIC_FORMAT ERROR: {error}")
        return 1
    print(f"PANIC_FORMAT OK: one complete {args.arch} panic")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
