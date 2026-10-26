//! x86-64 early virtual memory and CPU exception setup.
//!
//! The initial address space uses a low-4-GiB identity map with 2 MiB pages.
//! Page-table pages themselves come from the physical frame allocator. This
//! keeps the running kernel, its stack, BootInfo and the usual GOP framebuffer
//! mapped while removing the firmware's CR3 from the active execution path.

use core::{
    arch::{asm, global_asm},
    mem,
    ptr,
    sync::atomic::{AtomicPtr, AtomicUsize, Ordering},
};

use crate::{BootInfo, draw_text, fill_rect};

const PAGE_SIZE: u64 = 4096;
const PAGE_PRESENT: u64 = 1;
const PAGE_WRITABLE: u64 = 1 << 1;
const PAGE_HUGE: u64 = 1 << 7;
const HUGE_PAGE_SIZE: u64 = 2 * 1024 * 1024;
const IDENTITY_MAP_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
const PAGE_DIRECTORY_ENTRIES: usize = 512;

static BOOT_INFO: AtomicPtr<BootInfo> = AtomicPtr::new(ptr::null_mut());
static BREAKPOINT_COUNT: AtomicUsize = AtomicUsize::new(0);

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    options: u16,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const MISSING: Self = Self {
        offset_low: 0,
        selector: 0,
        options: 0,
        offset_middle: 0,
        offset_high: 0,
        reserved: 0,
    };

    fn interrupt_gate(handler: u64, selector: u16) -> Self {
        Self {
            offset_low: handler as u16,
            selector,
            options: 0x8e00,
            offset_middle: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

static mut IDT: [IdtEntry; 256] = [IdtEntry::MISSING; 256];

global_asm!(
    r#"
    .macro ISR_NOERR n
    .global everest_isr_\n
    everest_isr_\n:
        push 0
        push \n
        jmp everest_exception_common
    .endm

    .macro ISR_ERR n
    .global everest_isr_\n
    everest_isr_\n:
        push \n
        jmp everest_exception_common
    .endm

    ISR_NOERR 0
    ISR_NOERR 1
    ISR_NOERR 2
    ISR_NOERR 3
    ISR_NOERR 4
    ISR_NOERR 5
    ISR_NOERR 6
    ISR_NOERR 7
    ISR_ERR   8
    ISR_NOERR 9
    ISR_ERR   10
    ISR_ERR   11
    ISR_ERR   12
    ISR_ERR   13
    ISR_ERR   14
    ISR_NOERR 15
    ISR_NOERR 16
    ISR_ERR   17
    ISR_NOERR 18
    ISR_NOERR 19
    ISR_NOERR 20
    ISR_ERR   21
    ISR_NOERR 22
    ISR_NOERR 23
    ISR_NOERR 24
    ISR_NOERR 25
    ISR_NOERR 26
    ISR_NOERR 27
    ISR_NOERR 28
    ISR_ERR   29
    ISR_ERR   30
    ISR_NOERR 31

    .global everest_exception_common
    everest_exception_common:
        push rax
        push rbx
        push rcx
        push rdx
        push rsi
        push rdi
        push rbp
        push r8
        push r9
        push r10
        push r11
        push r12
        push r13
        push r14
        push r15

        mov r12, rsp
        mov rdi, [r12 + 120]
        mov rsi, [r12 + 128]
        mov rdx, [r12 + 136]
        and rsp, -16
        call everest_exception_dispatch
        mov rsp, r12

        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rbp
        pop rdi
        pop rsi
        pop rdx
        pop rcx
        pop rbx
        pop rax
        add rsp, 16
        iretq
    "#
);

unsafe extern "C" {
    fn everest_isr_0();
    fn everest_isr_1();
    fn everest_isr_2();
    fn everest_isr_3();
    fn everest_isr_4();
    fn everest_isr_5();
    fn everest_isr_6();
    fn everest_isr_7();
    fn everest_isr_8();
    fn everest_isr_9();
    fn everest_isr_10();
    fn everest_isr_11();
    fn everest_isr_12();
    fn everest_isr_13();
    fn everest_isr_14();
    fn everest_isr_15();
    fn everest_isr_16();
    fn everest_isr_17();
    fn everest_isr_18();
    fn everest_isr_19();
    fn everest_isr_20();
    fn everest_isr_21();
    fn everest_isr_22();
    fn everest_isr_23();
    fn everest_isr_24();
    fn everest_isr_25();
    fn everest_isr_26();
    fn everest_isr_27();
    fn everest_isr_28();
    fn everest_isr_29();
    fn everest_isr_30();
    fn everest_isr_31();
}

/// Build page tables from allocator-owned physical frames and switch CR3.
///
/// # Safety
/// Caller must have disabled maskable interrupts and must provide an allocator
/// whose page-table frames are writable through the current firmware mapping.
pub unsafe fn initialize(boot_info: &BootInfo, allocator: &mut crate::memory::FrameAllocator<'_>) -> Result<(), &'static str> {
    if boot_info.kernel_image_start >= IDENTITY_MAP_LIMIT
        || boot_info.kernel_image_start.checked_add(boot_info.kernel_image_size).is_none_or(|end| end > IDENTITY_MAP_LIMIT)
        || (boot_info as *const BootInfo as u64) >= IDENTITY_MAP_LIMIT
        || (boot_info.memory_map as u64) >= IDENTITY_MAP_LIMIT
        || (boot_info.framebuffer as u64) >= IDENTITY_MAP_LIMIT
        || boot_info.framebuffer_size as u64 > IDENTITY_MAP_LIMIT.saturating_sub(boot_info.framebuffer as u64)
    {
        return Err("Required early kernel memory lies outside the initial 4 GiB identity map");
    }

    let root = allocator.allocate().map_err(|_| "Could not allocate PML4 frame")?;
    let pdpt = allocator.allocate().map_err(|_| "Could not allocate PDPT frame")?;
    let mut directories = [0u64; 4];
    for directory in &mut directories {
        *directory = allocator.allocate().map_err(|_| "Could not allocate page-directory frame")?;
    }
    for frame in [root, pdpt, directories[0], directories[1], directories[2], directories[3]] {
        if frame >= IDENTITY_MAP_LIMIT || frame % PAGE_SIZE != 0 {
            return Err("Page-table frame is not safely identity-addressable");
        }
        unsafe { ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE as usize) };
    }

    let pml4 = root as *mut u64;
    let page_directory_pointer = pdpt as *mut u64;
    unsafe {
        pml4.write(pdpt | PAGE_PRESENT | PAGE_WRITABLE);
        for (slot, directory_frame) in directories.iter().enumerate() {
            page_directory_pointer.add(slot).write(*directory_frame | PAGE_PRESENT | PAGE_WRITABLE);
            let directory = *directory_frame as *mut u64;
            for entry in 0..PAGE_DIRECTORY_ENTRIES {
                let physical = (slot as u64 * PAGE_DIRECTORY_ENTRIES as u64 + entry as u64) * HUGE_PAGE_SIZE;
                directory.add(entry).write(physical | PAGE_PRESENT | PAGE_WRITABLE | PAGE_HUGE);
            }
        }
    }

    // Publish the boot data before loading the IDT so exception handlers can
    // render diagnostics even if the first post-switch instruction faults.
    BOOT_INFO.store(boot_info as *const BootInfo as *mut BootInfo, Ordering::Release);
    unsafe { install_idt() };
    unsafe { load_cr3(root) };

    // Exercise the installed IDT and common assembly entry path on every boot.
    // The breakpoint handler records success and returns with IRETQ.
    unsafe { asm!("int3") };
    if BREAKPOINT_COUNT.load(Ordering::Acquire) != 1 {
        return Err("IDT breakpoint self-test failed");
    }
    Ok(())
}

unsafe fn install_idt() {
    let mut code_selector: u16;
    unsafe { asm!("mov {0:x}, cs", out(reg) code_selector, options(nomem, nostack, preserves_flags)) };
    let stubs = [
        everest_isr_0 as *const () as usize, everest_isr_1 as *const () as usize, everest_isr_2 as *const () as usize, everest_isr_3 as *const () as usize,
        everest_isr_4 as *const () as usize, everest_isr_5 as *const () as usize, everest_isr_6 as *const () as usize, everest_isr_7 as *const () as usize,
        everest_isr_8 as *const () as usize, everest_isr_9 as *const () as usize, everest_isr_10 as *const () as usize, everest_isr_11 as *const () as usize,
        everest_isr_12 as *const () as usize, everest_isr_13 as *const () as usize, everest_isr_14 as *const () as usize, everest_isr_15 as *const () as usize,
        everest_isr_16 as *const () as usize, everest_isr_17 as *const () as usize, everest_isr_18 as *const () as usize, everest_isr_19 as *const () as usize,
        everest_isr_20 as *const () as usize, everest_isr_21 as *const () as usize, everest_isr_22 as *const () as usize, everest_isr_23 as *const () as usize,
        everest_isr_24 as *const () as usize, everest_isr_25 as *const () as usize, everest_isr_26 as *const () as usize, everest_isr_27 as *const () as usize,
        everest_isr_28 as *const () as usize, everest_isr_29 as *const () as usize, everest_isr_30 as *const () as usize, everest_isr_31 as *const () as usize,
    ];
    let idt_ptr = ptr::addr_of_mut!(IDT).cast::<IdtEntry>();
    for (vector, address) in stubs.iter().enumerate() {
        unsafe { idt_ptr.add(vector).write(IdtEntry::interrupt_gate(*address as u64, code_selector)) };
    }
    let pointer = IdtPointer {
        limit: (mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
        base: idt_ptr as u64,
    };
    unsafe { asm!("lidt [{0}]", in(reg) &pointer, options(readonly, nostack, preserves_flags)) };
}

unsafe fn load_cr3(root: u64) {
    unsafe { asm!("mov cr3, {0}", in(reg) root, options(nostack, preserves_flags)) };
}

/// Common exception dispatch target called by the assembly stubs.
///
/// # Safety
/// Called only from the matching interrupt stubs with the normalized stack frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn everest_exception_dispatch(vector: u64, error_code: u64, instruction_pointer: u64) {
    if vector == 3 {
        BREAKPOINT_COUNT.fetch_add(1, Ordering::Release);
        return;
    }
    let boot_info = BOOT_INFO.load(Ordering::Acquire);
    if boot_info.is_null() {
        loop { unsafe { asm!("cli; hlt", options(nomem, nostack)) }; }
    }
    let info = unsafe { &*boot_info };
    if vector == 14 {
        let fault_address: u64;
        unsafe { asm!("mov {0}, cr2", out(reg) fault_address, options(nomem, nostack, preserves_flags)) };
        show_exception(info, b"PAGE FAULT", error_code, fault_address, instruction_pointer);
    } else if vector == 0 {
        show_exception(info, b"DIVIDE BY ZERO", error_code, 0, instruction_pointer);
    } else if vector == 13 {
        show_exception(info, b"GENERAL PROTECTION", error_code, 0, instruction_pointer);
    } else {
        show_exception(info, b"CPU EXCEPTION", error_code, vector, instruction_pointer);
    }
    loop { unsafe { asm!("cli; hlt", options(nomem, nostack)) }; }
}

fn show_exception(info: &BootInfo, title: &[u8], error_code: u64, address: u64, instruction_pointer: u64) {
    fill_rect(info, 0, 0, info.width, info.height, (28, 5, 14));
    draw_text(info, 24, 24, 2, b"EVEREST OS EXCEPTION", (255, 230, 235));
    draw_text(info, 24, 64, 2, title, (255, 120, 145));
    let mut error = [0u8; 18];
    let mut fault = [0u8; 18];
    let mut rip = [0u8; 18];
    draw_text(info, 24, 112, 1, b"ERROR CODE", (220, 220, 235));
    draw_text(info, 160, 112, 1, hex64(error_code, &mut error), (255, 230, 235));
    if title == b"PAGE FAULT" {
        draw_text(info, 24, 136, 1, b"FAULT ADDRESS", (220, 220, 235));
        draw_text(info, 160, 136, 1, hex64(address, &mut fault), (255, 230, 235));
    }
    draw_text(info, 24, 160, 1, b"INSTRUCTION", (220, 220, 235));
    draw_text(info, 160, 160, 1, hex64(instruction_pointer, &mut rip), (255, 230, 235));
    draw_text(info, 24, 208, 1, b"SYSTEM HALTED FOR DIAGNOSTICS", (255, 120, 145));
}

fn hex64(value: u64, output: &mut [u8; 18]) -> &[u8] {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    output[0] = b'0';
    output[1] = b'x';
    for index in 0..16 {
        let shift = (15 - index) * 4;
        output[index + 2] = DIGITS[((value >> shift) & 0xf) as usize];
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idt_gate_encodes_handler_address_and_selector() {
        let address = 0x1234_5678_9abc_def0;
        let entry = IdtEntry::interrupt_gate(address, 0x38);
        let offset_low = entry.offset_low;
        let selector = entry.selector;
        let options = entry.options;
        let offset_middle = entry.offset_middle;
        let offset_high = entry.offset_high;
        assert_eq!(offset_low, 0xdef0);
        assert_eq!(selector, 0x38);
        assert_eq!(options, 0x8e00);
        assert_eq!(offset_middle, 0x9abc);
        assert_eq!(offset_high, 0x1234_5678);
    }

    #[test]
    fn hex_diagnostic_is_fixed_width() {
        let mut output = [0; 18];
        assert_eq!(hex64(0xdead_beef, &mut output), b"0x00000000DEADBEEF");
    }
}
