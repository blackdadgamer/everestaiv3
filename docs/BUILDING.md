# Building Everest OS

## Current scope

The current artifact is an x86-64 UEFI firmware bring-up application. It draws an Everest splash screen using the UEFI Graphics Output Protocol (GOP) when the firmware exposes a supported RGB/BGR framebuffer. It returns to firmware after a short delay. It does not yet enter an Everest kernel, mount storage, or provide a desktop.

## Requirements

- Rust stable toolchain via rustup.
- The x86_64-unknown-uefi Rust target.
- Git.
- A UEFI PC for physical testing.
- QEMU and OVMF for repeatable virtual-machine testing (optional but recommended).

The repository toolchain file requests the target and rustfmt/clippy components. On a new machine, install Rust from the official Rust distribution and run the build script.

## Build

From the repository root:

    ./scripts/build-uefi.sh

The script builds the release artifact and places it at:

    dist/EFI/BOOT/BOOTX64.EFI

The script does not format, partition, or write a physical disk.

## Physical PC test

1. Use a disposable USB drive whose contents can be erased.
2. Back up anything on that USB drive.
3. Format the USB as FAT32 using your normal operating-system tools.
4. Copy the contents of the generated dist directory to the USB root so the EFI file is at EFI/BOOT/BOOTX64.EFI.
5. Use the firmware's one-time boot menu to boot the USB in UEFI mode.
6. Verify the splash screen and record the PC model, firmware version, display resolution, and result.
7. Remove the USB and reboot normally.

Do not disable Secure Boot or change firmware security settings casually. If the firmware blocks the unsigned development EFI application, record the exact message and use an appropriate development test configuration only if you understand the security implications.

Never use an image-writing command against the internal system disk. There is no automated disk-writing command in this repository.

## QEMU/OVMF test

QEMU/OVMF instructions will be added when the build artifact and boot behavior have been validated on the first reference PC. A VM pass is useful for regression testing but does not prove that physical hardware is supported.

## Troubleshooting

- **No splash, but text appears:** the firmware may expose an unsupported GOP pixel format or an unsuitable framebuffer geometry.
- **USB does not appear in the boot menu:** verify that the drive is FAT32 and that the EFI file is at the standard removable-media path; confirm UEFI boot is enabled.
- **Firmware rejects the application:** development builds are unsigned. Record the firmware's message; do not download or run untrusted signing utilities.
- **Build fails:** capture the complete command output, Rust version, host OS, and target triple before changing dependency versions.

## What to record for a hardware report

- PC or motherboard make/model.
- CPU and RAM.
- Firmware vendor/version and UEFI mode.
- GPU/display path and resolution.
- USB brand/model and filesystem.
- Whether the EFI file launched and whether the splash appeared.
- Any firmware error text or hang.
