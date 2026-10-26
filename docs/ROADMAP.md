# Everest AI OS — Roadmap

This roadmap is milestone-based, not a promise of dates. Each phase depends on tests from the previous phase.

## M0 — Repository and design baseline
- [x] Create initial architecture and roadmap documents.
- [ ] Add Rust workspace and pinned toolchain.
- [ ] Document coding style, unsafe-code policy, license policy, and contribution workflow.
- [ ] Add CI for formatting, linting, unit tests, and build checks.

**Exit criteria:** clean checkout has a reproducible build pipeline and no undocumented generated artifacts.

## M1 — First x86-64 boot
- [ ] UEFI boot application and kernel entry.
- [ ] Serial logger and panic handler.
- [ ] Parse and validate boot information and memory map.
- [ ] Initialize framebuffer and draw an Everest boot screen.
- [ ] Build a disposable disk image and run it under QEMU/OVMF.
- [ ] Automated serial-log smoke test.

**Exit criteria:** CI can build and boot the kernel in QEMU and verify expected milestones.

## M2 — Kernel foundations
- [ ] Physical page-frame allocator.
- [ ] Virtual memory mappings and page-fault diagnostics.
- [ ] Interrupts and timer.
- [ ] Kernel heap with tests.
- [ ] Basic keyboard input in emulation.
- [ ] Panic diagnostics and debugging documentation.

**Exit criteria:** memory/interrupt tests pass without corrupting kernel state.

## M3 — Native process model
- [ ] User/kernel privilege separation.
- [ ] Native syscall ABI.
- [ ] Thread/process lifecycle and scheduler.
- [ ] Handle-based file and device access.
- [ ] IPC and permission checks.
- [ ] Run a tiny native userspace test program.

**Exit criteria:** a test process can run and exit without compromising the kernel.

## M4 — Storage and basic userspace
- [ ] Block-device abstraction and virtual storage driver.
- [ ] VFS and initial filesystem support.
- [ ] Init/service manager and logging service.
- [ ] Shell and basic file utilities.
- [ ] Image recovery and safe failure paths.

**Exit criteria:** kernel boots into a shell and can read/write a disposable filesystem image.

## M5 — AArch64 port
- [ ] UEFI entry and handoff for AArch64.
- [ ] Architecture-specific exception, interrupt, timer, and page-table setup.
- [ ] Shared kernel subsystem tests.
- [ ] QEMU AArch64 smoke test.
- [ ] Document supported emulator profiles and known gaps.

**Exit criteria:** AArch64 boots independently and passes the common kernel smoke tests. This does not imply support for all ARM64 devices.

## M6 — Native desktop foundation
- [ ] Framebuffer console and 2D rendering API.
- [ ] Font/text rendering and input routing.
- [ ] Compositor/window server.
- [ ] Minimal shell, launcher, panels, settings, and workspaces.
- [ ] Theme and keyboard customization.
- [ ] Memory and frame-time baselines.

**Exit criteria:** desktop launches reliably in the emulator and basic interactions work.

## M7 — Native developer platform
- [ ] Native application manifest and package format.
- [ ] Rust SDK and documented native ABI.
- [ ] Terminal, editor/debugging integration, logs, and crash reports.
- [ ] Package verification and rollback.
- [ ] Basic networking in the emulator.

**Exit criteria:** developers can build, install, launch, inspect, and remove a native application.

## M8 — ELF compatibility
- [ ] Hardened ELF parser and loader.
- [ ] Static native-ABI ELF support.
- [ ] PIE/relocations and dynamic linker in later substeps.
- [ ] Explicitly documented Linux ABI subset, if pursued.
- [ ] Test corpus of binaries and libraries with per-feature results.

**Exit criteria:** only documented ELF profiles are declared supported; unsupported binaries fail safely with clear diagnostics.

## M9 — PE/COFF research and compatibility prototype
- [ ] Safe PE parser and inspector.
- [ ] Architecture and subsystem validation.
- [ ] Small PE test executable using a defined Everest test ABI.
- [ ] Import resolution and relocations.
- [ ] Windows user-mode API compatibility roadmap and test harness.
- [ ] App-by-app compatibility report.

**Exit criteria:** a limited PE prototype runs a known test program. This does not mean arbitrary Windows applications or games work.

## M10 — Drivers, networking, and audio
- [ ] PCI/PCIe enumeration and selected storage/network drivers.
- [ ] USB HID and controller input.
- [ ] Networking, DNS, TCP/UDP, and TLS plan.
- [ ] Audio service and virtual-device tests.
- [ ] Hardware support matrix and regression suite.

**Exit criteria:** each claimed device works on a named test configuration.

## M11 — Native Everest AI
- [ ] User-space agent and UI.
- [ ] Typed tool broker and permission policy.
- [ ] Explicit confirmations for destructive/privileged actions.
- [ ] Sandboxed execution and audit trail.
- [ ] Model adapter with controllable network access.
- [ ] Tests for prompt-injection-like untrusted input and authorization bypass.

**Exit criteria:** the agent can perform a small set of useful, authorized actions without having unrestricted kernel authority.

## M12 — 3D graphics and gaming research
- [ ] GPU/device inventory and driver research.
- [ ] Graphics API and shader pipeline prototype.
- [ ] GPU memory management and synchronization design.
- [ ] Frame pacing, input latency, and audio latency benchmarks.
- [ ] Selected game/launcher feasibility tests.
- [ ] Per-title Epic Games and anti-cheat compatibility matrix.

**Exit criteria:** claims are limited to hardware, APIs, launchers, and titles that have been tested. Universal compatibility is not an exit criterion.

## Release gates
A release requires:
- reproducible build;
- passing CI and emulator smoke tests;
- documented limitations and supported hardware;
- artifact checksum and version;
- recovery guidance;
- no claim of full Windows, Linux, Epic, or anti-cheat compatibility without evidence.

**Important:** no milestone date is guaranteed. A custom desktop kernel and a Windows-compatible gaming environment are very different levels of effort; the latter is a long-term project.
