#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$ROOT_DIR"

for command_name in qemu-system-x86_64 mkfs.vfat mmd mcopy mtype socat python3; do
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
DATA_IMAGE="$TMP_DIR/everest-data.img"
VARS_COPY="$TMP_DIR/OVMF_VARS.fd"
cp "$EFI_VARS" "$VARS_COPY"

truncate -s 64M "$USB_IMAGE"
mkfs.vfat -F 32 "$USB_IMAGE" >/dev/null
mmd -i "$USB_IMAGE" ::/EFI
mmd -i "$USB_IMAGE" ::/EFI/BOOT
mcopy -i "$USB_IMAGE" dist/EFI/BOOT/BOOTX64.EFI ::/EFI/BOOT/BOOTX64.EFI

# A persistent FAT32 disk on the primary IDE master. HELLO.TXT proves reads;
# CONFIG.TXT is created by the OS and must survive a complete QEMU reboot.
truncate -s 64M "$DATA_IMAGE"
mkfs.vfat -F 32 "$DATA_IMAGE" >/dev/null
printf 'EVEREST DISK OK\n' > "$TMP_DIR/HELLO.TXT"
mcopy -i "$DATA_IMAGE" "$TMP_DIR/HELLO.TXT" ::HELLO.TXT
printf 'SECOND FILE\n' > "$TMP_DIR/SECOND.TXT"
mcopy -i "$DATA_IMAGE" "$TMP_DIR/SECOND.TXT" ::SECOND.TXT

run_qemu() {
    boot_number="$1"
    SCREEN="$TMP_DIR/everest-screen-$boot_number.ppm"
    MONITOR="$TMP_DIR/qemu-monitor-$boot_number.sock"
    QEMU_LOG="$TMP_DIR/qemu-$boot_number.log"
    qemu-system-x86_64 \
        -machine pc \
        -m 512M \
        -drive "if=pflash,format=raw,readonly=on,file=$EFI_CODE" \
        -drive "if=pflash,format=raw,file=$VARS_COPY" \
        -device qemu-xhci,id=xhci \
        -drive "if=none,id=usb,format=raw,file=$USB_IMAGE" \
        -device usb-storage,bus=xhci.0,drive=usb,bootindex=1 \
        -drive "if=ide,index=0,media=disk,format=raw,file=$DATA_IMAGE" \
        -display none \
        -monitor "unix:$MONITOR,server,nowait" \
        -serial none \
        -net none \
        -no-reboot \
        -no-shutdown >"$QEMU_LOG" 2>&1 &
    QEMU_PID=$!

    attempt=0
    while [ ! -S "$MONITOR" ] && [ "$attempt" -lt 100 ]; do
        sleep 0.1
        attempt=$((attempt + 1))
    done
    if [ ! -S "$MONITOR" ]; then
        cat "$QEMU_LOG" >&2
        printf 'QEMU monitor socket did not appear on boot %s.\n' "$boot_number" >&2
        exit 1
    fi

    sleep 5
    if ! kill -0 "$QEMU_PID" >/dev/null 2>&1; then
        cat "$QEMU_LOG" >&2
        printf "QEMU exited before accepting monitor commands on boot %s.\\n" "$boot_number" >&2
        exit 1
    fi
    if [ "$boot_number" = "1" ]; then
        # Enter username "admin", accept UTC, move to NPT, then finish setup.
        {
            for key in a d m i n; do
                printf 'sendkey %s\n' "$key"
                sleep 0.2
            done
            printf 'sendkey ret\n'
            sleep 0.3
            printf 'sendkey down\n'
            sleep 0.3
            printf 'sendkey ret\n'
            sleep 0.4
            printf 'sendkey ret\n'
        } | socat - "UNIX-CONNECT:$MONITOR" >/dev/null
        sleep 1
    else
        # Saved config bypasses setup. Move down once to test live selection.
        sleep 2
        { printf 'sendkey down\n'; sleep 0.5; } | socat - "UNIX-CONNECT:$MONITOR" >/dev/null
    fi

    printf 'screendump %s\nquit\n' "$SCREEN" | socat - "UNIX-CONNECT:$MONITOR" >/dev/null
    wait "$QEMU_PID" || true
    QEMU_PID=""

    if ! python3 scripts/check-qemu-screen.py "$SCREEN" --file-manager --selected-index "$([ "$boot_number" = "1" ] && printf 0 || printf 1)" --username admin --timezone NPT; then
        cp "$SCREEN" "$ROOT_DIR/dist/qemu-debug-screen-$boot_number.ppm"
        cp "$QEMU_LOG" "$ROOT_DIR/dist/qemu-debug-$boot_number.log"
        printf 'Saved QEMU failure screenshot and log under dist/ for CI artifact inspection.\n' >&2
        cat "$QEMU_LOG" >&2
        exit 1
    fi
}

run_qemu 1

# Inspect the actual on-disk bytes after the first VM has exited.
if ! mtype -i "$DATA_IMAGE" ::CONFIG.TXT > "$TMP_DIR/CONFIG.TXT"; then
    printf 'CONFIG.TXT was not written to the FAT32 disk.\n' >&2
    exit 1
fi
printf 'USERNAME=admin\nTIMEZONE=NPT\n' > "$TMP_DIR/expected-config.txt"
if ! cmp -s "$TMP_DIR/expected-config.txt" "$TMP_DIR/CONFIG.TXT"; then
    printf 'CONFIG.TXT contents did not match the onboarding selections.\n' >&2
    printf 'Actual config:\n' >&2
    cat "$TMP_DIR/CONFIG.TXT" >&2
    exit 1
fi

# Reboot with the exact same writable data-disk image and no onboarding input.
run_qemu 2
printf 'Persistent configuration and File Manager UI passed: saved config survived reboot and Down moved selection.\n'
