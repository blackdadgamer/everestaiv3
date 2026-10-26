# Everest OS — Hardware Support Matrix

Everest OS targets physical PCs from the beginning. Virtual machines are a repeatable test environment, not a substitute for real hardware.

No device is marked supported until it has been tested on physical hardware and the result is recorded here. A driver that compiles or initializes is not proof of working hardware support.

## Reference machine

**Status: not yet recorded.** Before the first physical test, record the exact machine used for development.

| Field | Value |
|---|---|
| PC or motherboard model | Not recorded |
| CPU architecture/model | Not recorded |
| RAM | Not recorded |
| UEFI firmware vendor/version | Not recorded |
| GPU/display controller | Not recorded |
| Storage controller and drive | Not recorded |
| USB host controller | Not recorded |
| Ethernet controller | Not recorded |
| Wi-Fi adapter and hardware ID | Not recorded |
| Bluetooth controller and hardware ID | Not recorded |
| Audio controller and codec | Not recorded |
| Test date / Everest commit | Not recorded |

## Initial milestone test cases

| Test | Required evidence | Status |
|---|---|---|
| UEFI removable-media boot | Firmware version, boot mode, result | Not tested |
| GOP framebuffer discovery | Resolution and pixel format | Not tested |
| Everest splash screen | Photo or repeatable screenshot | Not tested |
| Return to firmware | Observed after splash delay | Not tested |
| QEMU/OVMF boot | GitHub Actions run 40: 1280×800 screenshot detected 784 kernel-status-color pixels | Passed in CI; physical test still open |

## Later subsystem tests

| Subsystem | Test goal | Status |
|---|---|---|
| Keyboard | Key events delivered reliably | Not implemented |
| Mouse | Movement and button events delivered | Not implemented |
| Storage | Read/write and error recovery on test media | Not implemented |
| Ethernet | Link, address configuration, DNS, HTTPS | Not implemented |
| Wi-Fi | Secure association, address configuration, DNS, HTTPS | Not implemented |
| Bluetooth | Discovery, pairing, reconnect, selected profile | Not implemented |
| Audio output | Audible output and sustained playback | Not implemented |
| Microphone | Capture and stable sample delivery | Not implemented |
| GPU acceleration | Correct rendering and stability under load | Not implemented |
| Suspend/resume | Repeatable sleep/wake without device loss | Not implemented |

## Adding a supported device

For every claim, record the hardware identifier, firmware version, required firmware files, test procedure, observed result, known limitations, and the Everest commit tested. Never claim universal support based on one model.
