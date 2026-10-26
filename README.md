# Everest OS

**Everest OS** is an independent operating-system project being implemented from scratch, primarily in Rust. The first target is real x86-64 PCs booting through UEFI; virtual machines are a test environment, not the product target. AArch64 support is planned after the initial x86-64 foundation stabilizes.

> **Current status:** first implementation milestone. The repository contains the project design and the initial UEFI firmware bring-up application. This is not yet a standalone kernel, desktop OS, or release-ready boot image. Hardware support is unverified until tested on named physical machines.

## Project principles

- **Independent implementation:** Everest-owned kernel, drivers, system services, desktop, and native application platform. Published specifications and ordinary development libraries may be used as references/tools, but Everest is not based on an existing operating-system implementation.
- **Physical PCs first:** establish a documented reference PC and test actual UEFI boot, display, keyboard, storage, networking, and audio as each subsystem is implemented.
- **Virtual machines for repeatability:** use QEMU/OVMF to reproduce boot failures and run regression tests alongside physical-hardware tests.
- **Rust-first:** use Rust for system code, with architecture-specific assembly only where needed and documented.
- **Security by design:** isolate userspace from the kernel, use least privilege, and require permission for sensitive automated actions.
- **Evidence over claims:** a feature is supported only after a reproducible test or documented physical-device verification.

## First milestone

The first implementation step is a small UEFI application that can be built as an x86-64 EFI executable and copied to the standard removable-media path. It establishes the build pipeline and framebuffer bring-up on UEFI systems. This is a firmware-stage bring-up program, **not yet the Everest kernel**. The next milestone will establish the kernel handoff and boot information contract.

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
- kernel/ — kernel implementation, to be introduced after the boot contract.
- scripts/ — build and image preparation helpers.
- .github/workflows/ — reproducible CI checks.

## Safety and development policy

- Never write a disk image to a physical disk by default. Any future installer must require an explicit target and confirmation.
- The first USB image must be tested on a disposable USB drive; never test by overwriting a computer's internal drive.
- Do not store API keys, passwords, signing keys, or tokens in source control.
- Do not claim a driver works because it compiles or loads; verify the actual device behavior.
- Use public standards and technical documentation as references, and review third-party license requirements before distribution.

See [Architecture](docs/ARCHITECTURE.md), [Roadmap](docs/ROADMAP.md), [Hardware strategy](docs/HARDWARE.md), and [Build guide](docs/BUILDING.md).
