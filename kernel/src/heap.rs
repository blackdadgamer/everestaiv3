//! Frame-backed early global allocator.
//!
//! Each allocation owns one physical 4 KiB frame. This deliberately simple
//! allocator is bounded to one-page objects until a virtual heap mapper exists.
use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use crate::memory::{FrameAllocator, PAGE_SIZE};

static FRAMES: AtomicPtr<FrameAllocator<'static>> = AtomicPtr::new(ptr::null_mut());
static LOCK: AtomicBool = AtomicBool::new(false);

pub struct FrameHeap;
#[global_allocator]
static GLOBAL: FrameHeap = FrameHeap;

pub fn initialize(allocator: &mut FrameAllocator<'_>) {
    FRAMES.store(allocator as *mut FrameAllocator<'_> as *mut FrameAllocator<'static>, Ordering::Release);
}
struct Guard;
impl Guard {
    fn acquire() -> Self {
        while LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for Guard {
    fn drop(&mut self) { LOCK.store(false, Ordering::Release); }
}
unsafe impl GlobalAlloc for FrameHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() > PAGE_SIZE as usize || layout.align() > PAGE_SIZE as usize { return ptr::null_mut(); }
        let _guard = Guard::acquire();
        let allocator = FRAMES.load(Ordering::Acquire);
        if allocator.is_null() { return ptr::null_mut(); }
        let result = unsafe { (&mut *allocator).allocate() };
        match result {
            Ok(frame) if frame < 4 * 1024 * 1024 * 1024 && frame % PAGE_SIZE == 0 => frame as *mut u8,
            Ok(frame) => { let _ = unsafe { (&mut *allocator).deallocate(frame) }; ptr::null_mut() },
            Err(_) => ptr::null_mut(),
        }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, _layout: Layout) {
        if pointer.is_null() { return; }
        let _guard = Guard::acquire();
        let allocator = FRAMES.load(Ordering::Acquire);
        if !allocator.is_null() {
            let _ = unsafe { (&mut *allocator).deallocate(pointer as u64) };
        }
    }
}
