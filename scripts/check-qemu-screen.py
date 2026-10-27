#!/usr/bin/env python3
"""Verify the Everest OS desktop shell in a QEMU framebuffer screenshot."""

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
        print(f"Unexpected PPM payload size: expected {expected_size}, got {len(pixels)}.", file=sys.stderr)
        return 1

    def pixel_at(x: int, y: int) -> bytes:
        offset = (y * width + x) * 3
        return pixels[offset:offset + 3]

    def reversed_color(color: tuple[int, int, int]) -> bytes:
        return bytes((color[2], color[1], color[0]))

    def matches_color(x: int, y: int, color: tuple[int, int, int]) -> bool:
        actual = pixel_at(x, y)
        return actual == bytes(color) or actual == reversed_color(color)

    # The small status indicator is intentionally retained on the taskbar.
    marker = (110, 190, 255)
    marker_reverse = (255, 190, 110)
    colors = [pixels[offset:offset + 3] for offset in range(0, len(pixels), 3)]
    marker_count = sum(color == bytes(marker) or color == bytes(marker_reverse) for color in colors)
    if marker_count < 10:
        print(f"Kernel page-table/IDT health marker missing ({marker_count} pixels).", file=sys.stderr)
        return 1

    if width < 800 or height < 600:
        print(f"Desktop test requires at least 800x600; got {width}x{height}.", file=sys.stderr)
        return 1

    # Check real framebuffer colors, allowing either RGB or BGR output.
    checks = [
        ("desktop background", 20, 20, (12, 17, 32)),
        ("taskbar", 10, height - 10, (20, 28, 48)),
        ("Start button", 20, height - 28, (42, 62, 96)),
        ("window border", width // 5, height // 5 + 100, (92, 145, 220)),
        ("window title bar", width // 5 + 10, height // 5 + 10, (33, 47, 74)),
        ("window body", width // 5 + 10, height // 5 + 100, (14, 20, 36)),
    ]
    for label, x, y, color in checks:
        if not matches_color(x, y, color):
            print(f"{label} pixel mismatch at ({x}, {y}); expected {color}.", file=sys.stderr)
            return 1

    glyphs = {
        "A": [14, 17, 17, 31, 17, 17, 17],
        "E": [31, 16, 16, 30, 16, 16, 31],
        "D": [30, 17, 17, 17, 17, 17, 30],
        "I": [31, 4, 4, 4, 4, 4, 31],
        "K": [17, 18, 20, 24, 20, 18, 17],
        "O": [14, 17, 17, 17, 17, 17, 14],
        "D": [30, 17, 17, 17, 17, 17, 30],
        "I": [31, 4, 4, 4, 4, 4, 31],
        "K": [17, 18, 20, 24, 20, 18, 17],
        "O": [14, 17, 17, 17, 17, 17, 14],
        "M": [17, 27, 21, 21, 17, 17, 17],
        "N": [17, 25, 25, 21, 19, 19, 17],
        "R": [30, 17, 17, 30, 20, 18, 17],
        "S": [15, 16, 16, 14, 1, 1, 30],
        "T": [31, 4, 4, 4, 4, 4, 4],
        "Z": [31, 1, 2, 4, 8, 16, 31],
        "P": [30, 17, 17, 30, 16, 16, 16],
        "U": [17, 17, 17, 17, 17, 17, 14],
        "C": [14, 17, 16, 16, 16, 17, 14],
        "V": [17, 17, 17, 17, 17, 10, 4],
        "0": [14, 17, 19, 21, 25, 17, 14],
        "1": [4, 12, 4, 4, 4, 4, 14],
        "2": [14, 17, 1, 2, 4, 8, 31],
        "a": [0, 0, 14, 1, 15, 17, 15],
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
        "v": [0, 0, 17, 17, 17, 10, 4],
        ":": [0, 4, 4, 0, 4, 4, 0],
        " ": [0, 0, 0, 0, 0, 0, 0],
    }
    foreground = (255, 255, 255)

    def verify_text(message: str, origin_x: int, origin_y: int, scale: int,
                    background: tuple[int, int, int]) -> bool:
        for char_index, character in enumerate(message):
            rows = glyphs[character]
            for row, bits in enumerate(rows):
                for col in range(6):
                    lit = col < 5 and bool(bits & (1 << (4 - col)))
                    for dy in range(scale):
                        for dx in range(scale):
                            x = origin_x + char_index * 6 * scale + col * scale + dx
                            y = origin_y + row * scale + dy
                            if x >= width or y >= height:
                                print(f"Text {message!r} extends beyond the framebuffer.", file=sys.stderr)
                                return False
                            actual = pixel_at(x, y)
                            if lit:
                                if actual not in (bytes(foreground), reversed_color(foreground)):
                                    print(f"Text {message!r} has a missing glyph pixel at ({x}, {y}).", file=sys.stderr)
                                    return False
                            elif actual not in (bytes(background), reversed_color(background)):
                                print(f"Text {message!r} background mismatch at ({x}, {y}).", file=sys.stderr)
                                return False
        return True

    window_x = width // 5
    window_y = height // 5
    if not verify_text("Everest Terminal", window_x + 24, window_y + 13, 2, (33, 47, 74)):
        print("Terminal window title was not rendered correctly.", file=sys.stderr)
        return 1
    if "--username" in sys.argv:
        try:
            username = sys.argv[sys.argv.index("--username") + 1]
        except IndexError:
            print("--username requires a value.", file=sys.stderr)
            return 2
        if not verify_text(username, window_x + 24, window_y + 140, 2, (14, 20, 36)):
            print("Expected saved username was not rendered in the desktop.", file=sys.stderr)
            return 1
    if "--timezone" in sys.argv:
        try:
            timezone = sys.argv[sys.argv.index("--timezone") + 1]
        except IndexError:
            print("--timezone requires a value.", file=sys.stderr)
            return 2
        if not verify_text("TIMEZONE:", window_x + 24, window_y + 162, 1, (14, 20, 36)):
            print("Timezone label was not rendered correctly.", file=sys.stderr)
            return 1
        if not verify_text(timezone, window_x + 24 + 9 * 6, window_y + 162, 1, (14, 20, 36)):
            print("Expected saved timezone was not rendered in the desktop.", file=sys.stderr)
            return 1
    if not verify_text("START", 30, height - 36, 2, (42, 62, 96)):
        print("Taskbar Start button label was not rendered correctly.", file=sys.stderr)
        return 1
    if not verify_text("12:00", width - 120, height - 37, 2, (20, 28, 48)):
        print("Taskbar clock was not rendered correctly.", file=sys.stderr)
        return 1

    file_x, file_y = window_x + 24, window_y + 194
    if not verify_text("EVEREST DISK OK", file_x, file_y, 1, (14, 20, 36)):
        print("FAT32 HELLO.TXT contents were not rendered correctly.", file=sys.stderr)
        return 1

    print(
        f"QEMU UEFI desktop passed: health marker={marker_count}; verified desktop "
        f"and FAT32 HELLO.TXT contents in the {width}x{height} framebuffer."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
