#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$ROOT_DIR"

for command_name in qemu-system-x86_64 mkfs.vfat mmd mcopy socat python3; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
        printf 'Missing QEMU test dependency: %s\n' "$command_name" >&2
        exit 2
    fi
done

EFI_CODE="$(find /usr/share/OVMF -type f \( -name 'OVMF_CODE.fd' -o -name 'OVMF_CODE_4M.fd' \) -print -quit 2>/dev/null || true)"
if [ -z "$EFI_CODE" ]; then
    printf 'Could not find OVMF_CODE.fd under /usr/share/OVMF.\n' >&2
    exit 2
fi

sh scripts/build-uefi.sh

TMP_DIR="$(mktemp -d)"
QEMU_PID=""
cleanup() {
    if [ -n "$QEMU_PID" ]; then
        kill "$QEMU_PID" >/dev/null 2>&1 || true
        wait "$QEMU_PID" >/dev/null 2>&1 || true
    fi
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

USB_IMAGE="$TMP_DIR/everest-usb.img"
MONITOR="$TMP_DIR/qemu-monitor.sock"
SCREEN="$TMP_DIR/everest-screen.ppm"

truncate -s 64M "$USB_IMAGE"
mkfs.vfat "$USB_IMAGE" >/dev/null
mmd -i "$USB_IMAGE" ::/EFI
mmd -i "$USB_IMAGE" ::/EFI/BOOT
mcopy -i "$USB_IMAGE" dist/EFI/BOOT/BOOTX64.EFI ::/EFI/BOOT/BOOTX64.EFI

qemu-system-x86_64 \
    -machine q35 \
    -m 512M \
    -drive "if=pflash,format=raw,readonly=on,file=$EFI_CODE" \
    -drive "format=raw,file=$USB_IMAGE" \
    -display none \
    -monitor "unix:$MONITOR,server,nowait" \
    -serial none \
    -net none \
    -no-reboot \
    -no-shutdown >"$TMP_DIR/qemu.log" 2>&1 &
QEMU_PID=$!

attempt=0
while [ ! -S "$MONITOR" ] && [ "$attempt" -lt 50 ]; do
    sleep 0.1
    attempt=$((attempt + 1))
done
if [ ! -S "$MONITOR" ]; then
    cat "$TMP_DIR/qemu.log" >&2
    printf 'QEMU monitor socket did not appear.\n' >&2
    exit 1
fi

# The firmware splash is shown briefly, then the early kernel status screen
# remains visible. Capture the display after the handoff window.
sleep 5
printf 'screendump %s\nquit\n' "$SCREEN" | socat - "UNIX-CONNECT:$MONITOR" >/dev/null
wait "$QEMU_PID" || true
QEMU_PID=""

python3 scripts/check-qemu-screen.py "$SCREEN"
