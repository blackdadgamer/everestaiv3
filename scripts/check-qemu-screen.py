#!/usr/bin/env python3
"""Verify Everest OS desktop, launcher, calculator input, and window framebuffer."""
import re
import sys
from pathlib import Path

def main() -> int:
    if len(sys.argv) < 2:
        print("usage: check-qemu-screen.py SCREEN [--mouse-drag|--start-menu|--calculator-window|--calculator-result]", file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    data = path.read_bytes()
    match = re.match(rb"P6\s+(\d+)\s+(\d+)\s+(\d+)\s", data)
    if not match:
        print("Invalid PPM screenshot.", file=sys.stderr)
        return 1
    width, height, maximum = map(int, match.groups())
    pixels = data[match.end():]
    if maximum != 255 or len(pixels) != width * height * 3:
        print("Unexpected framebuffer payload.", file=sys.stderr)
        return 1
    def pixel(x: int, y: int) -> tuple[int, int, int]:
        i = (y * width + x) * 3
        return tuple(pixels[i:i+3])
    def is_color(actual: tuple[int, int, int], expected: tuple[int, int, int]) -> bool:
        return actual == expected or actual == expected[::-1]
    if width < 800 or height < 600:
        print(f"Framebuffer too small: {width}x{height}", file=sys.stderr)
        return 1
    if not is_color(pixel(5, 5), (26, 27, 38)):
        print("Tokyo Night desktop background mismatch.", file=sys.stderr)
        return 1
    if "--start-menu" in sys.argv:
        if not is_color(pixel(12, height - 36 - 198), (187, 154, 247)):
            print("Start menu popup was not visible above the taskbar.", file=sys.stderr)
            return 1
        print("Start menu popup verified.")
        return 0
    if "--calculator-window" in sys.argv:
        if not is_color(pixel((width - 520) // 2, (height - 336) // 2), (187, 154, 247)):
            print("Calculator window was not spawned at the center of the screen.", file=sys.stderr)
            return 1
        print("Centered Calculator window verified.")
        return 0
    if "--calculator-result" in sys.argv:
        marker_x = (width - 520) // 2 + 520 - 30
        marker_y = (height - 336) // 2 + 62
        if not is_color(pixel(marker_x, marker_y), (80, 200, 160)):
            print("Calculator did not update its display with a completed addition.", file=sys.stderr)
            return 1
        print("Calculator mouse input and addition result marker verified.")
        return 0
    # Window border is a long violet line; locate it rather than assuming it
    # remained centered after the mouse drag.
    border = (187, 154, 247)
    candidates = []
    for y in range(0, height // 2):
        run = 0
        for x in range(width):
            if is_color(pixel(x, y), border):
                run += 1
                if run >= (width * 3 // 4) - 8:
                    candidates.append((x - run + 1, y))
                    break
            else:
                run = 0
    if not candidates:
        print("File Manager violet window border not detected.", file=sys.stderr)
        return 1
    wx, wy = candidates[0]
    if "--mouse-drag" in sys.argv:
        centered_x = (width - width * 3 // 4) // 2
        centered_y = (height - height * 3 // 4) // 2
        if abs(wx - centered_x) < 8 and abs(wy - centered_y) < 8:
            print("Mouse drag did not move the File Manager window.", file=sys.stderr)
            return 1
    # Check the terminal's dark panel and saved-success green text when the
    # first boot has just submitted a command.
    terminal_y = wy + (height * 3 // 4) - 112
    if not is_color(pixel(wx + 24, terminal_y + 2), (16, 18, 27)):
        print("Terminal panel background mismatch.", file=sys.stderr)
        return 1
    if "--mouse-drag" not in sys.argv:
        green = (118, 222, 190)
        count = sum(1 for i in range(0, len(pixels), 3)
                    if is_color(tuple(pixels[i:i+3]), green))
        if count < 20:
            print("File Saved Successfully feedback was not visible.", file=sys.stderr)
            return 1
    print(f"QEMU desktop passed: {width}x{height}, terminal panel verified"
          + (", titlebar drag verified." if "--mouse-drag" in sys.argv else
             ", save-success feedback verified."))
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
