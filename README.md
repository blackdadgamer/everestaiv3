# Everest AI OS

**Everest AI OS** is a from-scratch operating-system research project written primarily in Rust, targeting x86-64 and AArch64 (ARM64) systems. Its long-term goal is a lightweight, customizable desktop for developers and gamers, with Everest AI integrated as a native system agent.

> **Status: planning / initial implementation not yet verified.** This repository currently contains the initial project blueprint. No bootable ISO, hardware compatibility, Windows compatibility, or gaming support is claimed until implemented and tested.

## Project principles

- **Own the core:** design and implement the kernel, user-space services, desktop shell, system APIs, and application platform as Everest components.
- **Rust-first:** use Rust for most kernel and system code, with small, documented architecture-specific assembly where required.
- **Two architectures:** support x86-64 and AArch64, with separate targets and CI jobs rather than assuming one binary runs on both.
- **Security by design:** user/kernel isolation, least privilege, capability-oriented service interfaces, signed updates, and explicit approval for privileged AI actions.
- **Performance measured, not assumed:** track boot time, idle RAM, frame pacing, process-launch latency, and power usage against published test hardware.
- **Incremental proof:** every milestone must boot or run in an emulator and have reproducible tests before expanding scope.

## Reality about executable compatibility

An executable file format is not a complete operating-system interface. ELF applications depend on an ABI, system calls, loaders, libraries, and runtime conventions. Windows PE applications additionally depend on Windows user-mode APIs, loader behavior, system services, graphics/audio/input APIs, and sometimes kernel drivers.

Everest plans to implement:
1. A native Everest application ABI and runtime.
2. An ELF loader for explicitly supported Everest ELF ABI profiles.
3. A PE/COFF parser and loader for a small, documented subset.
4. A Windows API compatibility environment, developed incrementally and separately from the kernel.
5. Graphics, audio, input, and security interfaces needed by applications.

This does **not** mean arbitrary Linux binaries or Windows games will work initially. A custom PE loader alone cannot provide Windows compatibility. Modern games, anti-cheat, GPU drivers, DRM, launchers, and Epic Games titles are long-term compatibility targets with per-title testing. The project does not promise universal compatibility or native execution of every EXE.

## First milestone

Build a minimal UEFI-bootable Rust kernel that runs in QEMU on x86-64, initializes a framebuffer and serial logging, handles a basic timer/interrupt path, and displays an Everest boot screen. Add AArch64 support after the boot protocol and architecture abstractions are stable. The first milestone is not a full desktop or gaming OS.

## Repository map

- [Architecture](docs/ARCHITECTURE.md) — subsystem boundaries and design decisions.
- [Roadmap](docs/ROADMAP.md) — staged milestones and acceptance criteria.
- `kernel/` — kernel and architecture code (to be added).
- `boot/` — boot protocol and image tooling (to be added).
- `userspace/` — system services, shell, and applications (to be added).
- `desktop/` — compositor, shell, settings, and UI (to be added).
- `compat/` — ELF and PE compatibility work (to be added).
- `tools/` — image builder, emulator scripts, and developer utilities (to be added).
- `tests/` — unit, integration, emulator, and hardware test suites (to be added).

## Development policy

- Start with QEMU/OVMF and an automated boot smoke test.
- Never write an image to a physical disk by default; image-writing tools must require an explicit target and confirmation.
- Keep early code small and reviewable. Do not add a subsystem merely to make the repository look complete.
- Record supported hardware, known limitations, and test evidence in every release.
- Use standards and public documentation as references. Review licensing and patent implications before implementing or distributing components that may be covered by third-party terms.

See the architecture and roadmap documents before opening implementation pull requests.
