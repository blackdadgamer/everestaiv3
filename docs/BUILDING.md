# Building Everest OS

## Current scope

The current artifact is an x86-64 UEFI executable containing both the firmware-stage application and an early Everest kernel entry. It draws an Everest splash screen using the UEFI Graphics Output Protocol (GOP), exits UEFI boot services, transfers control to the kernel entry, draws a kernel status screen, and halts. The kernel still relies on the firmware's existing CPU/page-table state and does not yet parse the memory map, initialize its own memory management, run device drivers, mount storage, or provide a desktop.

## Requirements

- Rust stable toolchain via rustup.
- The x86_64-unknown-uefi Rust target.
- Git.
- A UEFI PC for physical testing.
- QEMU, OVMF, dosfstools, mtools, and socat for the automated virtual-machine smoke test.

The repository toolchain file requests the target and rustfmt/clippy components. On a new machine, install Rust from the official Rust distribution and run the build script.

## Build

From the repository root:

    sh scripts/build-uefi.sh

The script builds the release artifact and places it at:

    dist/EFI/BOOT/BOOTX64.EFI

The script does not format, partition, or write a physical disk. After the QEMU/OVMF test passes, GitHub Actions also uploads an artifact named **everest-os-uefi-boot** containing the EFI application (retained for 14 days).

## Automated QEMU/OVMF smoke test

Install the test dependencies using your distribution's package manager, then run:

    sh scripts/test-qemu.sh

The script creates a temporary FAT USB image, copies the EFI application to the standard removable-media path, boots it in QEMU/OVMF, captures the virtual display, and checks that the kernel status screen appeared. Temporary images are removed on exit. This test does not write to any physical drive and does not prove support for physical hardware.

The GitHub Actions workflow runs the same smoke test on pushes and pull requests.

## Physical PC test

1. Use a disposable USB drive whose contents can be erased.
2. Back up anything on that USB drive.
3. Format the USB as FAT32 using your normal operating-system tools.
4. Copy the contents of the generated dist directory to the USB root so the EFI file is at EFI/BOOT/BOOTX64.EFI. Alternatively, download and extract the everest-os-uefi-boot artifact from a successful GitHub Actions run, create the EFI/BOOT folders on the USB, and place BOOTX64.EFI there.
5. Use the firmware's one-time boot menu to boot the USB in UEFI mode.
6. Verify both the Everest splash and the later kernel status screen. Record the PC model, firmware version, display resolution, and result.
7. Remove the USB and reboot normally.

Do not disable Secure Boot or change firmware security settings casually. If the firmware blocks the unsigned development EFI application, record the exact message and use an appropriate development test configuration only if you understand the security implications.

Never use an image-writing command against the internal system disk. There is no automated disk-writing command in this repository.

## Troubleshooting

- **No splash, but text appears:** the firmware may expose an unsupported GOP pixel format or an unsuitable framebuffer geometry.
- **USB does not appear in the boot menu:** verify that the drive is FAT32 and that the EFI file is at the standard removable-media path; confirm UEFI boot is enabled.
- **Firmware rejects the application:** development builds are unsigned. Record the firmware's message; do not download or run untrusted signing utilities.
- **Build fails:** capture the complete command output, Rust version, host OS, and target triple before changing dependency versions.
- **QEMU screenshot test fails:** inspect the workflow logs and confirm the OVMF CODE/VARS pair is available. The test is intentionally fail-closed if it cannot detect the kernel status screen.

## What to record for a hardware report

- PC or motherboard make/model.
- CPU and RAM.
- Firmware vendor/version and UEFI mode.
- GPU/display path and resolution.
- USB brand/model and filesystem.
- Whether the EFI file launched and whether both screens appeared.
- Any firmware error text or hang.
