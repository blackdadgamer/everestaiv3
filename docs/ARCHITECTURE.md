# Everest OS — Architecture

**Status:** implementation in progress; early kernel handoff code is committed  
**Implementation language:** Rust-first, with documented assembly only when required  
**First physical target:** x86-64 PC with UEFI firmware and a supported GOP framebuffer  
**Test target:** QEMU/OVMF plus physical hardware; emulation supplements physical testing  
**Additional architecture:** AArch64 after the x86-64 boot contract stabilizes

## 1. Non-negotiable principles

- Everest OS is its own operating-system implementation. It is not a Linux distribution or a Windows derivative.
- Published specifications, hardware documentation, toolchains, and general-purpose libraries may be used as references and development tools. We will not copy an existing OS implementation and rename it.
- Physical PCs are the product target from the beginning. QEMU is used to make failures reproducible and to test changes safely.
- Each supported device must be named and tested. A driver compiling or loading is not proof that the device works.
- Application compatibility is explicitly postponed until the kernel and system foundations are useful.
- The kernel and privileged services must not give an AI agent unrestricted authority. Sensitive actions require scoped permissions and appropriate user approval.

## 2. Boot stages

### Stage A — UEFI bring-up application

The Rust UEFI executable discovers the Graphics Output Protocol (GOP) and draws an Everest splash screen. It validates the framebuffer geometry and currently supports RGB/BGR pixel layouts. If the display path is unsupported, it emits a firmware-console diagnostic and returns to firmware.

### Stage B — early kernel handoff (implemented, not hardware-verified)

The EFI image contains an early no_std kernel entry. The boot application creates a framebuffer-only BootInfo structure, releases its GOP protocol handle, exits UEFI boot services, and calls the kernel entry. The kernel draws a status screen, masks normal interrupts, and halts. The UEFI memory-map owner remains alive, but the kernel does not yet parse or use the map for allocation.

This is a real handoff into code that no longer calls UEFI boot services, but it is not yet a complete kernel. It still uses the firmware's existing CPU state and page-table mapping.

### Stage C — kernel foundations

Next, extend the boot contract to carry a parsed memory map, ACPI table pointers where available, and firmware metadata. Add early diagnostics, exception handling, and safe memory-map validation. The kernel must not rely on firmware services after handoff.

## 3. Kernel architecture

The initial recommendation is a modular monolithic kernel. Keep subsystem boundaries explicit so components can be isolated or moved into user space later.

Planned kernel modules include architecture-specific CPU setup, memory management, interrupts, scheduler, processes, syscalls, IPC, VFS, driver lifecycle, time/logging, and security. Do not create empty modules merely to imply a feature exists. Introduce each subsystem when its first tested implementation is ready.

## 4. Hardware support strategy

Start with a documented reference x86-64 PC and a QEMU virtual machine. Record firmware version, CPU, RAM, GPU, storage controller, network adapters, Wi-Fi/Bluetooth IDs, audio controller, and input devices. Test physical USB boot before attempting installation.

Order of implementation:

1. UEFI boot, framebuffer output, early logging.
2. CPU exceptions, memory map, physical page allocator, virtual memory.
3. Timer and interrupt infrastructure.
4. Keyboard/mouse input and basic USB host-controller support.
5. Storage access and a read-only test filesystem, then writable storage.
6. Networking foundation and one explicitly supported wired adapter.
7. Audio framework and one documented audio device.
8. Wi-Fi and Bluetooth drivers, firmware loading, security, and user-facing configuration.
9. Accelerated graphics and broader device coverage.

Hardware and firmware vary widely. Wi-Fi, Bluetooth, GPU acceleration, audio, suspend/resume, and power management are not assumed to work universally. Use documented, redistributable firmware only after licensing and redistribution requirements are reviewed.

## 5. Graphics and desktop

The UEFI framebuffer is only a bring-up mechanism. It is not a GPU driver.

Planned progression: early framebuffer renderer; input events and display abstraction; font/text rendering and surfaces; then a compositor/window manager, launcher, settings, notifications, and accessibility. Display-driver and acceleration work follows once kernel memory and device interfaces can support it safely.

The visual direction is premium and restrained: dark surfaces, subtle violet/blue accents, legible typography, and animations that do not compromise responsiveness.

## 6. Native application platform

Everest will define its own native application ABI, executable format/profile, system library, package metadata, and permission model. Native programs must not have unrestricted access to kernel memory or hardware.

ELF execution and .deb package handling are later compatibility projects, not requirements for the initial boot milestone. A package installer does not provide the Linux ABI or missing libraries an application needs. Chrome compatibility will be claimed only after real launch, rendering, networking, audio, and stability tests.

## 7. Everest brand and AI service

Everest OS is the product name. Everest is the brand; AI is not part of the OS name.

The eventual AI/chat application will communicate with the configured Everest service API. Before integration, verify the API endpoint, authentication scheme, streaming format, error behavior, and usage limits. Do not hard-code secrets into the OS or commit credentials. A server-issued, appropriately scoped session credential is preferable to embedding a long-lived privileged secret.

AI system actions must use a narrow broker interface. The AI service may request an action, but the OS independently validates permissions and asks the user before sensitive operations. Hands-free operation is a later milestone after microphone, audio output, networking, and the permission system are reliable.

## 8. Build, test and release discipline

- Pin the Rust toolchain and document required targets.
- Keep host tools separate from firmware/kernel artifacts.
- CI must format and compile all committed code.
- Add emulator smoke tests as soon as the boot image can be launched automatically.
- Test on physical hardware before claiming support.
- Never automatically write an image to a physical disk. Image-writing must require a clearly named target and explicit confirmation.
- Keep test evidence and known limitations alongside each milestone.

## 9. First boot milestone acceptance criteria

The first boot milestone is complete only when the x86-64 EFI executable builds reproducibly, a script places it at EFI/BOOT/BOOTX64.EFI, and it boots in QEMU/OVMF and on at least one physical reference PC. On supported GOP hardware, the splash and early-kernel status screens must appear. The exact test machines, firmware settings, and limitations must be documented.

The current code includes an early kernel handoff but does not yet establish its own page tables, parse the memory map, install an IDT, allocate memory, run processes, or provide device drivers. Physical and emulator boot verification remains outstanding.
