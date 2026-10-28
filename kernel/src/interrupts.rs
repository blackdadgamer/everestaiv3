//! Legacy 8259 PIC, PIT clock, PS/2 keyboard and auxiliary mouse support.
use core::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use core::arch::{asm, global_asm};

static TICKS: AtomicU64 = AtomicU64::new(0);
static LAST_KEY: AtomicU8 = AtomicU8::new(0);
static KEY_COUNT: AtomicU64 = AtomicU64::new(0);
static SHIFT_PRESSED: AtomicU8 = AtomicU8::new(0);

// Single-producer (IRQ) / single-consumer (kernel loop) keyboard event ring.
const KEY_QUEUE_CAPACITY: usize = 64;
static KEY_QUEUE: [AtomicU8; KEY_QUEUE_CAPACITY] =
    [const { AtomicU8::new(0) }; KEY_QUEUE_CAPACITY];

// IRQ12 is the sole producer; the kernel loop is the sole consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MouseEvent { pub dx: i8, pub dy: i8, pub buttons: u8 }
const MOUSE_QUEUE_CAPACITY: usize = 32;
static MOUSE_QUEUE: [AtomicUsize; MOUSE_QUEUE_CAPACITY] =
    [const { AtomicUsize::new(0) }; MOUSE_QUEUE_CAPACITY];
static MOUSE_QUEUE_HEAD: AtomicUsize = AtomicUsize::new(0);
static MOUSE_QUEUE_TAIL: AtomicUsize = AtomicUsize::new(0);
static MOUSE_PACKET_INDEX: AtomicU8 = AtomicU8::new(0);
static MOUSE_PACKET: [AtomicU8; 3] = [const { AtomicU8::new(0) }; 3];
static MOUSE_AVAILABLE: AtomicU8 = AtomicU8::new(0);

fn enqueue_mouse(event: MouseEvent) {
    let head = MOUSE_QUEUE_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) % MOUSE_QUEUE_CAPACITY;
    if next == MOUSE_QUEUE_TAIL.load(Ordering::Acquire) { return; }
    let packed = event.dx as u8 as usize | ((event.dy as u8 as usize) << 8) | ((event.buttons as usize) << 16);
    MOUSE_QUEUE[head].store(packed, Ordering::Relaxed);
    MOUSE_QUEUE_HEAD.store(next, Ordering::Release);
}
pub fn pop_mouse_event() -> Option<MouseEvent> {
    let tail = MOUSE_QUEUE_TAIL.load(Ordering::Relaxed);
    if tail == MOUSE_QUEUE_HEAD.load(Ordering::Acquire) { return None; }
    let packed = MOUSE_QUEUE[tail].load(Ordering::Relaxed);
    MOUSE_QUEUE_TAIL.store((tail + 1) % MOUSE_QUEUE_CAPACITY, Ordering::Release);
    Some(MouseEvent { dx: packed as u8 as i8, dy: (packed >> 8) as u8 as i8, buttons: (packed >> 16) as u8 })
}
static QUEUE_HEAD: AtomicUsize = AtomicUsize::new(0);
static QUEUE_TAIL: AtomicUsize = AtomicUsize::new(0);

fn enqueue_key(key: u8) {
    let head = QUEUE_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) % KEY_QUEUE_CAPACITY;
    if next == QUEUE_TAIL.load(Ordering::Acquire) {
        return;
    }
    KEY_QUEUE[head].store(key, Ordering::Relaxed);
    QUEUE_HEAD.store(next, Ordering::Release);
}

pub(crate) fn pop_key() -> Option<u8> {
    let tail = QUEUE_TAIL.load(Ordering::Relaxed);
    if tail == QUEUE_HEAD.load(Ordering::Acquire) {
        return None;
    }
    let key = KEY_QUEUE[tail].load(Ordering::Relaxed);
    QUEUE_TAIL.store((tail + 1) % KEY_QUEUE_CAPACITY, Ordering::Release);
    Some(key)
}


global_asm!(
    r#"
    .macro IRQ_STUB n
    .global everest_irq_\n
    everest_irq_\n:
        push 0
        push \n
        jmp everest_irq_common
    .endm
    IRQ_STUB 32
    IRQ_STUB 33
    IRQ_STUB 34
    IRQ_STUB 35
    IRQ_STUB 36
    IRQ_STUB 37
    IRQ_STUB 38
    IRQ_STUB 39
    IRQ_STUB 40
    IRQ_STUB 41
    IRQ_STUB 42
    IRQ_STUB 43
    IRQ_STUB 44
    IRQ_STUB 45
    IRQ_STUB 46
    IRQ_STUB 47
    .global everest_irq_common
    everest_irq_common:
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
        mov rcx, [r12 + 120]
        and rsp, -16
        sub rsp, 32
        cld
        call everest_irq_dispatch
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
    fn everest_irq_32(); fn everest_irq_33(); fn everest_irq_34(); fn everest_irq_35();
    fn everest_irq_36(); fn everest_irq_37(); fn everest_irq_38(); fn everest_irq_39();
    fn everest_irq_40(); fn everest_irq_41(); fn everest_irq_42(); fn everest_irq_43();
    fn everest_irq_44(); fn everest_irq_45(); fn everest_irq_46(); fn everest_irq_47();
}
pub(crate) fn stubs() -> [usize; 16] {
    [everest_irq_32 as *const () as usize,everest_irq_33 as *const () as usize,everest_irq_34 as *const () as usize,everest_irq_35 as *const () as usize,
     everest_irq_36 as *const () as usize,everest_irq_37 as *const () as usize,everest_irq_38 as *const () as usize,everest_irq_39 as *const () as usize,
     everest_irq_40 as *const () as usize,everest_irq_41 as *const () as usize,everest_irq_42 as *const () as usize,everest_irq_43 as *const () as usize,
     everest_irq_44 as *const () as usize,everest_irq_45 as *const () as usize,everest_irq_46 as *const () as usize,everest_irq_47 as *const () as usize]
}
unsafe fn outb(port:u16,value:u8){ unsafe{asm!("out dx, al",in("dx")port,in("al")value,options(nomem,nostack,preserves_flags))}; }
unsafe fn inb(port:u16)->u8{let v:u8;unsafe{asm!("in al, dx",in("dx")port,out("al")v,options(nomem,nostack,preserves_flags))};v}
unsafe fn wait(){unsafe{outb(0x80,0)}}
unsafe fn wait_input_clear() -> bool {
    for _ in 0..100_000 { if unsafe { inb(0x64) } & 0x02 == 0 { return true; } }
    false
}
unsafe fn wait_output_full() -> bool {
    for _ in 0..100_000 { if unsafe { inb(0x64) } & 0x01 != 0 { return true; } }
    false
}
unsafe fn controller_command(command: u8) -> bool {
    if !unsafe { wait_input_clear() } { return false; }
    unsafe { outb(0x64, command); } true
}
unsafe fn read_controller_config() -> Option<u8> {
    if !unsafe { controller_command(0x20) } || !unsafe { wait_output_full() } { return None; }
    Some(unsafe { inb(0x60) })
}
unsafe fn write_controller_config(value: u8) -> bool {
    if !unsafe { controller_command(0x60) } || !unsafe { wait_input_clear() } { return false; }
    unsafe { outb(0x60, value); } true
}
unsafe fn mouse_command(value: u8) -> bool {
    if !unsafe { wait_input_clear() } { return false; }
    unsafe { outb(0x64, 0xD4); }
    if !unsafe { wait_input_clear() } { return false; }
    unsafe { outb(0x60, value); }
    if !unsafe { wait_output_full() } { return false; }
    unsafe { inb(0x60) == 0xFA }
}
unsafe fn initialize_mouse() -> bool {
    if !unsafe { controller_command(0xA8) } { return false; }
    let Some(mut config) = (unsafe { read_controller_config() }) else { return false; };
    config = (config | 0x02) & !0x20; // IRQ12 enabled, auxiliary clock enabled.
    if !unsafe { write_controller_config(config) } { return false; }
    if !unsafe { mouse_command(0xF6) } || !unsafe { mouse_command(0xF4) } { return false; }
    MOUSE_PACKET_INDEX.store(0, Ordering::Relaxed);
    MOUSE_AVAILABLE.store(1, Ordering::Release);
    true
}
fn handle_mouse_byte(byte: u8) {
    let index = MOUSE_PACKET_INDEX.load(Ordering::Relaxed) as usize;
    if index == 0 && byte & 0x08 == 0 { return; }
    MOUSE_PACKET[index].store(byte, Ordering::Relaxed);
    if index < 2 { MOUSE_PACKET_INDEX.store((index + 1) as u8, Ordering::Relaxed); return; }
    MOUSE_PACKET_INDEX.store(0, Ordering::Relaxed);
    let flags = MOUSE_PACKET[0].load(Ordering::Relaxed);
    if flags & 0xC0 != 0 { return; }
    let dx = MOUSE_PACKET[1].load(Ordering::Relaxed) as i8;
    let dy = -(MOUSE_PACKET[2].load(Ordering::Relaxed) as i8);
    enqueue_mouse(MouseEvent { dx, dy, buttons: flags & 0x07 });
}
/// Call with IF clear after loading the IDT. Controller waits are bounded.
pub unsafe fn initialize(){
    unsafe {
        outb(0x20,0x11); wait(); outb(0xA0,0x11); wait();
        outb(0x21,0x20); wait(); outb(0xA1,0x28); wait();
        outb(0x21,0x04); wait(); outb(0xA1,0x02); wait();
        outb(0x21,0x01); wait(); outb(0xA1,0x01); wait();
        outb(0x21,0xF8); outb(0xA1,0xFF);
        outb(0x43,0x36); outb(0x40,(11932u16 & 0xff) as u8); outb(0x40,(11932u16 >> 8) as u8);
        let _ = inb(0x60);
        if initialize_mouse() { outb(0x21,0xF8); outb(0xA1,0xEF); }
        else { outb(0x21,0xFC); outb(0xA1,0xFF); }
    }
}
pub fn ticks()->u64{TICKS.load(Ordering::Relaxed)}
pub fn uptime_seconds()->u64{ticks()/100}
pub fn last_key()->u8{LAST_KEY.load(Ordering::Relaxed)}
pub fn key_count()->u64{KEY_COUNT.load(Ordering::Relaxed)}
fn translate(scancode:u8)->u8{
    const ROW1:&[u8]=b"1234567890-=";
    const ROW2:&[u8]=b"qwertyuiop[]";
    const ROW3:&[u8]=b"asdfghjkl;'`";
    const ROW4:&[u8]=b"zxcvbnm,./";
    match scancode {
        0x02..=0x0D=>ROW1[(scancode-0x02) as usize],
        0x10..=0x1B=>ROW2[(scancode-0x10) as usize],
        0x1E..=0x29=>ROW3[(scancode-0x1E) as usize],
        0x2C..=0x35=>ROW4[(scancode-0x2C) as usize],
        0x0E=>8,0x39=>b' ',0x1C=>b'\n',0x48=>0x81,0x50=>0x82,_=>0
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn everest_irq_dispatch(vector:u64){
    unsafe {
        match vector {
            32=>{TICKS.fetch_add(1,Ordering::Relaxed);},
            33=>{
                let code=inb(0x60);
                match code {
                    0x2A | 0x36 => SHIFT_PRESSED.store(1, Ordering::Relaxed),
                    0xAA | 0xB6 => SHIFT_PRESSED.store(0, Ordering::Relaxed),
                    _ if code & 0x80 == 0 => {
                        let mut ch=translate(code);
                        if SHIFT_PRESSED.load(Ordering::Relaxed) != 0 && ch.is_ascii_lowercase() {
                            ch=ch.to_ascii_uppercase();
                        }
                        if ch!=0 {LAST_KEY.store(ch,Ordering::Relaxed);KEY_COUNT.fetch_add(1,Ordering::Relaxed);enqueue_key(ch);}
                    }
                    _ => {}
                }
            },
            44=>{if MOUSE_AVAILABLE.load(Ordering::Acquire) != 0 { handle_mouse_byte(inb(0x60)); }},
            _=>{}
        }
        if vector>=40 {outb(0xA0,0x20);}
        outb(0x20,0x20);
    }
}

#[cfg(test)]
mod tests {
    use super::translate;

    #[test]
    fn translates_every_key_in_each_supported_row_without_panicking() {
        for scancode in 0x02..=0x0D {
            assert_ne!(translate(scancode), 0);
        }
        for scancode in 0x10..=0x1B {
            assert_ne!(translate(scancode), 0);
        }
        for scancode in 0x1E..=0x29 {
            assert_ne!(translate(scancode), 0);
        }
        for scancode in 0x2C..=0x35 {
            assert_ne!(translate(scancode), 0);
        }
    }

    #[test]
    fn translates_top_left_and_common_control_keys() {
        assert_eq!(translate(0x29), 0x60);
        assert_eq!(translate(0x39), b' ');
        assert_eq!(translate(0x0E), 8);
        assert_eq!(translate(0x1C), b'\n');
        assert_eq!(translate(0x48), 0x81); // extended Up Arrow
        assert_eq!(translate(0x50), 0x82); // extended Down Arrow
        assert_eq!(translate(0x01), 0);
    }
}
