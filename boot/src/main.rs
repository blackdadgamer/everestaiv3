#![no_main]
#![no_std]

use core::ptr;

use everest_kernel::{kernel_main, BootInfo, PixelFormat as KernelPixelFormat};
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};

#[entry]
fn main() -> Status {
    if let Err(error) = uefi::helpers::init() {
        let _ = error;
        return Status::ABORTED;
    }

    uefi::println!("Everest OS firmware bring-up");
    uefi::println!("Preparing the early kernel handoff.");

    let handle = match boot::get_handle_for_protocol::<GraphicsOutput>() {
        Ok(handle) => handle,
        Err(_) => {
            uefi::println!("GOP framebuffer unavailable; cannot enter the visual kernel milestone.");
            let _ = boot::stall(2_000_000);
            return Status::SUCCESS;
        }
    };

    let mut gop = match boot::open_protocol_exclusive::<GraphicsOutput>(handle) {
        Ok(gop) => gop,
        Err(_) => {
            uefi::println!("Could not open GOP; cannot enter the visual kernel milestone.");
            let _ = boot::stall(2_000_000);
            return Status::SUCCESS;
        }
    };

    let info = gop.current_mode_info();
    let (width, height) = info.resolution();
    let stride = info.stride();
    let format = info.pixel_format();
    let mut framebuffer = gop.frame_buffer();

    if width < 320
        || height < 200
        || stride < width
        || framebuffer.size() < stride.saturating_mul(height).saturating_mul(4)
    {
        uefi::println!("Framebuffer geometry is not suitable for the splash screen.");
        let _ = boot::stall(2_000_000);
        return Status::SUCCESS;
    }

    if !matches!(format, PixelFormat::Rgb | PixelFormat::Bgr) {
        uefi::println!("Framebuffer pixel format is not supported by this renderer.");
        let _ = boot::stall(2_000_000);
        return Status::SUCCESS;
    }

    let kernel_pixel_format = match format {
        PixelFormat::Rgb => KernelPixelFormat::Rgb,
        PixelFormat::Bgr => KernelPixelFormat::Bgr,
        _ => return Status::UNSUPPORTED,
    };
    let base = framebuffer.as_mut_ptr();
    let boot_info = BootInfo {
        framebuffer: base,
        width,
        height,
        stride,
        pixel_format: kernel_pixel_format,
    };

    draw_splash(base, stride, width, height, format);
    drop(framebuffer);
    drop(gop);

    // Leave enough time for a physical-device camera/photo test, then take over.
    let _ = boot::stall(1_500_000);

    // UEFI services are no longer available after this call. The returned memory
    // map owner is deliberately kept alive while the early kernel is running.
    let _memory_map = unsafe { boot::exit_boot_services(boot::MemoryType::LOADER_DATA) };
    kernel_main(&boot_info)
}

fn draw_splash(base: *mut u8, stride: usize, width: usize, height: usize, format: PixelFormat) {
    fill_rect(base, stride, width, height, format, 0, 0, width, height, (5, 8, 24));

    // Quiet stars in the upper sky.
    for i in 0..52usize {
        let x = (i.wrapping_mul(97).wrapping_add(31)) % width;
        let y = (i.wrapping_mul(53).wrapping_add(17)) % (height / 2).max(1);
        put_pixel(base, stride, width, height, format, x, y, (78, 91, 140));
    }

    let horizon = height * 2 / 3;
    fill_triangle(
        base, stride, width, height, format,
        (width / 16, horizon), (width * 7 / 16, height / 4), (width * 5 / 8, horizon),
        (25, 39, 83),
    );
    fill_triangle(
        base, stride, width, height, format,
        (width * 3 / 8, horizon), (width * 11 / 16, height / 3), (width * 15 / 16, horizon),
        (19, 29, 65),
    );
    fill_triangle(
        base, stride, width, height, format,
        (width / 4, horizon), (width * 9 / 16, height / 2), (width * 13 / 16, horizon),
        (39, 51, 99),
    );

    fill_triangle(
        base, stride, width, height, format,
        (width * 7 / 16, height / 4), (width * 15 / 32, height * 3 / 8), (width * 17 / 32, height * 3 / 8),
        (190, 207, 241),
    );
    fill_triangle(
        base, stride, width, height, format,
        (width * 11 / 16, height / 3), (width * 21 / 32, height * 9 / 20), (width * 23 / 32, height * 9 / 20),
        (155, 176, 221),
    );

    let title_scale = (height / 100).clamp(3, 10);
    let title_width = 10 * 6 * title_scale;
    let title_x = width.saturating_sub(title_width) / 2;
    let title_y = height * 3 / 4;
    draw_text(
        base, stride, width, height, format,
        title_x, title_y, title_scale, b"EVEREST OS", (242, 246, 255),
    );

    let subtitle_scale = (title_scale / 3).max(1);
    let subtitle = b"INDEPENDENT OPERATING SYSTEM";
    let subtitle_width = subtitle.len() * 6 * subtitle_scale;
    draw_text(
        base, stride, width, height, format,
        width.saturating_sub(subtitle_width) / 2, title_y + 10 * title_scale,
        subtitle_scale, subtitle, (130, 153, 206),
    );
}

fn put_pixel(
    base: *mut u8,
    stride: usize,
    width: usize,
    height: usize,
    format: PixelFormat,
    x: usize,
    y: usize,
    color: (u8, u8, u8),
) {
    if x >= width || y >= height {
        return;
    }
    let offset = (y * stride + x) * 4;
    unsafe {
        let pixel = base.add(offset);
        let (r, g, b) = color;
        match format {
            PixelFormat::Rgb => {
                ptr::write_volatile(pixel, r);
                ptr::write_volatile(pixel.add(1), g);
                ptr::write_volatile(pixel.add(2), b);
            }
            PixelFormat::Bgr => {
                ptr::write_volatile(pixel, b);
                ptr::write_volatile(pixel.add(1), g);
                ptr::write_volatile(pixel.add(2), r);
            }
            _ => return,
        }
        ptr::write_volatile(pixel.add(3), 0);
    }
}

fn fill_rect(
    base: *mut u8, stride: usize, width: usize, height: usize, format: PixelFormat,
    x: usize, y: usize, rect_width: usize, rect_height: usize, color: (u8, u8, u8),
) {
    let x_end = x.saturating_add(rect_width).min(width);
    let y_end = y.saturating_add(rect_height).min(height);
    for py in y.min(height)..y_end {
        for px in x.min(width)..x_end {
            put_pixel(base, stride, width, height, format, px, py, color);
        }
    }
}

fn fill_triangle(
    base: *mut u8, stride: usize, width: usize, height: usize, format: PixelFormat,
    a: (usize, usize), b: (usize, usize), c: (usize, usize), color: (u8, u8, u8),
) {
    let min_y = a.1.min(b.1).min(c.1).min(height);
    let max_y = a.1.max(b.1).max(c.1).min(height.saturating_sub(1));
    if min_y > max_y {
        return;
    }
    for y in min_y..=max_y {
        let mut intersections = [0usize; 3];
        let mut count = 0usize;
        let edges = [(a, b), (b, c), (c, a)];
        for (p1, p2) in edges {
            let (x1, y1) = p1;
            let (x2, y2) = p2;
            if y1 == y2 || y < y1.min(y2) || y >= y1.max(y2) {
                continue;
            }
            let dy = y2 as i64 - y1 as i64;
            let dx = x2 as i64 - x1 as i64;
            let x = x1 as i64 + (y as i64 - y1 as i64) * dx / dy;
            if count < intersections.len() {
                intersections[count] = x.max(0) as usize;
                count += 1;
            }
        }
        if count >= 2 {
            let left = intersections[0].min(intersections[1]).min(width);
            let right = intersections[0].max(intersections[1]).min(width.saturating_sub(1));
            if left <= right {
                for x in left..=right {
                    put_pixel(base, stride, width, height, format, x, y, color);
                }
            }
        }
    }
}

fn draw_text(
    base: *mut u8, stride: usize, width: usize, height: usize, format: PixelFormat,
    x: usize, y: usize, scale: usize, text: &[u8], color: (u8, u8, u8),
) {
    let mut cursor_x = x;
    for &character in text {
        let glyph = glyph_rows(character);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5usize {
                if bits & (1 << (4 - col)) != 0 {
                    fill_rect(
                        base, stride, width, height, format,
                        cursor_x + col * scale, y + row * scale, scale, scale, color,
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
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'I' => [31, 4, 4, 4, 4, 4, 31],
        b'N' => [17, 25, 25, 21, 19, 19, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'P' => [30, 17, 17, 30, 16, 16, 16],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'V' => [17, 17, 17, 17, 17, 10, 4],
        b' ' => [0, 0, 0, 0, 0, 0, 0],
        _ => [0, 0, 0, 0, 0, 0, 0],
    }
}
