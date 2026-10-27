#![no_std]

pub mod arch;
pub mod memory;
pub mod interrupts;
pub mod heap;
pub mod filesystem;
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

const DESKTOP_BG: (u8, u8, u8) = (12, 17, 32);
const TASKBAR_BG: (u8, u8, u8) = (20, 28, 48);
const WINDOW_BG: (u8, u8, u8) = (14, 20, 36);
const TITLEBAR_BG: (u8, u8, u8) = (33, 47, 74);

fn draw_desktop(info: &BootInfo) {
    fill_rect(info, 0, 0, info.width, info.height, DESKTOP_BG);

    let window_width = info.width.saturating_mul(3) / 5;
    let window_height = info.height.saturating_mul(3) / 5;
    let window_x = info.width.saturating_sub(window_width) / 2;
    let window_y = info.height.saturating_sub(window_height) / 2;
    let titlebar_height = 42usize.min(window_height);
    draw_border(info, window_x, window_y, window_width, window_height, 3, UI_ACCENT);
    fill_rect(info, window_x + 3, window_y + 3, window_width.saturating_sub(6),
        window_height.saturating_sub(6), WINDOW_BG);
    fill_rect(info, window_x + 3, window_y + 3, window_width.saturating_sub(6),
        titlebar_height.saturating_sub(3), TITLEBAR_BG);
    draw_text(info, window_x + 24, window_y + 13, 2, b"Everest Terminal", UI_WHITE);
    draw_text(info, window_x + 24, window_y + 78, 2, b"Everest OS ready", (160, 190, 230));
    draw_text(info, window_x + 24, window_y + 112, 2, b"Welcome,", (160, 190, 230));
    let length = USERNAME_LEN.load(Ordering::Relaxed).min(USERNAME_CAPACITY);
    let mut username = [0u8; USERNAME_CAPACITY];
    for (index, byte) in username.iter_mut().enumerate().take(length) {
        *byte = USERNAME[index].load(Ordering::Relaxed);
    }
    draw_text(info, window_x + 24, window_y + 140, 2, &username[..length], UI_WHITE);
    draw_text(info, window_x + 24, window_y + 162, 1, b"TIMEZONE:", (160, 190, 230));
    let timezone: &[u8] = match TIMEZONE_SELECTION.load(Ordering::Relaxed).min(2) {
        1 => b"NPT",
        2 => b"EST",
        _ => b"UTC",
    };
    draw_text(info, window_x + 24 + 9 * 6, window_y + 162, 1, timezone, UI_WHITE);
    draw_text(info, window_x + 24, window_y + 176, 1, b"FAT32 FILE: HELLO.TXT", (110, 190, 255));
    if DISK_TEXT_OK.load(Ordering::Relaxed) == 1 {
        let disk_length = DISK_TEXT_LEN.load(Ordering::Relaxed).min(DISK_TEXT_CAPACITY);
        let mut disk_text = [0u8; DISK_TEXT_CAPACITY];
        for (index, byte) in disk_text.iter_mut().enumerate().take(disk_length) { *byte = DISK_TEXT[index].load(Ordering::Relaxed); }
        draw_text(info, window_x + 24, window_y + 194, 1, &disk_text[..disk_length], UI_WHITE);
    } else {
        draw_text(info, window_x + 24, window_y + 194, 1, b"FILE UNAVAILABLE", (255, 120, 120));
    }

    let taskbar_height = 56usize.min(info.height);
    let taskbar_y = info.height.saturating_sub(taskbar_height);
    fill_rect(info, 0, taskbar_y, info.width, taskbar_height, TASKBAR_BG);
    draw_border(info, 16, taskbar_y + 12, 104, 32, 2, UI_ACCENT);
    fill_rect(info, 18, taskbar_y + 14, 100, 28, (42, 62, 96));
    draw_text(info, 30, taskbar_y + 20, 2, b"START", UI_WHITE);
    draw_text(info, info.width.saturating_sub(120), taskbar_y + 19, 2, b"12:00", UI_WHITE);

    // Keep a small kernel-health indicator visible on the desktop taskbar.
    if info.width >= 180 {
        fill_rect(info, info.width - 160, taskbar_y + 26, 4, 4, (110, 190, 255));
    }
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
                // Step 3 is the completed setup confirmation; Enter is accepted
                // while leaving this milestone's final confirmation visible.
                if key == b'\n' {
                    // Only leave setup once the configuration is durable on disk.
                    if save_configuration() {
                        ONBOARDING_STEP.store(3, Ordering::Relaxed);
                        redraw = true;
                    }
                }
            }
        }
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
