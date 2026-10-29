#!/usr/bin/env sh
set -eu
ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$ROOT_DIR"
command -v xorriso >/dev/null 2>&1 || { echo "xorriso is required" >&2; exit 2; }
command -v mkfs.vfat >/dev/null 2>&1
command -v mmd >/dev/null 2>&1
command -v mcopy >/dev/null 2>&1
EFI_FILE="$ROOT_DIR/dist/EFI/BOOT/BOOTX64.EFI"
test -s "$EFI_FILE"
ISO_ROOT="$ROOT_DIR/dist/iso-root"
ESP="$ROOT_DIR/dist/efiboot.img"
mkdir -p "$ISO_ROOT/EFI/BOOT" "$ISO_ROOT/boot"
cp "$EFI_FILE" "$ISO_ROOT/EFI/BOOT/BOOTX64.EFI"
truncate -s 16M "$ESP"
mkfs.vfat -F 32 "$ESP" >/dev/null
mmd -i "$ESP" ::/EFI
mmd -i "$ESP" ::/EFI/BOOT
mcopy -i "$ESP" "$EFI_FILE" ::/EFI/BOOT/BOOTX64.EFI
cp "$ESP" "$ISO_ROOT/boot/efiboot.img"
xorriso -as mkisofs -R -J -V EVEREST_OS -o "$ROOT_DIR/dist/everest-os.iso" \
  -e boot/efiboot.img -no-emul-boot -isohybrid-gpt-basdat "$ISO_ROOT"
test -s "$ROOT_DIR/dist/everest-os.iso"
file "$ROOT_DIR/dist/everest-os.iso"
