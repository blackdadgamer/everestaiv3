#!/usr/bin/env python3
"""Check that the QEMU framebuffer screenshot contains the kernel status color."""

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

    # This exact color is used for the kernel's KERNEL ONLINE status text.
    expected = bytes((118, 222, 190))
    count = sum(
        1
        for offset in range(0, len(pixels), 3)
        if pixels[offset:offset + 3] == expected
    )
    if count < 10:
        print(
            f"No kernel status screen detected ({count} status-color pixels in {width}x{height}).",
            file=sys.stderr,
        )
        return 1

    print(f"QEMU UEFI boot passed: detected {count} kernel status pixels in {width}x{height}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
