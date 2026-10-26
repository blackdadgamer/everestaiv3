# Everest OS — Roadmap

This roadmap is an ordered engineering plan, not a delivery-date promise. Physical PCs are the product target from day one. QEMU/OVMF is used for repeatable tests and must not replace physical-device validation.

## M0 — Project baseline

- [x] Establish project principles and initial architecture.
- [x] Rename the project consistently to Everest OS.
- [x] Define physical-PC-first testing and safe image-writing policy.
- [x] Pin Rust 1.99.1 and create the workspace.
- [x] Commit Cargo.lock for dependency reproducibility.
- [x] Add CI for formatting and UEFI target compilation.
- [x] Document the reference-hardware test matrix.

**Exit criteria:** clean checkout has a documented build process and repeatable CI checks.

## M1 — UEFI boot and splash

- [x] Build an x86-64 UEFI application in Rust.
- [x] Discover GOP and validate framebuffer geometry/format.
- [x] Draw an Everest splash screen with a fallback diagnostic.
- [x] Add a script that creates the standard removable-media EFI path.
- [x] Boot in QEMU/OVMF (automated CI screenshot check passed).
- [ ] Boot from a disposable USB on one documented physical UEFI PC.

**Exit criteria:** repeatable EFI artifact and verified early-kernel boot on both emulated and physical hardware. The QEMU/OVMF smoke test passes in CI; physical-PC USB boot remains open.

## M2 — Kernel handoff and early diagnostics

- [x] Define the loader-to-kernel BootInfo contract.
- [x] Pass framebuffer bounds and UEFI memory-map metadata to the kernel.
- [x] Validate memory-map descriptor version, stride, bounds, and physical-region arithmetic.
- [x] Implement an early kernel entry and UEFI boot-services exit.
- [ ] Parse ACPI table pointers and firmware metadata.
- [ ] Add serial/early logging and panic diagnostics.
- [ ] Add focused tests for malformed boot information.

**Exit criteria:** kernel reaches its entry point on the reference PC and QEMU, validates boot data, and reports a verifiable milestone. QEMU execution is verified; physical-PC execution is not yet verified.

## M3 — CPU, memory and interrupt foundations

- [ ] CPU exception handling and diagnostic dumps.
- [ ] Physical page-frame allocator built from validated usable memory regions.
- [ ] Virtual memory mappings and page-fault handling.
- [ ] Kernel heap with tests.
- [ ] Timer and interrupt-controller support.
- [ ] Locking primitives and interrupt-safe rules.

**Exit criteria:** memory and interrupt tests run without corrupting kernel state.

## M4 — Input and storage

- [ ] Device discovery and driver lifecycle framework.
- [ ] Keyboard/mouse support for the documented reference configuration.
- [ ] Block-device interface and one supported storage path.
- [ ] Read-only filesystem, then a native writable filesystem.
- [ ] Safe shutdown/reboot and persistent logs.

**Exit criteria:** input and storage work on the reference PC with documented tests.

## M5 — Native processes and system services

- [ ] User/kernel privilege separation.
- [ ] Native syscall ABI and handle-based access.
- [ ] Thread/process lifecycle and scheduler.
- [ ] IPC and permission checks.
- [ ] Init/service manager and device manager.
- [ ] Run a tiny native userspace program.

**Exit criteria:** an isolated user program can start, use approved services, and exit safely.

## M6 — Graphics and desktop

- [ ] Framebuffer renderer and font rendering.
- [ ] Input event service.
- [ ] Surface/window abstraction and compositor/window manager.
- [ ] Everest-branded shell, launcher, settings, and terminal.
- [ ] Crash recovery and useful diagnostic UI.

**Exit criteria:** user can interact with a stable minimal desktop on the reference PC.

## M7 — Network and security foundation

- [ ] Network interfaces and packet-processing foundations.
- [ ] One supported wired adapter, if available on the reference PC.
- [ ] Address configuration, DNS, and TLS/HTTPS prerequisites.
- [ ] User identity, credential storage, permissions, and audit events.

**Exit criteria:** the reference system can securely reach a test endpoint with repeatable results.

## M8 — Audio, Wi-Fi and Bluetooth

- [ ] Audio device model and output service for selected hardware.
- [ ] Microphone capture where supported.
- [ ] One explicitly supported Wi-Fi chipset and secure connection flow.
- [ ] Bluetooth controller, discovery, pairing, and selected profiles.
- [ ] Hardware compatibility matrix and regression tests.

**Exit criteria:** only devices that pass real physical tests are marked supported.

## M9 — Everest integration

- [ ] Verify the actual Everest API contract and authentication.
- [ ] Native chatbot UI with connection/error states and streaming where supported.
- [ ] Permissioned action broker for a small set of safe system actions.
- [ ] No long-lived API secrets embedded in the image.
- [ ] Offline and service-unavailable behavior.

**Exit criteria:** chatbot works on a physical reference PC over a verified network connection.

## M10 — Hardening and physical-PC preview

- [ ] Boot/recovery documentation and disposable USB image workflow.
- [ ] Reproducible build and integrity checks.
- [ ] Test on more than one physical configuration.
- [ ] Security review, failure handling, and known-issues list.
- [ ] Publish a truthful developer-preview compatibility matrix.

**Exit criteria:** a documented, recoverable developer preview; not a claim of universal PC support.

## Later milestones — portability and compatibility

- AArch64 port after x86-64 interfaces stabilize.
- Native SDK, package manager, and developer tooling.
- Selected ELF ABI support and later .deb package workflows.
- Chrome as a specific compatibility acceptance target, only when the required runtime and system interfaces exist.
- Hands-free Everest voice interaction after audio and permissions are reliable.
- GPU acceleration, broader drivers, performance work, and gaming compatibility.

Windows executable compatibility and gaming are not part of the first kernel milestone. A file loader alone cannot provide the APIs, drivers, services, DRM, or anti-cheat behavior required by modern Windows applications and games.
