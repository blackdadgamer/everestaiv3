#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$ROOT_DIR"

rustup target add x86_64-unknown-uefi
cargo build --release --target x86_64-unknown-uefi -p everest-boot

OUT_DIR="$ROOT_DIR/dist"
EFI_DIR="$OUT_DIR/EFI/BOOT"
mkdir -p "$EFI_DIR"
cp "$ROOT_DIR/target/x86_64-unknown-uefi/release/everest-boot.efi" "$EFI_DIR/BOOTX64.EFI"

printf '\nEverest OS UEFI application built.\n'
printf 'Removable-media EFI file: %s\n' "$EFI_DIR/BOOTX64.EFI"
printf 'This is a UEFI bring-up application, not yet a standalone kernel.\n'
printf 'No physical disk was written. Copy dist/ to a disposable FAT32 USB manually.\n'
