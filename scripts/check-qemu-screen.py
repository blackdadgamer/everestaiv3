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

    # Verify actual framebuffer glyphs for both the welcome title and typed input.
    glyphs = {
        "A": [14, 17, 17, 31, 17, 17, 17],
        "E": [31, 16, 16, 30, 16, 16, 31],
        "H": [17, 17, 17, 31, 17, 17, 17],
        "O": [14, 17, 17, 17, 17, 17, 14],
        "S": [15, 16, 16, 14, 1, 1, 30],
        "W": [17, 17, 17, 21, 21, 21, 10],
        "a": [0, 0, 14, 1, 15, 17, 15],
        "c": [0, 0, 14, 16, 16, 17, 14],
        "d": [1, 1, 15, 17, 17, 17, 15],
        "e": [0, 0, 14, 17, 31, 16, 14],
        "i": [4, 0, 12, 4, 4, 4, 14],
        "l": [12, 4, 4, 4, 4, 4, 14],
        "m": [0, 0, 26, 21, 21, 21, 21],
        "n": [0, 0, 30, 17, 17, 17, 17],
        "o": [0, 0, 14, 17, 17, 17, 14],
        "r": [0, 0, 22, 25, 16, 16, 16],
        "s": [0, 0, 15, 16, 14, 1, 30],
        "t": [8, 8, 28, 8, 8, 9, 6],
        "u": [0, 0, 17, 17, 17, 19, 13],
        "v": [0, 0, 17, 17, 17, 10, 4],
        " ": [0, 0, 0, 0, 0, 0, 0],
    }
    foreground = bytes((255, 255, 255))
    screen_backgrounds = (bytes((5, 8, 24)), bytes((24, 8, 5)))
    input_backgrounds = (bytes((18, 26, 48)), bytes((48, 26, 18)))

    def pixel_at(x: int, y: int) -> bytes:
        offset = (y * width + x) * 3
        return pixels[offset:offset + 3]

    def verify_text(message: str, origin_x: int, origin_y: int, scale: int,
                    backgrounds: tuple[bytes, ...]) -> bool:
        for char_index, character in enumerate(message):
            rows = glyphs[character]
            for row, bits in enumerate(rows):
                for col in range(6):
                    lit = col < 5 and bool(bits & (1 << (4 - col)))
                    expected_color = foreground if lit else None
                    for dy in range(scale):
                        for dx in range(scale):
                            x = origin_x + char_index * 6 * scale + col * scale + dx
                            y = origin_y + row * scale + dy
                            if x >= width or y >= height:
                                print(f"Text {message!r} extends beyond the framebuffer.", file=sys.stderr)
                                return False
                            actual = pixel_at(x, y)
                            if (actual != expected_color if expected_color else actual not in backgrounds):
                                print(
                                    f"Text mismatch for {message!r} at character "
                                    f"{character!r}, row {row}, column {col}.",
                                    file=sys.stderr,
                                )
                                return False
        return True

    if not verify_text("Welcome to Everest OS", 80, 96, 3, screen_backgrounds):
        return 1
    if not verify_text("admin", 96, 205, 3, input_backgrounds):
        print("The injected username 'admin' was not rendered in the input box.", file=sys.stderr)
        return 1

    print(
        f"QEMU UEFI boot passed: page-table/IDT={count}; verified welcome text "
        f"and username 'admin' in the framebuffer ({width}x{height})."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
