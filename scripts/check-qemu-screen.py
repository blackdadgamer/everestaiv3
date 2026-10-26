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

    # This unique color is used only for the independent page-table + IDT status.
    expected = bytes((110, 190, 255))
    count = sum(
        1
        for offset in range(0, len(pixels), 3)
        if pixels[offset:offset + 3] == expected
    )
    if count < 10:
        print(
            f"No independent page tables/IDT status detected ({count} diagnostic pixels in {width}x{height}).",
            file=sys.stderr,
        )
        return 1

    print(f"QEMU UEFI boot passed: detected {count} independent page-table/IDT status pixels in {width}x{height}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
