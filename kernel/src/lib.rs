#![no_std]

pub mod arch;
pub mod memory;
pub mod interrupts;
pub mod heap;
pub mod filesystem;

use core::{mem, ptr};
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    Rgb = 0,
    Bgr = 1,
}

/// Shared boot contract populated by the UEFI loader.
///
/// The memory-map buffer is owned by the loader's MemoryMapOwned value. The
/// loader must keep that owner alive for the full duration of kernel_main.
#[repr(C)]
pub struct BootInfo {
    pub framebuffer: *mut u8,
    pub framebuffer_size: usize,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub pixel_format: PixelFormat,
    pub memory_map: *const u8,
    pub memory_map_size: usize,
    pub memory_descriptor_size: usize,
    pub memory_descriptor_version: u32,
    /// Physical base and byte length of the loaded EFI image containing the kernel.
    pub kernel_image_start: u64,
    pub kernel_image_size: u64,
}

/// UEFI x86-64 MemoryDescriptor v1 layout.
///
/// Do not walk descriptors using size_of alone: firmware may report a larger
/// descriptor stride. The stride from BootInfo is used for every entry.
#[repr(C)]
#[derive(Clone, Copy)]
struct UefiMemoryDescriptor {
    memory_type: u32,
    _padding: u32,
    physical_start: u64,
    _virtual_start: u64,
    page_count: u64,
    _attributes: u64,
}

const UEFI_MEMORY_DESCRIPTOR_VERSION: u32 = 1;
const UEFI_PAGE_SIZE: u64 = 4096;
const MAX_MEMORY_MAP_BYTES: usize = 64 * 1024 * 1024;
const USERNAME_CAPACITY: usize = 24;
const CONFIG_FILE: &[u8; 11] = b"CONFIG  TXT";
static USERNAME: [AtomicU8; USERNAME_CAPACITY] =
    [const { AtomicU8::new(0) }; USERNAME_CAPACITY];
static USERNAME_LEN: AtomicUsize = AtomicUsize::new(0);
static ONBOARDING_STEP: AtomicU8 = AtomicU8::new(0);
static TIMEZONE_SELECTION: AtomicU8 = AtomicU8::new(0);
const DISK_TEXT_CAPACITY: usize = 96;
static DISK_TEXT: [AtomicU8; DISK_TEXT_CAPACITY] = [const { AtomicU8::new(0) }; DISK_TEXT_CAPACITY];
static DISK_TEXT_LEN: AtomicUsize = AtomicUsize::new(0);
static DISK_TEXT_OK: AtomicU8 = AtomicU8::new(0);
const TERMINAL_CAPACITY: usize = 96;
static TERMINAL_INPUT: [AtomicU8; TERMINAL_CAPACITY] = [const { AtomicU8::new(0) }; TERMINAL_CAPACITY];
static TERMINAL_LEN: AtomicUsize = AtomicUsize::new(0);
static TERMINAL_SAVED: AtomicU8 = AtomicU8::new(0);

/// Read HELLO.TXT from the FAT32 root directory, keeping the UI path bounded.
fn load_demo_file() {
    let result = (|| {
        let volume = filesystem::open_volume().ok()?;
        let (cluster, size) = volume.find_root_file(b"HELLO   TXT").ok()?;
        if size as usize > DISK_TEXT_CAPACITY { return None; }
        let mut bytes = [0u8; DISK_TEXT_CAPACITY];
        let read = volume.read_file(cluster, size, &mut bytes).ok()?;
        Some((bytes, read))
    })();
    if let Some((bytes, length)) = result {
        for (i, slot) in DISK_TEXT.iter().enumerate() {
            slot.store(if i < length && (32..=126).contains(&bytes[i]) { bytes[i] } else { 0 }, Ordering::Relaxed);
        }
        DISK_TEXT_LEN.store(length, Ordering::Relaxed);
        DISK_TEXT_OK.store(1, Ordering::Relaxed);
    } else {
        DISK_TEXT_LEN.store(0, Ordering::Relaxed);
        DISK_TEXT_OK.store(0, Ordering::Relaxed);
    }
}

/// Entry reached only after the UEFI boot services have been exited.
///
/// Validates boot data, creates independent page tables, installs exception handlers, then halts.
pub fn kernel_main(info: &BootInfo) -> ! {
    unsafe {
        // No IDT or interrupt-controller setup exists yet. Do not allow maskable
        // interrupts to arrive before the kernel installs its own handlers.
        core::arch::asm!("cli", options(nomem, nostack));
    }

    let memory_map_valid = validate_memory_map(info);
    let mut usable = [memory::MemoryRegion::EMPTY; 256];
    let mut free_storage = [memory::MemoryRegion::EMPTY; 512];
    let mut managed_storage = [memory::MemoryRegion::EMPTY; 512];
    let mut allocator = if memory_map_valid {
        let parsed = unsafe {
            memory::parse_usable_regions(
                info.memory_map, info.memory_map_size, info.memory_descriptor_size,
                info.memory_descriptor_version, &mut usable,
            )
        };
        parsed.ok().and_then(|count| {
            let boot_info_start = (info as *const BootInfo) as usize as u64;
            let map_start = info.memory_map as usize as u64;
            let reserved = [
                memory::MemoryRegion::new(info.kernel_image_start,
                    info.kernel_image_start.checked_add(info.kernel_image_size).unwrap_or(info.kernel_image_start)),
                memory::MemoryRegion::new(boot_info_start,
                    boot_info_start.checked_add(mem::size_of::<BootInfo>() as u64).unwrap_or(boot_info_start)),
                memory::MemoryRegion::new(map_start,
                    map_start.checked_add(info.memory_map_size as u64).unwrap_or(map_start)),
            ];
            memory::FrameAllocator::new(&usable[..count], &reserved, &mut free_storage, &mut managed_storage).ok()
        })
    } else { None };
    let allocator_ready = allocator.as_ref().is_some_and(|a| a.free_frame_count() > 0);
    let virtual_memory_ready = if let Some(ref mut allocator) = allocator {
        // SAFETY: maskable interrupts are disabled and allocator metadata plus
        // all required boot data remain live for the non-returning kernel entry.
        unsafe { arch::initialize(info, allocator).is_ok() }
    } else {
        false
    };
    if let Some(ref mut allocator) = allocator { heap::initialize(allocator); }
    load_demo_file();
    load_configuration();
    CURSOR_X.store(info.width / 2, Ordering::Relaxed);
    CURSOR_Y.store(info.height / 2, Ordering::Relaxed);
    draw_kernel_status(info, memory_map_valid, allocator_ready, virtual_memory_ready);
    draw_onboarding_screen(info);
    if virtual_memory_ready {
        unsafe { interrupts::initialize(); }
        unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    }

    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
        if virtual_memory_ready {
            update_onboarding(info);
        }
    }
}

const INPUT_BG: (u8, u8, u8) = (18, 26, 48);
const UI_WHITE: (u8, u8, u8) = (255, 255, 255);
const UI_ACCENT: (u8, u8, u8) = (92, 145, 220);

fn draw_border(info: &BootInfo, x: usize, y: usize, width: usize, height: usize, thickness: usize, color: (u8, u8, u8)) {
    if width == 0 || height == 0 || thickness == 0 {
        return;
    }
    let edge = thickness.min(width).min(height);
    fill_rect(info, x, y, width, edge, color);
    fill_rect(info, x, y.saturating_add(height.saturating_sub(edge)), width, edge, color);
    fill_rect(info, x, y, edge, height, color);
    fill_rect(info, x.saturating_add(width.saturating_sub(edge)), y, edge, height, color);
}

fn draw_onboarding_screen(info: &BootInfo) {
    // Keep the kernel's page-table/IDT status marker below the onboarding panel
    // so the existing boot-health screenshot assertion remains independent.
    fill_rect(info, 40, 70, 720, 290, (5, 8, 24));

    match ONBOARDING_STEP.load(Ordering::Relaxed) {
        0 => {
            draw_text(info, 80, 96, 3, b"Welcome to Everest OS", UI_WHITE);
            draw_text(info, 80, 164, 2, b"Username", (160, 190, 230));
            draw_border(info, 80, 188, 480, 56, 3, UI_ACCENT);
            fill_rect(info, 83, 191, 474, 50, INPUT_BG);
            let length = USERNAME_LEN.load(Ordering::Relaxed).min(USERNAME_CAPACITY);
            let mut text = [0u8; USERNAME_CAPACITY];
            for (index, byte) in text.iter_mut().enumerate().take(length) {
                *byte = USERNAME[index].load(Ordering::Relaxed);
            }
            draw_text(info, 96, 205, 3, &text[..length], UI_WHITE);
        }
        1 => {
            draw_text(info, 80, 96, 3, b"Timezone Selection", UI_WHITE);
            let selected = TIMEZONE_SELECTION.load(Ordering::Relaxed).min(2) as usize;
            let options: [&[u8]; 3] = [b"UTC", b"NPT", b"EST"];
            for (index, option) in options.iter().enumerate() {
                let row_y = 168 + index * 50;
                if index == selected {
                    draw_border(info, 80, row_y, 360, 42, 3, UI_ACCENT);
                }
                draw_text(info, 112, row_y + 10, 3, option, UI_WHITE);
            }
            draw_text(info, 80, 322, 2, b"Use Up or Down, then Enter", (160, 190, 230));
        }
        2 => {
            draw_text(info, 80, 96, 3, b"Setup Complete", UI_WHITE);
            draw_text(info, 80, 170, 2, b"Username", (160, 190, 230));
            draw_text(info, 80, 220, 2, b"Timezone", (160, 190, 230));
            let timezone: &[u8] = match TIMEZONE_SELECTION.load(Ordering::Relaxed) {
                1 => b"NPT",
                2 => b"EST",
                _ => b"UTC",
            };
            let length = USERNAME_LEN.load(Ordering::Relaxed).min(USERNAME_CAPACITY);
            let mut username = [0u8; USERNAME_CAPACITY];
            for (index, byte) in username.iter_mut().enumerate().take(length) {
                *byte = USERNAME[index].load(Ordering::Relaxed);
            }
            // Draw the actual saved username, not a test-specific constant.
            fill_rect(info, 205, 164, 160, 18, (5, 8, 24));
            draw_text(info, 210, 170, 2, &username[..length], UI_WHITE);
            draw_text(info, 210, 220, 2, timezone, UI_WHITE);
            draw_border(info, 70, 280, 360, 50, 3, UI_ACCENT);
            draw_text(info, 90, 296, 2, b"Press Enter to Start", UI_WHITE);
        }
        _ => draw_desktop(info),
    }
}

const DESKTOP_BG: (u8, u8, u8) = (26, 27, 38); // Tokyo Night #1a1b26
const TASKBAR_BG: (u8, u8, u8) = (22, 24, 35);
const WINDOW_BG: (u8, u8, u8) = (26, 27, 38);
const TITLEBAR_BG: (u8, u8, u8) = (36, 40, 59);
const FILE_SELECTION: (u8, u8, u8) = (187, 154, 247); // #bb9af7
const FILE_TEXT: (u8, u8, u8) = (192, 202, 245);
const FILE_SELECTED_TEXT: (u8, u8, u8) = (26, 27, 38);
static FILE_SELECTION_INDEX: AtomicUsize = AtomicUsize::new(0);
static WINDOW_X: AtomicUsize = AtomicUsize::new(usize::MAX);
static WINDOW_Y: AtomicUsize = AtomicUsize::new(usize::MAX);
static CURSOR_X: AtomicUsize = AtomicUsize::new(0);
static CURSOR_Y: AtomicUsize = AtomicUsize::new(0);
static MOUSE_BUTTONS: AtomicU8 = AtomicU8::new(0);
static DRAGGING_WINDOW: AtomicU8 = AtomicU8::new(0);
static DRAG_OFFSET_X: AtomicUsize = AtomicUsize::new(0);
static DRAG_OFFSET_Y: AtomicUsize = AtomicUsize::new(0);
static START_MENU_OPEN: AtomicU8 = AtomicU8::new(0);
static APP_WINDOW_COUNT: AtomicUsize = AtomicUsize::new(0);
static APP_WINDOW_KIND: [AtomicU8; 8] = [const { AtomicU8::new(0) }; 8];
static APP_WINDOW_ID: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];
static APP_ID_CLOCK: AtomicUsize = AtomicUsize::new(1);
static ACTIVE_APP_WINDOW: AtomicUsize = AtomicUsize::new(usize::MAX);
static APP_WINDOW_X: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];
static APP_WINDOW_Y: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];
static APP_WINDOW_Z: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];
static APP_WINDOW_W: [AtomicUsize; 8] = [const { AtomicUsize::new(520) }; 8];
static APP_WINDOW_H: [AtomicUsize; 8] = [const { AtomicUsize::new(300) }; 8];
static APP_DRAG_INDEX: AtomicUsize = AtomicUsize::new(usize::MAX);
static APP_Z_CLOCK: AtomicUsize = AtomicUsize::new(1);
const CALCULATOR_CAPACITY: usize = 24;
static CALCULATOR_DISPLAY: [AtomicU8; CALCULATOR_CAPACITY] =
    [const { AtomicU8::new(0) }; CALCULATOR_CAPACITY];
static CALCULATOR_LEN: AtomicUsize = AtomicUsize::new(0);
static CALCULATOR_RESULT_OK: AtomicU8 = AtomicU8::new(0);
const EDITOR_CAPACITY: usize = 96;
static EDITOR_BUFFER: [AtomicU8; EDITOR_CAPACITY * 8] = [const { AtomicU8::new(0) }; EDITOR_CAPACITY * 8];
static EDITOR_LEN: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];
static EDITOR_SAVED: [AtomicU8; 8] = [const { AtomicU8::new(0) }; 8];
static VIDEO_PLAYING: AtomicU8 = AtomicU8::new(0);
static VIDEO_FRAME: AtomicUsize = AtomicUsize::new(0);

fn spawn_app_window(info: &BootInfo, kind: u8) {
    let count = APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8);
    let slot = if count < 8 { count } else {
        let mut oldest = 0usize;
        for i in 1..8 { if APP_WINDOW_Z[i].load(Ordering::Relaxed) < APP_WINDOW_Z[oldest].load(Ordering::Relaxed) { oldest = i; } }
        oldest
    };
    APP_WINDOW_KIND[slot].store(kind, Ordering::Relaxed);
    APP_WINDOW_ID[slot].store(APP_ID_CLOCK.fetch_add(1, Ordering::Relaxed), Ordering::Relaxed);
    ACTIVE_APP_WINDOW.store(slot, Ordering::Relaxed);
    if kind == 5 {
        let base = slot * EDITOR_CAPACITY;
        for byte in &EDITOR_BUFFER[base..base + EDITOR_CAPACITY] {
            byte.store(0, Ordering::Relaxed);
        }
        EDITOR_LEN[slot].store(0, Ordering::Relaxed);
        EDITOR_SAVED[slot].store(0, Ordering::Relaxed);
    }
    let editor_already_open = (0..count).any(|i| APP_WINDOW_KIND[i].load(Ordering::Relaxed) == 5);
    let (window_x, window_y) = if kind == 5 && editor_already_open {
        (info.width / 2 + 40, info.height.saturating_sub(336) / 2)
    } else if kind == 4 && editor_already_open {
        (info.width / 2 + 40, info.height.saturating_sub(336) / 2)
    } else if kind == 4 || kind == 5 {
        (info.width.saturating_sub(520) / 2, info.height.saturating_sub(336) / 2)
    } else {
        (180 + (slot * 44), 100 + (slot * 36))
    };
    APP_WINDOW_X[slot].store(window_x, Ordering::Relaxed);
    APP_WINDOW_Y[slot].store(window_y, Ordering::Relaxed);
    APP_WINDOW_W[slot].store(520, Ordering::Relaxed);
    APP_WINDOW_H[slot].store(300, Ordering::Relaxed);
    APP_WINDOW_Z[slot].store(APP_Z_CLOCK.fetch_add(1, Ordering::Relaxed), Ordering::Relaxed);
    if count < 8 { APP_WINDOW_COUNT.store(count + 1, Ordering::Relaxed); }
    START_MENU_OPEN.store(0, Ordering::Relaxed);
    draw_desktop(info);
}

fn draw_app_windows(info: &BootInfo) {
    let count = APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8);
    let mut drawn = [false; 8];
    for _ in 0..count {
        let mut next = None;
        let mut z = usize::MAX;
        for i in 0..count {
            let zi = APP_WINDOW_Z[i].load(Ordering::Relaxed);
            if !drawn[i] && zi < z { z = zi; next = Some(i); }
        }
        let Some(i) = next else { break; };
        drawn[i] = true;
        let kind = APP_WINDOW_KIND[i].load(Ordering::Relaxed);
        let x = APP_WINDOW_X[i].load(Ordering::Relaxed);
        let y = APP_WINDOW_Y[i].load(Ordering::Relaxed);
        let w = APP_WINDOW_W[i].load(Ordering::Relaxed).min(info.width.saturating_sub(x));
        let h = APP_WINDOW_H[i].load(Ordering::Relaxed).min(info.height.saturating_sub(y + 36));
        if w < 180 || h < 100 { continue; }
        draw_border(info, x, y, w, h, 2, FILE_SELECTION);
        fill_rect(info, x + 2, y + 2, w - 4, h - 4, WINDOW_BG);
        fill_rect(info, x + 2, y + 2, w - 4, 42, TITLEBAR_BG);
        let title: &[u8] = match kind { 1 => b"TERMINAL", 2 => b"FILE MANAGER", 3 => b"SETTINGS", 4 => b"CALCULATOR", 5 => b"TEXT EDITOR", _ => b"VIDEO PLAYER" };
        draw_text(info, x + 18, y + 15, 2, title, UI_WHITE);
        if kind == 1 {
            // Stable framebuffer marker used by the headless launcher regression test.
            fill_rect(info, x + 8, y + 48, 8, 8, (80, 200, 160));
        }
        if kind == 4 {
            draw_calculator(info, x, y, w, h);
        } else if kind == 5 {
            draw_text_editor(info, i, x, y, w, h);
        } else if kind == 3 {
            draw_text(info, x + 24, y + 90, 2, b"Everest OS v1.0", FILE_TEXT);
            draw_text(info, x + 24, y + 140, 2, b"Created by Amit Pathak", FILE_TEXT);
        } else if kind == 6 {
            draw_video_player(info, x, y, w, h);
        } else if kind == 1 {
            draw_text(info, x + 24, y + 80, 2, b"Everest Terminal", FILE_TEXT);
            draw_text(info, x + 24, y + 124, 1, b"Ready for commands", UI_WHITE);
        } else {
            draw_text(info, x + 24, y + 80, 2, b"FILE MANAGER", FILE_TEXT);
            draw_text(info, x + 24, y + 124, 1, b"Browse your files", FILE_TEXT);
        }
    }
}

fn draw_video_player(info: &BootInfo, x: usize, y: usize, w: usize, h: usize) {
    let frame = VIDEO_FRAME.load(Ordering::Relaxed);
    let playing = VIDEO_PLAYING.load(Ordering::Relaxed) != 0;
    let vx = x + 18;
    let vy = y + 54;
    let vw = w.saturating_sub(36);
    let vh = h.saturating_sub(116).min(190);
    fill_rect(info, vx, vy, vw, vh, (5, 8, 24));
    let bar_width = (vw / 8).max(1);
    let colors = [(239, 68, 68), (249, 115, 22), (234, 179, 8), (34, 197, 94), (6, 182, 212), (59, 130, 246), (139, 92, 246), (236, 72, 153)];
    for bar in 0..8usize {
        let phase = (bar + frame / 2) % 8;
        let width = if bar == 7 { vw.saturating_sub(bar * bar_width) } else { bar_width };
        fill_rect(info, vx + bar * bar_width, vy, width, vh, colors[phase]);
    }
    let marker_x = vx + 12 + (frame % vw.saturating_sub(24).max(1));
    fill_rect(info, marker_x, vy + 8, 4, vh.saturating_sub(16), UI_WHITE);
    let control_y = vy + vh + 12;
    fill_rect(info, vx, control_y, 112, 32, (49, 57, 82));
    draw_border(info, vx, control_y, 112, 32, 1, FILE_SELECTION);
    draw_text(info, vx + 12, control_y + 10, 1, if playing { b"PAUSE [P]" } else { b"PLAY [P]" }, UI_WHITE);
    fill_rect(info, vx + 124, control_y, 92, 32, (49, 57, 82));
    draw_border(info, vx + 124, control_y, 92, 32, 1, FILE_SELECTION);
    draw_text(info, vx + 140, control_y + 10, 1, b"STOP [S]", UI_WHITE);
    draw_text(info, vx, control_y + 42, 1, if playing { b"PLAYING - RGB TEST PATTERN" } else { b"PAUSED - RGB TEST PATTERN" }, FILE_TEXT);
}

fn handle_video_player_click(info: &BootInfo, x: usize, y: usize) -> bool {
    let count = APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8);
    for i in (0..count).rev() {
        if APP_WINDOW_KIND[i].load(Ordering::Relaxed) != 6 { continue; }
        let wx = APP_WINDOW_X[i].load(Ordering::Relaxed);
        let wy = APP_WINDOW_Y[i].load(Ordering::Relaxed);
        if x >= wx + 18 && x < wx + 130 && y >= wy + 256 && y < wy + 300 {
            VIDEO_PLAYING.store(if VIDEO_PLAYING.load(Ordering::Relaxed) == 0 { 1 } else { 0 }, Ordering::Relaxed);
            draw_desktop(info);
            return true;
        }
        if x >= wx + 142 && x < wx + 234 && y >= wy + 256 && y < wy + 300 {
            VIDEO_PLAYING.store(0, Ordering::Relaxed);
            VIDEO_FRAME.store(0, Ordering::Relaxed);
            draw_desktop(info);
            return true;
        }
    }
    false
}

fn draw_calculator(info: &BootInfo, x: usize, y: usize, w: usize, _h: usize) {
    // Tokyo Night display panel and crisp button grid.
    fill_rect(info, x + 18, y + 54, w.saturating_sub(36), 42, (16, 18, 27));
    draw_border(info, x + 18, y + 54, w.saturating_sub(36), 42, 1, (65, 72, 97));
    let len = CALCULATOR_LEN.load(Ordering::Relaxed).min(CALCULATOR_CAPACITY);
    if len == 0 {
        draw_text(info, x + 30, y + 68, 2, b"0", FILE_TEXT);
    } else {
        let mut display = [0u8; CALCULATOR_CAPACITY];
        for (i, byte) in display.iter_mut().enumerate().take(len) {
            *byte = CALCULATOR_DISPLAY[i].load(Ordering::Relaxed);
        }
        draw_text(info, x + 30, y + 68, 2, &display[..len], UI_WHITE);
    }
    if len > 0 {
        // Stable indicator that a mouse click changed the calculator display.
        fill_rect(info, x + w.saturating_sub(30), y + 62, 8, 8, (80, 200, 160));
    }
    if CALCULATOR_RESULT_OK.load(Ordering::Relaxed) != 0 {
        fill_rect(info, x + w.saturating_sub(44), y + 62, 8, 8, (122, 162, 247));
    }
    let keys: [[u8; 4]; 4] = [
        [b'7', b'8', b'9', b'/'],
        [b'4', b'5', b'6', b'*'],
        [b'1', b'2', b'3', b'-'],
        [b'C', b'0', b'=', b'+'],
    ];
    let button_w = 108usize;
    let button_h = 36usize;
    for (row, line) in keys.iter().enumerate() {
        for (col, key) in line.iter().enumerate() {
            let bx = x + 20 + col * 118;
            let by = y + 108 + row * 42;
            let operator = matches!(*key, b'/' | b'*' | b'-' | b'+' | b'=');
            let bg = if operator { (49, 57, 82) } else if *key == b'C' { (61, 43, 61) } else { (31, 34, 48) };
            let edge = if operator { (122, 162, 247) } else if *key == b'C' { (247, 118, 142) } else { (65, 72, 97) };
            fill_rect(info, bx, by, button_w, button_h, bg);
            draw_border(info, bx, by, button_w, button_h, 1, edge);
            let label = [*key];
            draw_text(info, bx + 50, by + 13, 1, &label, if operator { (122, 162, 247) } else { FILE_TEXT });
        }
    }
}

fn handle_calculator_click(info: &BootInfo, x: usize, y: usize) -> bool {
    let count = APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8);
    // Test from front to back so overlapping app windows route to the top window.
    for i in (0..count).rev() {
        if APP_WINDOW_KIND[i].load(Ordering::Relaxed) != 4 { continue; }
        let wx = APP_WINDOW_X[i].load(Ordering::Relaxed);
        let wy = APP_WINDOW_Y[i].load(Ordering::Relaxed);
        if x < wx + 20 || x >= wx + 20 + 4 * 118 - 10 || y < wy + 108 || y >= wy + 108 + 4 * 42 - 6 {
            continue;
        }
        let col = (x - (wx + 20)) / 118;
        let row = (y - (wy + 108)) / 42;
        if col >= 4 || row >= 4 { return false; }
        let key = [[b'7',b'8',b'9',b'/'],[b'4',b'5',b'6',b'*'],[b'1',b'2',b'3',b'-'],[b'C',b'0',b'=',b'+']][row][col];
        if key == b'C' {
            for slot in CALCULATOR_DISPLAY.iter() { slot.store(0, Ordering::Relaxed); }
            CALCULATOR_LEN.store(0, Ordering::Relaxed);
            CALCULATOR_RESULT_OK.store(0, Ordering::Relaxed);
        } else if key == b'=' {
            let len = CALCULATOR_LEN.load(Ordering::Relaxed).min(CALCULATOR_CAPACITY);
            let mut bytes = [0u8; CALCULATOR_CAPACITY];
            for (j, byte) in bytes.iter_mut().enumerate().take(len) { *byte = CALCULATOR_DISPLAY[j].load(Ordering::Relaxed); }
            if let Some(op) = bytes[..len].iter().position(|b| *b == b'+') {
                let left = parse_calculator_number(&bytes[..op]);
                let right = parse_calculator_number(&bytes[op + 1..len]);
                if let (Some(a), Some(b)) = (left, right) {
                    let sum = a.saturating_add(b);
                    let mut out = [0u8; CALCULATOR_CAPACITY];
                    let mut n = 0usize;
                    let mut value = sum;
                    if value == 0 { out[n] = b'0'; n += 1; }
                    while value > 0 && n < CALCULATOR_CAPACITY {
                        out[n] = b'0' + (value % 10) as u8;
                        value /= 10;
                        n += 1;
                    }
                    for j in 0..n { CALCULATOR_DISPLAY[j].store(out[n - 1 - j], Ordering::Relaxed); }
                    CALCULATOR_LEN.store(n, Ordering::Relaxed);
                    CALCULATOR_RESULT_OK.store(1, Ordering::Relaxed);
                }
            }
        } else {
            CALCULATOR_RESULT_OK.store(0, Ordering::Relaxed);
            let len = CALCULATOR_LEN.load(Ordering::Relaxed).min(CALCULATOR_CAPACITY);
            if len < CALCULATOR_CAPACITY {
                CALCULATOR_DISPLAY[len].store(key, Ordering::Relaxed);
                CALCULATOR_LEN.store(len + 1, Ordering::Relaxed);
            }
        }
        draw_desktop(info);
        return true;
    }
    false
}


fn draw_text_editor(info: &BootInfo, index: usize, x: usize, y: usize, w: usize, _h: usize) {
    // Stable per-instance marker used by the QEMU screenshot regression test.
    fill_rect(info, x + 8, y + 48, 8, 8, (80, 200, 160));
    fill_rect(info, x + 18, y + 54, w.saturating_sub(36), 174, (16, 18, 27));
    draw_border(info, x + 18, y + 54, w.saturating_sub(36), 174, 1, (65, 72, 97));
    draw_text(info, x + 28, y + 62, 1, b"NOTES.TXT", (122, 162, 247));
    let id = APP_WINDOW_ID[index].load(Ordering::Relaxed);
    let id_label = [b'I', b'D', b' ', b'0' + (id % 10) as u8];
    draw_text(info, x + w.saturating_sub(96), y + 15, 1, &id_label, UI_WHITE);
    let len = EDITOR_LEN[index].load(Ordering::Relaxed).min(EDITOR_CAPACITY);
    let mut bytes = [0u8; EDITOR_CAPACITY];
    let base = index * EDITOR_CAPACITY;
    for (i, byte) in bytes.iter_mut().enumerate().take(len) { *byte = EDITOR_BUFFER[base + i].load(Ordering::Relaxed); }
    for row in 0..3 {
        let start = row * 30;
        if start >= len { break; }
        let end = (start + 30).min(len);
        draw_text(info, x + 28, y + 88 + row * 36, 1, &bytes[start..end], FILE_TEXT);
    }
    fill_rect(info, x + 380, y + 244, 112, 34, (49, 57, 82));
    draw_border(info, x + 380, y + 244, 112, 34, 1, (122, 162, 247));
    draw_text(info, x + 410, y + 256, 1, b"SAVE", UI_WHITE);
    if EDITOR_SAVED[index].load(Ordering::Relaxed) != 0 {
        fill_rect(info, x + 24, y + 244, 8, 8, (80, 200, 160));
        draw_text(info, x + 42, y + 250, 1, b"SAVED TO DISK", (80, 200, 160));
    }
}

fn handle_text_editor_key(key: u8) -> bool {
    let index = ACTIVE_APP_WINDOW.load(Ordering::Relaxed);
    if index >= APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8)
        || APP_WINDOW_KIND[index].load(Ordering::Relaxed) != 5 { return false; }
    let base = index * EDITOR_CAPACITY;
    let len = EDITOR_LEN[index].load(Ordering::Relaxed).min(EDITOR_CAPACITY);
    if key == 8 || key == 127 {
        if len > 0 {
            EDITOR_BUFFER[base + len - 1].store(0, Ordering::Relaxed);
            EDITOR_LEN[index].store(len - 1, Ordering::Relaxed);
            EDITOR_SAVED[index].store(0, Ordering::Relaxed);
        }
    } else if (32..=126).contains(&key) && len < EDITOR_CAPACITY {
        let previous = if len == 0 { b' ' } else { EDITOR_BUFFER[base + len - 1].load(Ordering::Relaxed) };
        let stored = if key.is_ascii_lowercase() && previous == b' ' { key.to_ascii_uppercase() } else { key };
        EDITOR_BUFFER[base + len].store(stored, Ordering::Relaxed);
        EDITOR_LEN[index].store(len + 1, Ordering::Relaxed);
        EDITOR_SAVED[index].store(0, Ordering::Relaxed);
    }
    true
}

fn handle_text_editor_click(info: &BootInfo, x: usize, y: usize) -> bool {
    let count = APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8);
    let mut target = None;
    let mut target_z = 0usize;
    for i in 0..count {
        if APP_WINDOW_KIND[i].load(Ordering::Relaxed) != 5 { continue; }
        let wx = APP_WINDOW_X[i].load(Ordering::Relaxed);
        let wy = APP_WINDOW_Y[i].load(Ordering::Relaxed);
        if x >= wx + 380 && x < wx + 492 && y >= wy + 244 && y < wy + 278 {
            let z = APP_WINDOW_Z[i].load(Ordering::Relaxed);
            if target.is_none() || z >= target_z { target = Some(i); target_z = z; }
        }
    }
    let Some(index) = target else { return false; };
    ACTIVE_APP_WINDOW.store(index, Ordering::Relaxed);
    let len = EDITOR_LEN[index].load(Ordering::Relaxed).min(EDITOR_CAPACITY);
    let base = index * EDITOR_CAPACITY;
    let mut bytes = [0u8; EDITOR_CAPACITY];
    for (j, byte) in bytes.iter_mut().enumerate().take(len) { *byte = EDITOR_BUFFER[base + j].load(Ordering::Relaxed); }
    let saved = filesystem::open_volume()
        .and_then(|volume| volume.write_root_file(b"NOTES   TXT", &bytes[..len]))
        .is_ok();
    EDITOR_SAVED[index].store(saved as u8, Ordering::Relaxed);
    draw_desktop(info);
    true
}

fn parse_calculator_number(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() { return None; }
    let mut value = 0usize;
    for byte in bytes {
        if !byte.is_ascii_digit() { return None; }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as usize)?;
    }
    Some(value)
}

fn draw_start_menu(info: &BootInfo) {
    if START_MENU_OPEN.load(Ordering::Relaxed) == 0 { return; }
    let y = info.height.saturating_sub(36);
    fill_rect(info, 12, y.saturating_sub(266), 220, 262, TITLEBAR_BG);
    draw_border(info, 12, y.saturating_sub(266), 220, 262, 2, FILE_SELECTION);
    draw_text(info, 28, y.saturating_sub(248), 2, b"START MENU", UI_WHITE);
    draw_text(info, 32, y.saturating_sub(180), 2, b"VIDEO PLAYER", (80, 200, 160));
    draw_text(info, 32, y.saturating_sub(146), 2, b"TERMINAL", FILE_TEXT);
    draw_text(info, 32, y.saturating_sub(112), 2, b"FILE MANAGER", FILE_TEXT);
    draw_text(info, 32, y.saturating_sub(78), 2, b"SETTINGS", FILE_TEXT);
    draw_text(info, 32, y.saturating_sub(44), 2, b"CALCULATOR", (122, 162, 247));
    draw_text(info, 32, y.saturating_sub(10), 2, b"TEXT EDITOR", FILE_TEXT);
}


fn draw_desktop(info: &BootInfo) {
    fill_rect(info, 0, 0, info.width, info.height, DESKTOP_BG);
    let window_width = info.width.saturating_mul(3) / 4;
    let window_height = info.height.saturating_mul(3) / 4;
    let centered_x = info.width.saturating_sub(window_width) / 2;
    let centered_y = info.height.saturating_sub(window_height) / 2;
    let stored_x = WINDOW_X.load(Ordering::Relaxed);
    let stored_y = WINDOW_Y.load(Ordering::Relaxed);
    let window_x = if stored_x == usize::MAX { centered_x } else { stored_x.min(info.width.saturating_sub(window_width)) };
    let window_y = if stored_y == usize::MAX { centered_y } else { stored_y.min(info.height.saturating_sub(window_height + 36)) };
    WINDOW_X.store(window_x, Ordering::Relaxed);
    WINDOW_Y.store(window_y, Ordering::Relaxed);
    let titlebar_height = 46usize.min(window_height);
    draw_border(info, window_x, window_y, window_width, window_height, 2, FILE_SELECTION);
    fill_rect(info, window_x + 2, window_y + 2, window_width.saturating_sub(4), window_height.saturating_sub(4), WINDOW_BG);
    fill_rect(info, window_x + 2, window_y + 2, window_width.saturating_sub(4), titlebar_height.saturating_sub(2), TITLEBAR_BG);
    draw_text(info, window_x + 22, window_y + 15, 2, b"FILE MANAGER", UI_WHITE);
    draw_text(info, window_x + window_width.saturating_sub(140), window_y + 17, 1, b"EVEREST OS", FILE_TEXT);

    draw_text(info, window_x + 24, window_y + 62, 1, b"USER:", FILE_TEXT);
    let username_len = USERNAME_LEN.load(Ordering::Relaxed).min(USERNAME_CAPACITY);
    let mut username = [0u8; USERNAME_CAPACITY];
    for (i, byte) in username.iter_mut().enumerate().take(username_len) { *byte = USERNAME[i].load(Ordering::Relaxed); }
    draw_text(info, window_x + 60, window_y + 62, 1, &username[..username_len], UI_WHITE);
    draw_text(info, window_x + window_width.saturating_sub(104), window_y + 62, 1, b"TIMEZONE:", FILE_TEXT);
    let timezone: &[u8] = match TIMEZONE_SELECTION.load(Ordering::Relaxed).min(2) { 1 => b"NPT", 2 => b"EST", _ => b"UTC" };
    draw_text(info, window_x + window_width.saturating_sub(50), window_y + 62, 1, timezone, UI_WHITE);

    draw_border(info, window_x + 16, window_y + 82, window_width.saturating_sub(32), window_height.saturating_sub(142), 1, (65, 72, 97));
    draw_text(info, window_x + 32, window_y + 92, 1, b"NAME", (122, 162, 247));
    draw_text(info, window_x + window_width.saturating_sub(160), window_y + 92, 1, b"TYPE", (122, 162, 247));

    let mut names = [[0u8; 11]; 32];
    let mut count = 0usize;
    if let Ok(volume) = filesystem::open_volume() { let _ = volume.list_root_files(&mut names, &mut count); }
    let selected = FILE_SELECTION_INDEX.load(Ordering::Relaxed).min(count.saturating_sub(1));
    let visible = 6usize.min(count);
    for index in 0..visible {
        let row_y = window_y + 112 + index * 38;
        if index == selected {
            fill_rect(info, window_x + 20, row_y - 4, window_width.saturating_sub(40), 30, FILE_SELECTION);
        } else if index % 2 == 1 {
            fill_rect(info, window_x + 20, row_y - 4, window_width.saturating_sub(40), 30, (31, 34, 48));
        }
        let mut name = [0u8; 13];
        let mut out = 0usize;
        for byte in &names[index][..8] {
            if *byte == b' ' { break; }
            name[out] = if (32..=126).contains(byte) { *byte } else { b'?' }; out += 1;
        }
        if names[index][8..11].iter().any(|b| *b != b' ') {
            name[out] = b'.'; out += 1;
            for byte in &names[index][8..11] {
                if *byte == b' ' { break; }
                name[out] = if (32..=126).contains(byte) { *byte } else { b'?' }; out += 1;
            }
        }
        let color = if index == selected { FILE_SELECTED_TEXT } else { FILE_TEXT };
        draw_text(info, window_x + 36, row_y + 3, 2, &name[..out], color);
        draw_text(info, window_x + window_width.saturating_sub(150), row_y + 5, 1,
            if names[index][8..11].iter().any(|b| *b != b' ') { b"FILE" } else { b"ITEM" }, color);
    }

    // A compact terminal pane shares the desktop window with the root listing.
    let terminal_y = window_y + window_height.saturating_sub(112);
    fill_rect(info, window_x + 20, terminal_y, window_width.saturating_sub(40), 58, (16, 18, 27));
    draw_text(info, window_x + 28, terminal_y + 6, 1, b"TERMINAL  C:/USER.TXT", (122, 162, 247));
    draw_text(info, window_x + 28, terminal_y + 24, 2, b">", FILE_SELECTION);
    let terminal_len = TERMINAL_LEN.load(Ordering::Relaxed).min(TERMINAL_CAPACITY);
    let mut terminal_line = [0u8; TERMINAL_CAPACITY];
    for (i, byte) in terminal_line.iter_mut().enumerate().take(terminal_len) { *byte = TERMINAL_INPUT[i].load(Ordering::Relaxed); }
    draw_text(info, window_x + 46, terminal_y + 24, 2, &terminal_line[..terminal_len], UI_WHITE);
    if TERMINAL_SAVED.load(Ordering::Relaxed) != 0 {
        draw_text(info, window_x + 260, terminal_y + 24, 1, b"File Saved Successfully", (118, 222, 190));
    }

    let status_y = window_y + window_height.saturating_sub(44);
    fill_rect(info, window_x + 3, status_y, window_width.saturating_sub(6), 39, TITLEBAR_BG);
    draw_border(info, window_x + 3, status_y, window_width.saturating_sub(6), 1, 1, FILE_SELECTION);
    draw_text(info, window_x + 24, status_y + 14, 1, b"C:/", UI_WHITE);
    draw_text(info, window_x + 90, status_y + 14, 1, b"FILES:", FILE_TEXT);
    let value = count.min(999);
    let mut digits = [b'0'; 3];
    let digit_count = if value >= 100 {
        digits = [b'0' + (value / 100) as u8, b'0' + ((value / 10) % 10) as u8, b'0' + (value % 10) as u8]; 3
    } else if value >= 10 {
        digits[0] = b'0' + (value / 10) as u8; digits[1] = b'0' + (value % 10) as u8; 2
    } else { digits[0] = b'0' + value as u8; 1 };
    draw_text(info, window_x + 132, status_y + 14, 1, &digits[3 - digit_count..], UI_WHITE);

    let taskbar_height = 36usize.min(info.height);
    let taskbar_y = info.height.saturating_sub(taskbar_height);
    fill_rect(info, 0, taskbar_y, info.width, taskbar_height, TASKBAR_BG);
    fill_rect(info, 8, taskbar_y + 4, 104, 28, TITLEBAR_BG);
    draw_border(info, 8, taskbar_y + 4, 104, 28, 1, FILE_SELECTION);
    draw_text(info, 22, taskbar_y + 12, 1, b"START", UI_WHITE);
    draw_text(info, info.width.saturating_sub(58), taskbar_y + 12, 1, b"12:00", FILE_TEXT);
    if info.width >= 180 { fill_rect(info, info.width - 160, taskbar_y + 16, 4, 4, (110, 190, 255)); }
    // Render app windows above the desktop/taskbar, then the launcher popup above
    // both. Keep the cursor last so it remains visible over interactive surfaces.
    draw_app_windows(info);
    draw_start_menu(info);
    draw_mouse_cursor(info, CURSOR_X.load(Ordering::Relaxed), CURSOR_Y.load(Ordering::Relaxed));
}

fn draw_mouse_cursor(info: &BootInfo, x: usize, y: usize) {
    const ARROW: [u16; 16] = [
        0b100000000000, 0b110000000000, 0b111000000000, 0b111100000000,
        0b111110000000, 0b111111000000, 0b111111100000, 0b111111110000,
        0b111111111000, 0b111111111100, 0b111111000000, 0b110111100000,
        0b100011110000, 0b000001110000, 0b000001110000, 0b000000100000,
    ];
    for (row, bits) in ARROW.iter().enumerate() {
        for col in 0..12 {
            if bits & (1 << (11 - col)) != 0 {
                put_pixel(info, x.saturating_add(col), y.saturating_add(row), (10, 10, 14));
                if col > 0 && row > 0 && ARROW[row - 1] & (1 << (12 - col)) != 0 {
                    put_pixel(info, x.saturating_add(col), y.saturating_add(row), (245, 245, 255));
                }
            }
        }
    }
}

fn activate_app_window_at(x: usize, y: usize) {
    let count = APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8);
    let mut hit = None;
    let mut top_z = 0usize;
    for i in 0..count {
        let ax = APP_WINDOW_X[i].load(Ordering::Relaxed);
        let ay = APP_WINDOW_Y[i].load(Ordering::Relaxed);
        let aw = APP_WINDOW_W[i].load(Ordering::Relaxed);
        let ah = APP_WINDOW_H[i].load(Ordering::Relaxed);
        if x >= ax && x < ax + aw && y >= ay && y < ay + ah + 36 {
            let z = APP_WINDOW_Z[i].load(Ordering::Relaxed);
            if hit.is_none() || z >= top_z { hit = Some(i); top_z = z; }
        }
    }
    if let Some(index) = hit {
        ACTIVE_APP_WINDOW.store(index, Ordering::Relaxed);
        APP_WINDOW_Z[index].store(APP_Z_CLOCK.fetch_add(1, Ordering::Relaxed), Ordering::Relaxed);
    }
}

fn process_mouse_events(info: &BootInfo) -> bool {
    if ONBOARDING_STEP.load(Ordering::Relaxed) < 3 { return false; }
    let mut changed = false;
    while let Some(event) = interrupts::pop_mouse_event() {
        let x = CURSOR_X.load(Ordering::Relaxed).saturating_add_signed(event.dx as isize).min(info.width.saturating_sub(1));
        let y = CURSOR_Y.load(Ordering::Relaxed).saturating_add_signed(event.dy as isize).min(info.height.saturating_sub(1));
        CURSOR_X.store(x, Ordering::Relaxed);
        CURSOR_Y.store(y, Ordering::Relaxed);
        let previous = MOUSE_BUTTONS.swap(event.buttons, Ordering::Relaxed);
        let left_now = event.buttons & 1 != 0;
        let left_before = previous & 1 != 0;
        let window_width = info.width.saturating_mul(3) / 4;
        let window_height = info.height.saturating_mul(3) / 4;
        let wx = WINDOW_X.load(Ordering::Relaxed).min(info.width.saturating_sub(window_width));
        let wy = WINDOW_Y.load(Ordering::Relaxed).min(info.height.saturating_sub(window_height + 36));
        if left_now && !left_before {
            activate_app_window_at(x, y);
            let taskbar_y = info.height.saturating_sub(36);
            if x >= 8 && x <= 112 && y >= taskbar_y + 4 && y < taskbar_y + 32 {
                START_MENU_OPEN.store(if START_MENU_OPEN.load(Ordering::Relaxed) == 0 { 1 } else { 0 }, Ordering::Relaxed);
            } else if START_MENU_OPEN.load(Ordering::Relaxed) != 0 && x >= 12 && x < 232 && y >= taskbar_y.saturating_sub(266) && y < taskbar_y {
                let row = y.saturating_sub(taskbar_y.saturating_sub(197)) / 34;
                match row { 0 => { VIDEO_PLAYING.store(1, Ordering::Relaxed); spawn_app_window(info, 6); }, 1 => spawn_app_window(info, 1), 2 => spawn_app_window(info, 2), 3 => spawn_app_window(info, 3), 4 => spawn_app_window(info, 4), _ => spawn_app_window(info, 5) }
            } else if handle_video_player_click(info, x, y) {
                // Video Player transport controls consumed the click.
            } else if handle_video_player_click(info, x, y) {
                // Video Player transport controls consumed the click.
            } else if handle_text_editor_click(info, x, y) {
                // Text Editor Save button consumed the click.
            } else if handle_calculator_click(info, x, y) {
                // Calculator button consumed the click.
            } else {
                let count = APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8);
                let mut hit = None;
                let mut top_z = 0usize;
                for i in 0..count {
                    let ax = APP_WINDOW_X[i].load(Ordering::Relaxed);
                    let ay = APP_WINDOW_Y[i].load(Ordering::Relaxed);
                    let aw = APP_WINDOW_W[i].load(Ordering::Relaxed);
                    if x >= ax && x < ax + aw && y >= ay && y < ay + 44 {
                        let z = APP_WINDOW_Z[i].load(Ordering::Relaxed);
                        if hit.is_none() || z >= top_z { hit = Some(i); top_z = z; }
                    }
                }
                if let Some(i) = hit {
                    APP_DRAG_INDEX.store(i, Ordering::Relaxed);
                    DRAG_OFFSET_X.store(x.saturating_sub(APP_WINDOW_X[i].load(Ordering::Relaxed)), Ordering::Relaxed);
                    DRAG_OFFSET_Y.store(y.saturating_sub(APP_WINDOW_Y[i].load(Ordering::Relaxed)), Ordering::Relaxed);
                    APP_WINDOW_Z[i].store(APP_Z_CLOCK.fetch_add(1, Ordering::Relaxed), Ordering::Relaxed);
                } else if x >= wx && x < wx + window_width && y >= wy && y < wy + 48 {
                    DRAG_OFFSET_X.store(x.saturating_sub(wx), Ordering::Relaxed);
                    DRAG_OFFSET_Y.store(y.saturating_sub(wy), Ordering::Relaxed);
                    DRAGGING_WINDOW.store(1, Ordering::Relaxed);
                } else if x >= wx + 20 && x < wx + window_width.saturating_sub(20)
                    && y >= wy + 108 && y < wy + 108 + 6 * 38 {
                    let row = y.saturating_sub(wy + 108) / 38;
                    let count = filesystem::open_volume().ok().map(|volume| {
                        let mut names = [[0u8; 11]; 32]; let mut count = 0usize;
                        let _ = volume.list_root_files(&mut names, &mut count); count
                    }).unwrap_or(0);
                    FILE_SELECTION_INDEX.store(row.min(count.saturating_sub(1)), Ordering::Relaxed);
                }
            }
        }
        if left_now && !left_before {
            let count = APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8);
            for i in 0..count {
                let ax = APP_WINDOW_X[i].load(Ordering::Relaxed);
                let ay = APP_WINDOW_Y[i].load(Ordering::Relaxed);
                if x >= ax && x < ax + 520 && y >= ay && y < ay + 44 {
                    APP_WINDOW_Z[i].store(APP_Z_CLOCK.fetch_add(1, Ordering::Relaxed), Ordering::Relaxed);
                }
            }
        }
        let app_drag = APP_DRAG_INDEX.load(Ordering::Relaxed);
        if left_now && app_drag < APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8) {
            let nx = x.saturating_sub(DRAG_OFFSET_X.load(Ordering::Relaxed));
            let ny = y.saturating_sub(DRAG_OFFSET_Y.load(Ordering::Relaxed));
            APP_WINDOW_X[app_drag].store(nx.min(info.width.saturating_sub(APP_WINDOW_W[app_drag].load(Ordering::Relaxed))), Ordering::Relaxed);
            APP_WINDOW_Y[app_drag].store(ny.min(info.height.saturating_sub(APP_WINDOW_H[app_drag].load(Ordering::Relaxed) + 36)), Ordering::Relaxed);
        } else if left_now && DRAGGING_WINDOW.load(Ordering::Relaxed) != 0 {
            let nx = x.saturating_sub(DRAG_OFFSET_X.load(Ordering::Relaxed)).min(info.width.saturating_sub(window_width));
            let ny = y.saturating_sub(DRAG_OFFSET_Y.load(Ordering::Relaxed)).min(info.height.saturating_sub(window_height + 36));
            WINDOW_X.store(nx, Ordering::Relaxed);
            WINDOW_Y.store(ny, Ordering::Relaxed);
        }
        if !left_now {
            DRAGGING_WINDOW.store(0, Ordering::Relaxed);
            if app_drag < APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8) {
                let edge = 18usize;
                let aw = APP_WINDOW_W[app_drag].load(Ordering::Relaxed);
                let ah = APP_WINDOW_H[app_drag].load(Ordering::Relaxed);
                if x <= edge {
                    APP_WINDOW_X[app_drag].store(0, Ordering::Relaxed);
                    APP_WINDOW_Y[app_drag].store(0, Ordering::Relaxed);
                    APP_WINDOW_W[app_drag].store(info.width / 2, Ordering::Relaxed);
                    APP_WINDOW_H[app_drag].store(info.height.saturating_sub(36), Ordering::Relaxed);
                } else if x >= info.width.saturating_sub(edge) {
                    APP_WINDOW_X[app_drag].store(info.width / 2, Ordering::Relaxed);
                    APP_WINDOW_Y[app_drag].store(0, Ordering::Relaxed);
                    APP_WINDOW_W[app_drag].store(info.width.saturating_sub(info.width / 2), Ordering::Relaxed);
                    APP_WINDOW_H[app_drag].store(info.height.saturating_sub(36), Ordering::Relaxed);
                } else if y <= edge {
                    APP_WINDOW_X[app_drag].store(0, Ordering::Relaxed);
                    APP_WINDOW_Y[app_drag].store(0, Ordering::Relaxed);
                    APP_WINDOW_W[app_drag].store(info.width, Ordering::Relaxed);
                    APP_WINDOW_H[app_drag].store(info.height.saturating_sub(36), Ordering::Relaxed);
                } else {
                    let _ = (aw, ah);
                }
                APP_DRAG_INDEX.store(usize::MAX, Ordering::Relaxed);
            }
        }
        changed = true;
    }
    if changed { draw_desktop(info); }
    changed
}

fn load_configuration() {
    let Ok(volume) = filesystem::open_volume() else { return; };
    let Ok((cluster, size)) = volume.find_root_file(CONFIG_FILE) else { return; };
    let mut bytes = [0u8; 128];
    let Ok(length) = volume.read_file(cluster, size, &mut bytes) else { return; };
    let data = &bytes[..length];
    let mut username = None;
    let mut timezone = None;
    for line in data.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(value) = line.strip_prefix(b"USERNAME=") { username = Some(value); }
        if let Some(value) = line.strip_prefix(b"TIMEZONE=") { timezone = Some(value); }
    }
    let Some(saved_name) = username else { return; };
    let Some(saved_zone) = timezone else { return; };
    if saved_name.is_empty() || saved_name.len() > USERNAME_CAPACITY
        || !saved_name.iter().all(|b| (32..=126).contains(b)) {
        return;
    }
    let zone = match saved_zone {
        b"UTC" => 0,
        b"NPT" => 1,
        b"EST" => 2,
        _ => return,
    };
    for (index, byte) in saved_name.iter().enumerate() {
        USERNAME[index].store(*byte, Ordering::Relaxed);
    }
    for index in saved_name.len()..USERNAME_CAPACITY {
        USERNAME[index].store(0, Ordering::Relaxed);
    }
    USERNAME_LEN.store(saved_name.len(), Ordering::Relaxed);
    TIMEZONE_SELECTION.store(zone, Ordering::Relaxed);
    ONBOARDING_STEP.store(3, Ordering::Relaxed);
}

fn save_configuration() -> bool {
    let length = USERNAME_LEN.load(Ordering::Relaxed).min(USERNAME_CAPACITY);
    if length == 0 { return false; }
    let mut bytes = [0u8; 64];
    let mut cursor = 0usize;
    let name_prefix = b"USERNAME=";
    bytes[cursor..cursor + name_prefix.len()].copy_from_slice(name_prefix);
    cursor += name_prefix.len();
    for index in 0..length {
        bytes[cursor] = USERNAME[index].load(Ordering::Relaxed);
        cursor += 1;
    }
    bytes[cursor] = b'\n'; cursor += 1;
    let zone_prefix = b"TIMEZONE=";
    bytes[cursor..cursor + zone_prefix.len()].copy_from_slice(zone_prefix);
    cursor += zone_prefix.len();
    let zone: &[u8] = match TIMEZONE_SELECTION.load(Ordering::Relaxed).min(2) {
        1 => b"NPT",
        2 => b"EST",
        _ => b"UTC",
    };
    bytes[cursor..cursor + zone.len()].copy_from_slice(zone);
    cursor += zone.len();
    bytes[cursor] = b'\n'; cursor += 1;
    let Ok(volume) = filesystem::open_volume() else { return false; };
    volume.write_root_file(CONFIG_FILE, &bytes[..cursor]).is_ok()
}

fn update_onboarding(info: &BootInfo) {
    let mut length = USERNAME_LEN.load(Ordering::Relaxed).min(USERNAME_CAPACITY);
    let mut redraw = false;

    while let Some(key) = interrupts::pop_key() {
        if ONBOARDING_STEP.load(Ordering::Relaxed) >= 3 {
            let active = ACTIVE_APP_WINDOW.load(Ordering::Relaxed);
            if active < APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8) && APP_WINDOW_KIND[active].load(Ordering::Relaxed) == 6 {
                if key == b'p' || key == b'P' {
                    VIDEO_PLAYING.store(if VIDEO_PLAYING.load(Ordering::Relaxed) == 0 { 1 } else { 0 }, Ordering::Relaxed);
                    redraw = true;
                    continue;
                } else if key == b's' || key == b'S' {
                    VIDEO_PLAYING.store(0, Ordering::Relaxed);
                    VIDEO_FRAME.store(0, Ordering::Relaxed);
                    redraw = true;
                    continue;
                }
            }
        }
        if ONBOARDING_STEP.load(Ordering::Relaxed) >= 3 && handle_text_editor_key(key) {
            redraw = true;
            continue;
        }
        match ONBOARDING_STEP.load(Ordering::Relaxed) {
            0 => {
                if key == b'\n' {
                    // The current atomic buffer/length are the persisted setup choice.
                    USERNAME_LEN.store(length, Ordering::Relaxed);
                    ONBOARDING_STEP.store(1, Ordering::Relaxed);
                    redraw = true;
                } else if key == 8 || key == 127 {
                    if length > 0 {
                        length -= 1;
                        USERNAME[length].store(0, Ordering::Relaxed);
                        USERNAME_LEN.store(length, Ordering::Relaxed);
                        redraw = true;
                    }
                } else if (32..=126).contains(&key) && length < USERNAME_CAPACITY {
                    USERNAME[length].store(key, Ordering::Relaxed);
                    length += 1;
                    USERNAME_LEN.store(length, Ordering::Relaxed);
                    redraw = true;
                }
            }
            1 => match key {
                0x81 => {
                    let selected = TIMEZONE_SELECTION.load(Ordering::Relaxed);
                    TIMEZONE_SELECTION.store(selected.saturating_sub(1), Ordering::Relaxed);
                    redraw = true;
                }
                0x82 => {
                    let selected = TIMEZONE_SELECTION.load(Ordering::Relaxed).min(2);
                    TIMEZONE_SELECTION.store((selected + 1).min(2), Ordering::Relaxed);
                    redraw = true;
                }
                b'\n' => {
                    ONBOARDING_STEP.store(2, Ordering::Relaxed);
                    redraw = true;
                }
                _ => {}
            },
            _ => {
                if ONBOARDING_STEP.load(Ordering::Relaxed) >= 3 {
                    let count = filesystem::open_volume().ok().map(|volume| {
                        let mut names = [[0u8; 11]; 32]; let mut count = 0usize;
                        let _ = volume.list_root_files(&mut names, &mut count); count
                    }).unwrap_or(0);
                    if key == b'\n' {
                        let length = TERMINAL_LEN.load(Ordering::Relaxed).min(TERMINAL_CAPACITY);
                        let mut line = [0u8; TERMINAL_CAPACITY];
                        for (i, byte) in line.iter_mut().enumerate().take(length) {
                            *byte = TERMINAL_INPUT[i].load(Ordering::Relaxed);
                        }
                        let saved = match filesystem::open_volume() {
                            Ok(volume) => volume.write_root_file(b"USER    TXT", &line[..length]).is_ok(),
                            Err(_) => false,
                        };
                        TERMINAL_SAVED.store(saved as u8, Ordering::Relaxed);
                        TERMINAL_LEN.store(0, Ordering::Relaxed);
                        for slot in &TERMINAL_INPUT {
                            slot.store(0, Ordering::Relaxed);
                        }
                        redraw = true;
                    } else if key == 8 || key == 127 {
                        let length = TERMINAL_LEN.load(Ordering::Relaxed).min(TERMINAL_CAPACITY);
                        if length > 0 { TERMINAL_INPUT[length - 1].store(0, Ordering::Relaxed); TERMINAL_LEN.store(length - 1, Ordering::Relaxed); redraw = true; }
                    } else if (32..=126).contains(&key) {
                        let length = TERMINAL_LEN.load(Ordering::Relaxed).min(TERMINAL_CAPACITY);
                        if length < TERMINAL_CAPACITY { let previous = if length == 0 { b' ' } else { TERMINAL_INPUT[length - 1].load(Ordering::Relaxed) }; let stored = if key.is_ascii_lowercase() && previous == b' ' { key.to_ascii_uppercase() } else { key }; TERMINAL_INPUT[length].store(stored, Ordering::Relaxed); TERMINAL_LEN.store(length + 1, Ordering::Relaxed); TERMINAL_SAVED.store(0, Ordering::Relaxed); redraw = true; }
                    } else if key == 0x81 {
                        let selected = FILE_SELECTION_INDEX.load(Ordering::Relaxed);
                        FILE_SELECTION_INDEX.store(selected.saturating_sub(1), Ordering::Relaxed);
                        redraw = true;
                    } else if key == 0x82 {
                        let selected = FILE_SELECTION_INDEX.load(Ordering::Relaxed);
                        FILE_SELECTION_INDEX.store(selected.saturating_add(1).min(count.saturating_sub(1)), Ordering::Relaxed);
                        redraw = true;
                    }
                } else if key == b'\n' && save_configuration() {
                    ONBOARDING_STEP.store(3, Ordering::Relaxed);
                    redraw = true;
                }
            }
        }
    }

    if process_mouse_events(info) { return; }
    let video_open = (0..APP_WINDOW_COUNT.load(Ordering::Relaxed).min(8))
        .any(|index| APP_WINDOW_KIND[index].load(Ordering::Relaxed) == 6);
    if ONBOARDING_STEP.load(Ordering::Relaxed) >= 3 && video_open && VIDEO_PLAYING.load(Ordering::Relaxed) != 0 {
        VIDEO_FRAME.fetch_add(1, Ordering::Relaxed);
        draw_desktop(info);
    }
    if redraw {
        draw_onboarding_screen(info);
    }
}

fn validate_memory_map(info: &BootInfo) -> bool {
    if info.memory_map.is_null()
        || info.memory_map_size == 0
        || info.memory_map_size > MAX_MEMORY_MAP_BYTES
        || info.memory_descriptor_version != UEFI_MEMORY_DESCRIPTOR_VERSION
        || info.memory_descriptor_size < mem::size_of::<UefiMemoryDescriptor>()
        || info.memory_map_size % info.memory_descriptor_size != 0
    {
        return false;
    }

    let descriptor_count = info.memory_map_size / info.memory_descriptor_size;
    if descriptor_count == 0 {
        return false;
    }

    for index in 0..descriptor_count {
        let Some(offset) = index.checked_mul(info.memory_descriptor_size) else {
            return false;
        };
        // SAFETY: the loader passes a live memory-map buffer and the bounds
        // above ensure each descriptor's fixed v1 prefix lies within it.
        let descriptor = unsafe {
            ptr::read_unaligned(
                info.memory_map
                    .add(offset)
                    .cast::<UefiMemoryDescriptor>(),
            )
        };

        let Some(region_size) = descriptor.page_count.checked_mul(UEFI_PAGE_SIZE) else {
            return false;
        };
        if descriptor
            .physical_start
            .checked_add(region_size)
            .is_none()
        {
            return false;
        }
    }

    true
}

fn draw_kernel_status(info: &BootInfo, memory_map_valid: bool, allocator_ready: bool, virtual_memory_ready: bool) {
    if info.framebuffer.is_null()
        || info.width < 320
        || info.height < 200
        || info.stride < info.width
        || info.stride.checked_mul(info.height).and_then(|v| v.checked_mul(4))
            .is_none_or(|required| required > info.framebuffer_size)
    {
        return;
    }

    fill_rect(info, 0, 0, info.width, info.height, (5, 8, 24));

    let scale = (info.height / 110).clamp(3, 10);
    let title = b"EVEREST OS";
    let title_width = title.len() * 6 * scale;
    draw_text(
        info,
        info.width.saturating_sub(title_width) / 2,
        info.height * 2 / 5,
        scale,
        title,
        (242, 246, 255),
    );

    let status = b"KERNEL ONLINE";
    let status_scale = (scale / 3).max(1);
    let status_width = status.len() * 6 * status_scale;
    draw_text(
        info,
        info.width.saturating_sub(status_width) / 2,
        info.height * 2 / 5 + 12 * scale,
        status_scale,
        status,
        (118, 222, 190),
    );

    let note = b"EARLY KERNEL HANDOFF COMPLETE";
    let note_width = note.len() * 6;
    draw_text(
        info,
        info.width.saturating_sub(note_width) / 2,
        info.height * 2 / 5 + 23 * scale,
        1,
        note,
        (130, 153, 206),
    );

    let memory_status: &[u8] = if memory_map_valid {
        b"UEFI MEMORY MAP VALIDATED"
    } else {
        b"UEFI MEMORY MAP INVALID"
    };
    let memory_status_width = memory_status.len() * 6;
    draw_text(
        info,
        info.width.saturating_sub(memory_status_width) / 2,
        info.height * 2 / 5 + 34 * scale,
        1,
        memory_status,
        if memory_map_valid { (118, 222, 190) } else { (255, 120, 120) },
    );

    let allocator_status: &[u8] = if allocator_ready { b"FRAME ALLOCATOR READY" } else { b"FRAME ALLOCATOR UNAVAILABLE" };
    let allocator_status_width = allocator_status.len() * 6;
    draw_text(
        info, info.width.saturating_sub(allocator_status_width) / 2,
        info.height * 2 / 5 + 42 * scale, 1, allocator_status,
        if allocator_ready { (118, 222, 190) } else { (255, 120, 120) },
    );

    let vm_status: &[u8] = if virtual_memory_ready {
        b"OWN PAGE TABLES + IDT ACTIVE"
    } else {
        b"VIRTUAL MEMORY / IDT FAILED"
    };
    let vm_status_width = vm_status.len() * 6;
    draw_text(
        info, info.width.saturating_sub(vm_status_width) / 2,
        info.height * 2 / 5 + 51 * scale, 1, vm_status,
        if virtual_memory_ready { (110, 190, 255) } else { (255, 120, 120) },
    );
}

fn put_pixel(info: &BootInfo, x: usize, y: usize, color: (u8, u8, u8)) {
    if x >= info.width || y >= info.height || info.framebuffer.is_null() {
        return;
    }

    let Some(offset) = y
        .checked_mul(info.stride)
        .and_then(|row| row.checked_add(x))
        .and_then(|pixel| pixel.checked_mul(4))
    else {
        return;
    };
    if offset.checked_add(4).is_none_or(|end| end > info.framebuffer_size) {
        return;
    }

    unsafe {
        let pixel = info.framebuffer.add(offset);
        let (red, green, blue) = color;
        match info.pixel_format {
            PixelFormat::Rgb => {
                ptr::write_volatile(pixel, red);
                ptr::write_volatile(pixel.add(1), green);
                ptr::write_volatile(pixel.add(2), blue);
            }
            PixelFormat::Bgr => {
                ptr::write_volatile(pixel, blue);
                ptr::write_volatile(pixel.add(1), green);
                ptr::write_volatile(pixel.add(2), red);
            }
        }
        ptr::write_volatile(pixel.add(3), 0);
    }
}

fn fill_rect(info: &BootInfo, x: usize, y: usize, width: usize, height: usize, color: (u8, u8, u8)) {
    let end_x = x.saturating_add(width).min(info.width);
    let end_y = y.saturating_add(height).min(info.height);
    for py in y.min(info.height)..end_y {
        for px in x.min(info.width)..end_x {
            put_pixel(info, px, py, color);
        }
    }
}

fn draw_text(
    info: &BootInfo,
    x: usize,
    y: usize,
    scale: usize,
    text: &[u8],
    color: (u8, u8, u8),
) {
    let mut cursor_x = x;
    for &character in text {
        let glyph = glyph_rows(character);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5usize {
                if bits & (1 << (4 - col)) != 0 {
                    fill_rect(
                        info,
                        cursor_x.saturating_add(col.saturating_mul(scale)),
                        y.saturating_add(row.saturating_mul(scale)),
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        cursor_x = cursor_x.saturating_add(6 * scale);
    }
}

fn glyph_rows(character: u8) -> [u8; 7] {
    match character {
        b'A' => [14, 17, 17, 31, 17, 17, 17],
        b'B' => [30, 17, 17, 30, 17, 17, 30],
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'D' => [30, 17, 17, 17, 17, 17, 30],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'F' => [31, 16, 16, 30, 16, 16, 16],
        b'G' => [14, 17, 16, 23, 17, 17, 15],
        b'H' => [17, 17, 17, 31, 17, 17, 17],
        b'I' => [31, 4, 4, 4, 4, 4, 31],
        b'J' => [7, 2, 2, 2, 18, 18, 12],
        b'K' => [17, 18, 20, 24, 20, 18, 17],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'M' => [17, 27, 21, 21, 17, 17, 17],
        b'N' => [17, 25, 25, 21, 19, 19, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'P' => [30, 17, 17, 30, 16, 16, 16],
        b'Q' => [14, 17, 17, 17, 21, 18, 13],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'U' => [17, 17, 17, 17, 17, 17, 14],
        b'V' => [17, 17, 17, 17, 17, 10, 4],
        b'W' => [17, 17, 17, 21, 21, 21, 10],
        b'X' => [17, 17, 10, 4, 10, 17, 17],
        b'Y' => [17, 17, 10, 4, 4, 4, 4],
        b'Z' => [31, 1, 2, 4, 8, 16, 31],
        b'0' => [14, 17, 19, 21, 25, 17, 14],
        b'1' => [4, 12, 4, 4, 4, 4, 14],
        b'2' => [14, 17, 1, 2, 4, 8, 31],
        b'3' => [30, 1, 1, 14, 1, 1, 30],
        b'4' => [2, 6, 10, 18, 31, 2, 2],
        b'5' => [31, 16, 16, 30, 1, 1, 30],
        b'6' => [14, 16, 16, 30, 17, 17, 14],
        b'7' => [31, 1, 2, 4, 8, 8, 8],
        b'8' => [14, 17, 17, 14, 17, 17, 14],
        b'9' => [14, 17, 17, 15, 1, 1, 14],
        b'a' => [0, 0, 14, 1, 15, 17, 15],
        b'b' => [16, 16, 30, 17, 17, 17, 30],
        b'c' => [0, 0, 14, 16, 16, 17, 14],
        b'd' => [1, 1, 15, 17, 17, 17, 15],
        b'e' => [0, 0, 14, 17, 31, 16, 14],
        b'f' => [6, 9, 8, 28, 8, 8, 8],
        b'g' => [0, 0, 15, 17, 15, 1, 14],
        b'h' => [16, 16, 30, 17, 17, 17, 17],
        b'i' => [4, 0, 12, 4, 4, 4, 14],
        b'j' => [2, 0, 6, 2, 2, 18, 12],
        b'k' => [16, 16, 18, 20, 24, 20, 18],
        b'l' => [12, 4, 4, 4, 4, 4, 14],
        b'm' => [0, 0, 26, 21, 21, 21, 21],
        b'n' => [0, 0, 30, 17, 17, 17, 17],
        b'o' => [0, 0, 14, 17, 17, 17, 14],
        b'p' => [0, 0, 30, 17, 30, 16, 16],
        b'q' => [0, 0, 15, 17, 15, 1, 1],
        b'r' => [0, 0, 22, 25, 16, 16, 16],
        b's' => [0, 0, 15, 16, 14, 1, 30],
        b't' => [8, 8, 28, 8, 8, 9, 6],
        b'u' => [0, 0, 17, 17, 17, 19, 13],
        b'v' => [0, 0, 17, 17, 17, 10, 4],
        b'w' => [0, 0, 17, 17, 21, 21, 10],
        b'x' => [0, 0, 17, 10, 4, 10, 17],
        b'y' => [0, 0, 17, 17, 15, 1, 14],
        b'z' => [0, 0, 31, 2, 4, 8, 31],
        b':' => [0, 4, 4, 0, 4, 4, 0],
        b'+' => [0, 4, 4, 31, 4, 4, 0],
        b'-' => [0, 0, 0, 31, 0, 0, 0],
        b'*' => [0, 17, 10, 4, 10, 17, 0],
        b'/' => [1, 2, 2, 4, 8, 8, 16],
        b'=' => [0, 31, 0, 31, 0, 0, 0],
        b' ' => [0, 0, 0, 0, 0, 0, 0],
        _ => [0, 0, 0, 0, 0, 0, 0],
    }
}


#[cfg(test)]
mod font_tests {
    use super::glyph_rows;

    #[test]
    fn bitmap_font_covers_ascii_uppercase_and_digits() {
        for character in b'A'..=b'Z' {
            assert!(glyph_rows(character).iter().any(|row| *row != 0), "missing glyph: {}", character as char);
        }
        for character in b'0'..=b'9' {
            assert!(glyph_rows(character).iter().any(|row| *row != 0), "missing glyph: {}", character as char);
        }
    }

    #[test]
    fn bitmap_font_covers_every_lowercase_letter_used_by_boot_greeting() {
        for character in b"Hello Everest OS" {
            if *character != b' ' {
                assert!(glyph_rows(*character).iter().any(|row| *row != 0), "missing glyph: {}", *character as char);
            }
        }
    }
}
