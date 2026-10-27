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

EFI_CODE=""
EFI_VARS=""
if [ -f /usr/share/OVMF/OVMF_CODE.fd ] && [ -f /usr/share/OVMF/OVMF_VARS.fd ]; then
    EFI_CODE="/usr/share/OVMF/OVMF_CODE.fd"
    EFI_VARS="/usr/share/OVMF/OVMF_VARS.fd"
elif [ -f /usr/share/OVMF/OVMF_CODE_4M.fd ] && [ -f /usr/share/OVMF/OVMF_VARS_4M.fd ]; then
    EFI_CODE="/usr/share/OVMF/OVMF_CODE_4M.fd"
    EFI_VARS="/usr/share/OVMF/OVMF_VARS_4M.fd"
else
    printf 'Could not find a matching OVMF CODE/VARS pair under /usr/share/OVMF.\n' >&2
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
VARS_COPY="$TMP_DIR/OVMF_VARS.fd"
cp "$EFI_VARS" "$VARS_COPY"

truncate -s 64M "$USB_IMAGE"
mkfs.vfat "$USB_IMAGE" >/dev/null
mmd -i "$USB_IMAGE" ::/EFI
mmd -i "$USB_IMAGE" ::/EFI/BOOT
mcopy -i "$USB_IMAGE" dist/EFI/BOOT/BOOTX64.EFI ::/EFI/BOOT/BOOTX64.EFI

qemu-system-x86_64 \
    -machine q35 \
    -m 512M \
    -drive "if=pflash,format=raw,readonly=on,file=$EFI_CODE" \
    -drive "if=pflash,format=raw,file=$VARS_COPY" \
    -device qemu-xhci,id=xhci \
    -drive "if=none,id=usb,format=raw,file=$USB_IMAGE" \
    -device usb-storage,bus=xhci.0,drive=usb,bootindex=1 \
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

# Wait for the UI to initialize, type "admin" through emulated PS/2,
# then capture the framebuffer after the kernel has consumed the key queue.
sleep 5
{
    for key in a d m i n; do
        printf 'sendkey %s\n' "$key"
        sleep 0.2
    done
} | socat - "UNIX-CONNECT:$MONITOR" >/dev/null
sleep 1
printf 'screendump %s\nquit\n' "$SCREEN" | socat - "UNIX-CONNECT:$MONITOR" >/dev/null
wait "$QEMU_PID" || true
QEMU_PID=""

if ! python3 scripts/check-qemu-screen.py "$SCREEN"; then
    cp "$SCREEN" "$ROOT_DIR/dist/qemu-debug-screen.ppm"
    cp "$TMP_DIR/qemu.log" "$ROOT_DIR/dist/qemu-debug.log"
    printf 'Saved QEMU failure screenshot and log under dist/ for CI artifact inspection.\n' >&2
    printf 'QEMU diagnostic log follows:\n' >&2
    cat "$TMP_DIR/qemu.log" >&2
    exit 1
fi
