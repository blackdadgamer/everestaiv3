//! Legacy 8259 PIC, PIT clock and PS/2 keyboard interrupt support.
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use core::arch::{asm, global_asm};

static TICKS: AtomicU64 = AtomicU64::new(0);
static LAST_KEY: AtomicU8 = AtomicU8::new(0);
static KEY_COUNT: AtomicU64 = AtomicU64::new(0);


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
/// Call with IF clear after loading the IDT.
pub unsafe fn initialize(){
    unsafe {
        // Remap master/slave PIC vectors to 0x20 and 0x28.
        outb(0x20,0x11); wait(); outb(0xA0,0x11); wait();
        outb(0x21,0x20); wait(); outb(0xA1,0x28); wait();
        outb(0x21,0x04); wait(); outb(0xA1,0x02); wait();
        outb(0x21,0x01); wait(); outb(0xA1,0x01); wait();
        // Unmask PIT IRQ0 and keyboard IRQ1 only.
        outb(0x21,0xFC); outb(0xA1,0xFF);
        // PIT channel 0, square wave, approximately 100 Hz.
        outb(0x43,0x36); outb(0x40,(11932u16 & 0xff) as u8); outb(0x40,(11932u16 >> 8) as u8);
        // Enable keyboard IRQ; discard stale output byte if present.
        let _ = inb(0x60);
    }
}
pub fn ticks()->u64{TICKS.load(Ordering::Relaxed)}
pub fn uptime_seconds()->u64{ticks()/100}
pub fn last_key()->u8{LAST_KEY.load(Ordering::Relaxed)}
pub fn key_count()->u64{KEY_COUNT.load(Ordering::Relaxed)}
fn translate(scancode:u8)->u8{
    const ROW1:&[u8]=b"1234567890-=";
    const ROW2:&[u8]=b"qwertyuiop[]";
    const ROW3:&[u8]=b"asdfghjkl;'";
    const ROW4:&[u8]=b"zxcvbnm,./";
    match scancode {
        0x02..=0x0D=>ROW1[(scancode-0x02) as usize],
        0x10..=0x1B=>ROW2[(scancode-0x10) as usize],
        0x1E..=0x29=>ROW3[(scancode-0x1E) as usize],
        0x2C..=0x35=>ROW4[(scancode-0x2C) as usize],
        0x39=>b' ',0x1C=>b'\n',_=>0
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn everest_irq_dispatch(vector:u64){
    unsafe {
        match vector {
            32=>{TICKS.fetch_add(1,Ordering::Relaxed);},
            33=>{let code=inb(0x60); if code & 0x80==0 {let ch=translate(code);if ch!=0{LAST_KEY.store(ch,Ordering::Relaxed);KEY_COUNT.fetch_add(1,Ordering::Relaxed);}}},
            _=>{}
        }
        if vector>=40 {outb(0xA0,0x20);}
        outb(0x20,0x20);
    }
}
