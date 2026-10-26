#![no_std]

pub mod arch;
pub mod memory;

use core::{mem, ptr};

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
    draw_kernel_status(info, memory_map_valid, allocator_ready, virtual_memory_ready);

    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
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
        b' ' => [0, 0, 0, 0, 0, 0, 0],
        _ => [0, 0, 0, 0, 0, 0, 0],
    }
}
