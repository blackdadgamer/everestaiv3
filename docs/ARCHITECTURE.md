# Everest AI OS — Architecture

**Status:** proposed design, not an implementation claim  
**Kernel language:** Rust-first, `no_std` kernel  
**Initial targets:** x86-64 and AArch64 (ARM64)  
**Boot target:** UEFI, first in QEMU  
**First user-visible result:** boot screen / framebuffer console, then a minimal desktop

## 1. Design goals and non-goals

### Goals
- A kernel and user-space designed for Everest rather than a Linux distribution or Windows derivative.
- A modular architecture that can grow from a small bootable kernel into a general-purpose desktop OS.
- Native development tools and APIs, with ELF and PE compatibility as separately scoped runtimes.
- A custom desktop shell and a permissioned native Everest AI agent.
- Reproducible builds, emulator-first testing, and measurable resource use.
- Architecture portability: x86-64 first for the earliest prototype, followed by AArch64; keep CPU-specific code behind interfaces.

### Non-goals for the first milestone
- Running all Linux applications or Windows EXEs.
- A production-quality GPU driver stack.
- Universal Epic Games support or guaranteed multiplayer/anti-cheat compatibility.
- Recreating every Windows API, Linux syscall, or driver interface at once.
- Shipping an installer that risks overwriting a user's drive before safety checks exist.

## 2. High-level layers

1. **Firmware and boot:** UEFI loads a small Everest boot application.
2. **Kernel:** CPU setup, physical/virtual memory, interrupts, scheduler, processes, IPC, VFS, security primitives, and driver framework.
3. **Core user space:** init/service manager, logging, device manager, storage services, network services, authentication/session manager, shell.
4. **Graphics platform:** framebuffer bring-up first; then compositor, window server, input routing, font/text rendering, and a stable 2D API.
5. **Application platform:** native Everest ABI, runtime, package format, SDK, developer tools, and app lifecycle management.
6. **Compatibility:** ELF ABI profiles and PE/COFF loading, then selected POSIX-like and Windows user-mode APIs.
7. **Gaming platform:** controller/input, audio, graphics API, GPU drivers, timing, fullscreen/presentation, launchers, and game-specific compatibility testing.
8. **Everest AI:** a user-space agent with a narrow, auditable system API and explicit permission gates.

No user-facing application, including the AI agent, should run with kernel privileges.

## 3. Bootloader and firmware

### Plan
- Use UEFI as the initial boot interface, with a small loader that can read a kernel image and a signed or checksummed boot manifest.
- Keep bootloader code separate from kernel initialization. The loader supplies a versioned boot-information structure: memory map, framebuffer, ACPI tables where available, CPU features, boot drive and entropy sources.
- Support x86-64 UEFI first. Add AArch64 UEFI as a distinct target with architecture-specific entry and page-table setup.
- Add Secure Boot signing and verified boot after image format and key management are defined.
- Provide a QEMU/OVMF workflow before testing physical USB media.

### Requirements
- The kernel must not depend on firmware boot services after the handoff unless an explicitly designed runtime service is used.
- Validate every pointer and length in boot information.
- Fail visibly and log over serial when the boot contract is invalid.
- Build architecture-specific EFI binaries; an x86-64 EFI binary is not an ARM64 EFI binary.

UEFI is an external firmware interface, not something the project can replace on ordinary PCs. The Everest bootloader and kernel can be original while implementing the published boot contract.

## 4. Kernel structure

Prefer a **modular monolithic kernel** initially: core subsystems execute in kernel mode, but maintain clean interfaces so selected drivers and services can later move to user space. A microkernel design can improve isolation but adds IPC and driver complexity; the first milestone should not combine an unproven kernel design with a new desktop and compatibility stack.

Suggested modules:
- `arch/x86_64`, `arch/aarch64`: entry, exceptions, interrupt controllers, context switch, CPU features, page tables.
- `mm`: physical frame allocator, virtual address spaces, kernel heap, page faults, mapping and unmapping.
- `sched`: threads, run queues, timers, preemption, CPU affinity, SMP later.
- `process`: address spaces, process lifecycle, handles, loader handoff.
- `ipc`: message queues, events, shared memory, synchronization.
- `syscall`: versioned native syscall ABI and validation.
- `vfs`: filesystem-neutral paths, file handles, mount points, permissions.
- `drivers`: bus enumeration and device driver interfaces.
- `security`: identity, capabilities, access checks, audit events.
- `time`: monotonic clock, wall clock, timers.
- `panic/log`: crash diagnostics and serial/framebuffer logs.

Keep `unsafe` code confined to small, reviewed hardware and memory boundaries. Use Rust types to encode ownership and invariants where practical, but do not assume Rust alone prevents all kernel bugs.

## 5. Memory management

### Initial implementation
1. Parse firmware memory map and reserve kernel, boot structures, framebuffer, and firmware-reserved ranges.
2. Implement a physical page-frame allocator with explicit ownership.
3. Set up architecture-specific virtual memory and kernel mappings.
4. Add a kernel heap allocator only after early allocations can be debugged.
5. Add page-fault diagnostics and guard regions.

### Later
- Per-process address spaces and user/kernel privilege separation.
- Demand paging, copy-on-write, memory-mapped files, shared memory, and page cache.
- NUMA awareness and large pages if benchmarks justify them.
- ASLR, W^X, stack guards, kernel address randomization where practical.

Acceptance tests must detect overlapping allocations, invalid mappings, page leaks, and faults from user processes that must not crash the kernel.

## 6. Processes, threads, and system calls

Implement a small native ABI before trying to reproduce other operating systems' interfaces:
- process and thread creation/exit;
- scheduler yield and sleep;
- file and device handles;
- read/write and memory mapping;
- timers, events, and IPC;
- signal/exception delivery or a documented equivalent;
- user identity and capability checks.

System calls must validate user pointers, lengths, handle rights, and integer overflow. Avoid exposing raw kernel pointers. Build a native command-line process and test process isolation before a graphical shell.

SMP and preemptive multitasking should follow a stable single-core kernel; add inter-processor interrupts, per-CPU data, locking rules, and race testing deliberately.

## 7. Driver model and hardware support

Drivers are a major scope constraint. Start with virtual hardware and common baseline interfaces, not a claim to support every laptop or PC.

### Order
1. Serial console and UEFI framebuffer.
2. Timer and interrupt controller.
3. Keyboard/input in QEMU; then USB HID.
4. VirtIO block and network devices for emulation.
5. PCI/PCIe enumeration and storage controllers.
6. NVMe and AHCI storage.
7. Ethernet; Wi-Fi later, device by device.
8. USB host controllers, audio devices, Bluetooth, power management.
9. GPU-specific acceleration only with a documented hardware matrix.

Use IOMMU and DMA isolation when supported before trusting high-risk devices. Firmware blobs and vendor driver licensing must be reviewed; writing the driver code from scratch does not grant redistribution rights for proprietary firmware.

## 8. Filesystems and storage

- Start with a simple read-only boot filesystem or a minimal original filesystem for early tests.
- Add a robust writable filesystem with checksums, crash recovery, and explicit on-disk versioning before storing user data.
- Add FAT support for interoperability with UEFI/removable media.
- Design VFS interfaces independent of any single filesystem.
- Later evaluate journaling, snapshots, encryption, quotas, permissions, and recovery tooling.

Never test format or image-writing operations against a user's real drive by default. Test using generated disk images in temporary files and QEMU.

## 9. Networking

Implement the network stack in layers:
- network-device interface and packet buffers;
- Ethernet, ARP/neighbor discovery;
- IPv4 and IPv6;
- UDP and TCP;
- DNS and DHCP;
- TLS using reviewed cryptographic implementations or a carefully audited implementation plan;
- sockets and user-space network APIs;
- firewall policy, connection tracking where necessary, and per-service permissions.

Network code requires fuzzing, malformed-packet tests, resource limits, and timeouts. Wi-Fi adds device drivers and authentication protocols and should not be part of the first bootable milestone.

## 10. Security model

Security is a design requirement, not a final polish phase.

- Separate kernel and user address spaces and privilege levels.
- Run services and applications as unprivileged principals by default.
- Use capabilities or explicit handle rights rather than ambient global authority.
- Authenticate package metadata and updates; verify hashes and signatures.
- Protect secrets using OS-managed storage; do not expose them to the AI model or logs by default.
- Audit privileged actions and permission changes.
- Add process sandboxing, resource limits, executable memory policy, and crash isolation.
- Require a clear user confirmation for destructive disk operations, installing privileged drivers, changing security settings, or executing high-impact AI-requested commands.
- Keep emergency recovery and safe boot paths.

## 11. Graphics, display, and desktop

### Graphics stages
1. UEFI framebuffer output and a software-rendered console.
2. 2D drawing primitives, fonts, text shaping, and input events.
3. A user-space compositor/window server with damage tracking and frame pacing.
4. Window manager and desktop shell: panels, launcher, settings, workspaces, keyboard shortcuts, themes, and accessibility.
5. Hardware acceleration through GPU-specific drivers and a stable graphics API.

Do not promise low-overhead 3D until real drivers and benchmarks exist. A portable API such as Vulkan is a useful *compatibility target and reference*, but implementing a conformant Vulkan driver requires hardware-specific support and a very large API surface. The initial desktop should use a small native 2D API with a clean path to acceleration.

### UX goals
- Dark, uncluttered default theme; configurable colors and layouts.
- Optional gaming and developer profiles that change notifications, startup services, and UI behavior—not kernel correctness.
- Keyboard-first navigation and accessibility.
- Avoid permanent animations and background effects that waste CPU/GPU time.
- Publish memory and frame-time budgets and test them on declared hardware.

## 12. ELF executable support

ELF is a container format, not a complete application contract. The loader and ABI must agree on architecture, calling convention, syscall behavior, dynamic linker, shared libraries, thread-local storage, signals, and process startup.

### Implementation sequence
1. Parse and validate ELF headers, program headers, machine type, and load ranges with overflow-safe bounds checks.
2. Support a small, static, position-dependent native ELF profile for Everest-built programs.
3. Add PIE and relocation handling.
4. Add dynamic linking and a native C/runtime library.
5. Define a documented Linux-compatible ABI subset only if compatibility with unmodified Linux binaries remains a goal.
6. Add a syscall compatibility layer only after the native ABI is stable; document each supported syscall and behavior.

Do not label the result “runs Linux apps” until a test suite of real binaries and libraries passes. Many ELF programs also require a matching userspace library stack and kernel ABI.

## 13. PE/COFF and Windows application compatibility

PE/COFF parsing is only the first stage. A useful Windows compatibility environment requires:
- PE32+ validation, architecture and subsystem checks;
- image mapping, relocations, imports/exports, TLS, exception/unwind support, and process startup;
- Windows calling conventions and runtime library expectations;
- user-mode APIs for files, registry-like configuration, processes, threads, synchronization, memory, security, UI, COM where needed, and networking;
- DLL loading and version behavior;
- graphics, audio, input, DirectX or other graphics interfaces, controller support, and timing;
- installer, launcher, DRM and anti-cheat behavior where legally and technically possible.

### Suggested compatibility tiers
- **Tier 0:** parse and inspect PE files safely; never execute untrusted code.
- **Tier 1:** run a tiny custom PE test program linked to an explicitly defined Everest PE ABI.
- **Tier 2:** run selected simple Windows console applications against an implemented API subset.
- **Tier 3:** add selected GUI and multimedia APIs with a per-API compatibility matrix.
- **Tier 4:** evaluate specific games and launchers, one title at a time, with performance and correctness reports.

An executable loader must not silently pretend that arbitrary Windows programs are supported. Windows API coverage is the major effort; kernel drivers and kernel-level anti-cheat are especially difficult and cannot be assumed to work.

## 14. Gaming, graphics, audio, input, and Epic Games

Gaming is a long-term track that depends on the kernel, drivers, and compatibility environment.

- **GPU:** display modesetting, command submission, memory management, synchronization, shader compilation, crash recovery, and vendor/device-specific drivers.
- **Graphics APIs:** establish native 2D first; later implement selected 3D APIs or a compatible implementation. Windows games often depend on Direct3D; a PE loader does not provide it.
- **Audio:** device discovery, mixing, low-latency output, device switching, and common audio interfaces.
- **Input:** USB HID, gamepads, hot-plug, remapping, vibration where supported, and stable event timing.
- **Scheduling:** measure frame-time variance and latency, avoid starving audio/input, and make performance policies observable.
- **Epic Games:** launcher sign-in, updates, manifests, installation paths, overlays, and each game's dependencies need individual validation. The store client working does not imply that its games work.
- **Anti-cheat/DRM:** compatibility is title-specific. Some systems may require kernel interfaces or platform trust properties that Everest cannot provide. Never promise universal multiplayer support.

Create a test matrix that records hardware, OS build, game version, launcher version, launch success, rendering correctness, frame-time metrics, audio/input status, online functionality, and anti-cheat result.

## 15. Native application ecosystem and developer experience

Build a native SDK around:
- a documented syscall and userspace API;
- Rust and C toolchains first;
- an application manifest and package format;
- package signing, dependency resolution, uninstall and rollback;
- a terminal, editor integration, Git tooling, debugger, profiler, logs, and crash reports;
- a reproducible build tool that produces signed packages and test images.

Docker compatibility should not be treated as a checkbox: Docker commonly depends on Linux kernel features and container APIs. First implement native process isolation, namespaces or an equivalent isolation model, filesystem layers, and resource controls; then evaluate container runtime compatibility. Virtual machines and Kubernetes are later milestones.

## 16. Native Everest AI agent

The AI agent should be a user-space system service with a desktop surface, not a privileged kernel component.

### Components
- **Agent UI:** voice/text interface, status, history controls, and permission prompts.
- **Planner:** turns user intent into proposed actions and explains expected effects.
- **Tool broker:** exposes a limited set of typed OS APIs; no arbitrary unrestricted shell by default.
- **Policy engine:** per-action permissions, confirmation for sensitive operations, and allow/deny rules.
- **Execution service:** runs approved tasks in a restricted context with timeouts and resource limits.
- **Audit trail:** records action, target, authorization, result, and errors without logging secrets.
- **Model adapter:** supports local or remote models behind a stable interface, with network access controlled by the user.

AI must not be able to silently disable security, read other users' private data, install privileged drivers, or wipe disks. Treat model output and content from files/web pages as untrusted input. Require explicit approval for destructive or privilege-escalating actions.

## 17. Build system and developer workflow

Use a Rust workspace with separate crates/modules for architecture-specific code, kernel subsystems, boot tooling, userspace, compatibility, desktop, and tests.

- Pin Rust toolchain and dependencies.
- Keep target triples and build profiles explicit for x86-64 and AArch64.
- Use reproducible image generation and record artifact hashes.
- Use QEMU with UEFI firmware for early boot tests; add AArch64 emulation separately.
- Produce serial logs and machine-readable test summaries.
- Add static checks, formatting, unit tests, fuzz tests for binary parsers, and kernel smoke tests to CI.
- Do not publish a release image if required tests fail.
- Keep third-party dependencies minimal and audit licenses before use.

## 18. Test strategy

### Unit tests
Host-test pure Rust components such as ELF/PE parsers, filesystem structures, package manifests, address-range arithmetic, and protocol parsers. Fuzz malformed input and assert that failures are safe.

### Emulator integration tests
- Boot each architecture.
- Verify expected boot milestones through serial output.
- Allocate/free memory and validate mappings.
- Start and terminate isolated test processes.
- Exercise filesystem read/write and recovery on disposable images.
- Test virtual block/network devices and timer behavior.
- Test compositor output via framebuffer screenshots where available.
- Verify that denied privileged requests are actually denied.

### Hardware tests
Start with a small, documented set of x86-64 PCs. Add ARM64 boards/devices with explicit firmware and peripheral matrices. Do not infer compatibility from one successful QEMU boot.

### Performance tests
Record boot time, idle and peak memory, CPU utilization, application launch latency, frame-time percentiles, audio latency, and power use. Compare only identical workloads and hardware; do not claim performance gains without measurements.

## 19. Key risks and mitigations

| Risk | Mitigation |
|---|---|
| Scope too large | Keep bootable kernel as the first gate; defer compatibility and gaming |
| Hardware driver coverage | Start with QEMU/virtio and publish a hardware support matrix |
| Windows compatibility gap | Implement a narrow API subset and test each app/title |
| Dual architecture slows progress | Share architecture-neutral code; make x86-64 the first release target |
| Security defects | Isolate processes, fuzz parsers, require review of unsafe code |
| Performance assumptions | Benchmark and publish repeatable test results |
| Driver/firmware licensing | Review rights before redistribution |
| Data loss during flashing | Use disposable image files in tests and require explicit physical-drive confirmation |

## 20. Definition of “first bootable prototype”

A prototype is considered bootable only when:
- a reproducible build completes from a clean checkout;
- the generated image boots in QEMU/OVMF on x86-64;
- the kernel initializes its memory map and framebuffer;
- serial logs show a versioned boot sequence;
- keyboard or timer input can be demonstrated;
- a kernel panic is diagnosable rather than silently hanging;
- an automated smoke test passes;
- the image is labeled experimental and is not presented as a complete desktop OS.

AArch64 support is a separate acceptance gate. A desktop, ELF userspace, PE compatibility, and gaming are subsequent milestones.
