#!/usr/bin/env python3
"""Strict single-boot contracts behind parse-boot-marks.sh (stdlib only)."""

import argparse
import re
import sys
from pathlib import Path


ORDERED = {
    "minimal": ("BOOT_ENTRY", "BOOT_MMU_ON"),
    "full": (
        "BOOT_ENTRY",
        "BOOT_MMU_ON",
        "BOOT_INITRAMFS_LOADED",
        "BOOT_INIT_EXEC",
    ),
}
U64_MAX = (1 << 64) - 1
MARK = re.compile(r"BOOT_MARK[ \t]+label=([A-Z0-9_]+)[ \t]+ticks=([0-9]+)")
MINIMAL_OK = {"BOOT_MINIMAL_OK", "RustOS: BOOT_MINIMAL_OK"}
MINIMAL_BANNER = "RustOS: boot-minimal entering common path"
# Scan the entire log, including text after the success milestone. Fail closed
# on failure diagnostics, but not ordinary QEMU timeouts/CPU Reset tracing.
FAILURE = re.compile(
    r"\b(?:panic(?:ked)?|fatal|fail(?:ed|ure)?|error)\b"
    r"|\bPANIC_[A-Z0-9_]+\b"
    r"|\b[A-Z0-9_]+_(?:FAIL(?:ED|URE)?|ERROR|UNSUPPORTED)\b"
    r"|K E R N E L[ \t]+P A N I C"
    r"|\btriple[ ._-]*fault\b",
    re.IGNORECASE,
)


class InvalidLog(Exception):
    """The log cannot substantiate the requested boot contract."""


def validate(lines, mode):
    ordered = ORDERED[mode]
    marks = {}
    next_milestone = 0
    last_ticks = None
    entry_count = 0
    minimal_ok = False
    placeholder = False

    for line_number, raw_line in enumerate(lines, 1):
        # UART output can be LF, CRLF, or LFCR (CR at the next line's start).
        # Only trim boundary CRs: an embedded CR must not repair a broken tick.
        line = raw_line.rstrip("\n").strip("\r")

        def reject(reason):
            raise InvalidLog(f"line {line_number}: {reason}")

        if FAILURE.search(line):
            reject(f"panic/failure diagnostic: {line}")

        if line == MINIMAL_BANNER and mode == "full":
            reject("minimal boot evidence is not a full/userspace boot")

        if line in MINIMAL_OK:
            if mode != "minimal":
                reject("BOOT_MINIMAL_OK cannot satisfy the full/userspace contract")
            if minimal_ok:
                reject("duplicate BOOT_MINIMAL_OK (expected one boot)")
            if next_milestone != len(ordered):
                reject("BOOT_MINIMAL_OK appeared before required minimal milestones")
            minimal_ok = True
            continue
        if any(line.startswith(sentinel) for sentinel in MINIMAL_OK):
            reject("malformed BOOT_MINIMAL_OK sentinel")

        # Never extract a marker from a quoted, prefixed, or embedded substring.
        # A record beginning with BOOT_MARK must be well-formed in its entirety.
        if not line.startswith("BOOT_MARK"):
            continue
        match = MARK.fullmatch(line)
        if match is None:
            reject("malformed BOOT_MARK record")
        label, value = match.groups()
        # Permit leading zeroes as decimal, not octal. Bound the string before
        # int() so arbitrarily long corrupt records produce a clean diagnostic.
        decimal = value.lstrip("0") or "0"
        if len(decimal) > 20 or int(decimal) > U64_MAX:
            reject(f"{label} ticks outside unsigned 64-bit range")
        ticks = int(decimal)
        if last_ticks is not None and ticks < last_ticks:
            reject(f"regressing ticks at {label}: {ticks} < {last_ticks}")
        last_ticks = ticks

        if minimal_ok:
            reject("BOOT_MARK after BOOT_MINIMAL_OK (expected one completed boot)")
        if label in marks:
            # kernel_main and boot_minimal::enter currently both emit ENTRY.
            # Keep the FIRST baseline, allow exactly one monotonic repeat only
            # before any other marker, and never silently overwrite a record.
            if label == "BOOT_ENTRY" and entry_count == 1 and len(marks) == 1:
                entry_count += 1
                continue
            reject(f"duplicate {label} (only two initial BOOT_ENTRY records allowed)")

        if mode == "minimal":
            if label == "BOOT_INIT_EXEC":
                reject("BOOT_INIT_EXEC belongs to the full/userspace contract")
            if label == "BOOT_INITRAMFS_LOADED":
                if next_milestone != len(ordered):
                    reject("legacy minimal placeholder appeared before BOOT_MMU_ON")
                # Check syntax, range and order, but never report an initramfs
                # performance segment: this path does not load an initramfs.
                placeholder = True

        if label in ordered:
            if next_milestone >= len(ordered) or label != ordered[next_milestone]:
                reject(f"out-of-order milestone {label}")
            next_milestone += 1
        marks[label] = ticks
        if label == "BOOT_ENTRY":
            entry_count += 1

    missing = list(ordered[next_milestone:])
    if mode == "minimal" and not minimal_ok:
        missing.append("BOOT_MINIMAL_OK")
    if missing:
        raise InvalidLog(f"missing milestones: {', '.join(missing)}")
    return marks, entry_count, placeholder


def report(mode, marks, entry_count, placeholder):
    ordered = ORDERED[mode]
    print(f"RustOS boot performance markers — {mode} contract")
    if entry_count == 2:
        print("NOTE: two initial BOOT_ENTRY records; using the first as baseline.")
    if mode == "minimal":
        if placeholder:
            print("NOTE: ignoring legacy BOOT_INITRAMFS_LOADED placeholder for timing.")
        print("Minimal timing covers ENTRY to MMU_ON only; no initramfs/userspace timing.")
        print("BOOT_MINIMAL_OK verified separately (untimed completion sentinel).")
    print(f"{'MILESTONE':<30}  {'TICKS (abs)':>20}  {'DELTA (ticks)':>20}")
    print("-" * 74)
    previous = None
    for label in ordered:
        ticks = marks[label]
        delta = "(baseline)" if previous is None else str(ticks - previous)
        print(f"{label:<30}  {ticks:>20}  {delta:>20}")
        previous = ticks
    print("-" * 74)
    end = ordered[-1]
    total = marks[end] - marks["BOOT_ENTRY"]
    print(f"{'TOTAL (ENTRY → ' + end.removeprefix('BOOT_') + ')':<30}  {'':>20}  {total:>20}")
    print(f"BOOT_PERF OK — {mode} contract validated.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mode",
        choices=tuple(ORDERED),
        default="full",
        help="full (default) requires INIT_EXEC; minimal requires BOOT_MINIMAL_OK",
    )
    parser.add_argument("log", type=Path, help="QEMU serial log from a single boot")
    args = parser.parse_args()
    try:
        # newline="\n" preserves CRs for explicit UART normalization above;
        # iteration also processes a final record without a terminating newline.
        with args.log.open(encoding="utf-8", errors="replace", newline="\n") as log:
            marks, entry_count, placeholder = validate(log, args.mode)
    except (OSError, InvalidLog) as error:
        print(f"BOOT_PERF ERROR: {error}", file=sys.stderr)
        return 1
    # No table or success message is printed until the WHOLE log is validated.
    report(args.mode, marks, entry_count, placeholder)
    return 0


if __name__ == "__main__":
    sys.exit(main())
