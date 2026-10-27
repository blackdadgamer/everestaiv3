# Everest OS

**Everest OS** is an independent operating-system project being implemented from scratch, primarily in Rust. The first target is real x86-64 PCs booting through UEFI; virtual machines are a test environment, not the product target. AArch64 support is planned after the initial x86-64 foundation stabilizes.

> **Current status:** early kernel foundations, a framebuffer desktop shell, and a first read-only FAT32 file path are implemented. CI exercises Rust formatting/tests and QEMU/OVMF boot with a separate FAT32 data disk. Physical-PC boot and hardware compatibility remain unverified.

## Project principles

- **Independent implementation:** Everest-owned kernel, drivers, system services, desktop, and native application platform. Published specifications and ordinary development libraries may be used as references/tools, but Everest is not based on an existing operating-system implementation.
- **Physical PCs first:** establish a documented reference PC and test actual UEFI boot, display, keyboard, storage, networking, and audio as each subsystem is implemented.
- **Virtual machines for repeatability:** use QEMU/OVMF to reproduce boot failures and run regression tests alongside physical-hardware tests.
- **Rust-first:** use Rust for system code, with architecture-specific assembly only where needed and documented.
- **Security by design:** isolate userspace from the kernel, use least privilege, and require permission for sensitive automated actions.
- **Evidence over claims:** a feature is supported only after a reproducible test or documented physical-device verification.

## Current desktop-shell milestone

The onboarding flow can transition into a first pixel-rendered desktop shell. The shell has a solid dark navy background, a bottom taskbar, a bordered `START` button on the left, a fixed `12:00` clock on the right, a small blue kernel-health marker, and a centered bordered `Everest Terminal` window. The terminal window displays `Everest OS ready`, the onboarding username, and the contents of `HELLO.TXT` read from a FAT32 data disk when one is present. The kernel includes a basic primary-master ATA PIO sector reader and a bounded, read-only FAT32 parser for boot-sector metadata, FAT chains, root-directory listing, and small file reads. The current driver targets legacy IDE PIO; it is not an AHCI driver and does not claim general SATA hardware support. The current shell is a framebuffer drawing, not a full window manager: taskbar controls are not yet interactive, the clock is a placeholder, and window movement/resizing and physical display support are not implemented.

The visual direction for Version 1.0 is documented in [the desktop-shell mockup](docs/desktop-shell-mockup.svg). It is a design target; the SVG includes illustrative decoration that is not necessarily present in the current kernel renderer. The QEMU screenshot test verifies the actual simpler pixel output, not every decorative mockup detail.

## First implementation milestone

The UEFI application discovers the Graphics Output Protocol (GOP), draws an Everest splash screen, records framebuffer metadata, exits UEFI boot services, and transfers control to a small kernel entry linked into the same EFI image. The kernel validates the UEFI memory map, initializes an early physical-frame allocator and page tables/IDT, handles keyboard input for onboarding, and renders the desktop shell. This remains an early kernel foundation rather than a usable general-purpose operating system: process scheduling, user space, a VFS, general device drivers, and physical-hardware validation are still outstanding. The automated QEMU/OVMF test attaches a separate FAT32 test disk containing `HELLO.TXT` and asserts that its `EVEREST DISK OK` contents appear in the framebuffer. Physical-PC boot and disk-controller compatibility remain unverified.

## Planned system layers

1. UEFI boot and kernel handoff.
2. Everest Rust kernel: memory management, interrupts, scheduling, processes, syscalls, IPC, storage abstractions, and driver framework.
3. Core services: device discovery, logging, storage, networking, audio, session management, and security.
4. Graphics and desktop: display bring-up, compositor/window manager, launcher, settings, and accessibility.
5. Native Everest application ABI and SDK.
6. Everest-branded AI/chat integration through the configured Everest service, with a permissioned system-action interface.
7. Future application compatibility work, including selected ELF and .deb workflows, only after the OS foundation is usable.
8. Future hands-free voice interaction after audio and permissions are reliable.

## Hardware policy

No universal compatibility promise is made. Initial hardware targets must be documented by model and relevant component IDs. Wi-Fi, Bluetooth, audio, GPU acceleration, sleep/resume, and firmware behavior vary between devices and require dedicated driver work and real testing.

## Repository map

- docs/ARCHITECTURE.md — architecture and subsystem boundaries.
- docs/ROADMAP.md — staged milestones and acceptance criteria.
- docs/HARDWARE.md — reference-hardware and test matrix.
- docs/BUILDING.md — toolchain and build instructions.
- boot/ — initial UEFI application and future kernel handoff.
- kernel/ — no_std kernel, memory/interrupt foundations, ATA PIO block reads, and read-only FAT32 file access.
- scripts/ — build and image preparation helpers.
- .github/workflows/ — reproducible CI checks.

## Safety and development policy

- Never write a disk image to a physical disk by default. Any future installer must require an explicit target and confirmation.
- The first USB image must be tested on a disposable USB drive; never test by overwriting a computer's internal drive.
- Do not store API keys, passwords, signing keys, or tokens in source control.
- Do not claim a driver works because it compiles or loads; verify the actual device behavior.
- Use public standards and technical documentation as references, and review third-party license requirements before distribution.

See [Architecture](docs/ARCHITECTURE.md), [Roadmap](docs/ROADMAP.md), [Hardware strategy](docs/HARDWARE.md), and [Build guide](docs/BUILDING.md).
