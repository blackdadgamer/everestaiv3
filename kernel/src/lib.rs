#![no_std]

use core::ptr;

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    Rgb = 0,
    Bgr = 1,
}

#[repr(C)]
pub struct BootInfo {
    pub framebuffer: *mut u8,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub pixel_format: PixelFormat,
}

/// Entry reached only after the UEFI boot services have been exited.
///
/// This is an intentionally small first kernel milestone. It uses the firmware's
/// existing CPU state and framebuffer mapping; it does not yet initialize its
/// own page tables, interrupt controllers, scheduler, or device drivers.
pub fn kernel_main(info: &BootInfo) -> ! {
    draw_kernel_status(info);
    loop {
        // No scheduler or interrupt subsystem exists yet. Halt until an interrupt
        // arrives, then re-enter the loop. This is a deliberate bring-up idle loop.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

fn draw_kernel_status(info: &BootInfo) {
    if info.framebuffer.is_null()
        || info.width < 320
        || info.height < 200
        || info.stride < info.width
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
    let note_scale = 1usize;
    let note_width = note.len() * 6 * note_scale;
    draw_text(
        info,
        info.width.saturating_sub(note_width) / 2,
        info.height * 2 / 5 + 23 * scale,
        note_scale,
        note,
        (130, 153, 206),
    );
}

fn put_pixel(info: &BootInfo, x: usize, y: usize, color: (u8, u8, u8)) {
    if x >= info.width || y >= info.height || info.framebuffer.is_null() {
        return;
    }

    let offset = (y * info.stride + x) * 4;
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
                        cursor_x + col * scale,
                        y + row * scale,
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
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'I' => [31, 4, 4, 4, 4, 4, 31],
        b'K' => [17, 18, 20, 24, 20, 18, 17],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'N' => [17, 25, 25, 21, 19, 19, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'V' => [17, 17, 17, 17, 17, 10, 4],
        b' ' => [0, 0, 0, 0, 0, 0, 0],
        _ => [0, 0, 0, 0, 0, 0, 0],
    }
}
