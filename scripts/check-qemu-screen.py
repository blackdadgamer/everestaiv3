#!/usr/bin/env python3
"""Check that QEMU reaches the kernel with its own page tables and IDT active."""

import re
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: check-qemu-screen.py SCREEN.ppm", file=sys.stderr)
        return 2

    data = Path(sys.argv[1]).read_bytes()
    match = re.match(rb"P6\s+(\d+)\s+(\d+)\s+255\s", data)
    if not match:
        print("QEMU screenshot is not a supported binary PPM image.", file=sys.stderr)
        return 1

    width, height = int(match.group(1)), int(match.group(2))
    pixels = data[match.end():]
    expected_size = width * height * 3
    if len(pixels) != expected_size:
        print(
            f"Unexpected PPM payload size: expected {expected_size}, got {len(pixels)}.",
            file=sys.stderr,
        )
        return 1

    # Unique color used only for the independent page-table + IDT status.
    expected = bytes((110, 190, 255))
    reverse_expected = bytes((255, 190, 110))
    old_kernel_status = bytes((118, 222, 190))
    exception_status = bytes((255, 120, 145))
    colors = [pixels[offset:offset + 3] for offset in range(0, len(pixels), 3)]
    count = sum(color == expected or color == reverse_expected for color in colors)
    old_count = sum(color == old_kernel_status for color in colors)
    exception_count = sum(color == exception_status for color in colors)
    if count < 10:
        print(
            f"No independent page tables/IDT status detected ({count} expected pixels; "
            f"{old_count} legacy kernel-status pixels; {exception_count} exception pixels "
            f"in {width}x{height}).",
            file=sys.stderr,
        )
        return 1

    print(f"QEMU UEFI boot passed: detected {count} independent page-table/IDT status pixels in {width}x{height}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
