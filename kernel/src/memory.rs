//! Early physical frame allocator for x86-64 UEFI memory maps.
//!
//! Only UEFI ConventionalMemory (type 7) is initially eligible. All ranges use
//! half-open physical byte addresses. The allocator is deliberately fixed-capacity
//! and heap-free so it can run before the kernel heap exists.

use core::{mem, ptr};

pub const PAGE_SIZE: u64 = 4096;
pub const UEFI_MEMORY_DESCRIPTOR_VERSION: u32 = 1;
const UEFI_CONVENTIONAL_MEMORY: u32 = 7;
const MAX_MEMORY_MAP_BYTES: usize = 64 * 1024 * 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
}

impl MemoryRegion {
    pub const EMPTY: Self = Self { start: 0, end: 0 };

    pub const fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }
}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AllocError {
    InvalidMemoryMap,
    InvalidRange,
    CapacityExceeded,
    OutOfMemory,
    UnalignedFrame,
    FrameNotManaged,
    DoubleFree,
}

/// Parse only conventional memory into page-aligned, half-open regions.
///
/// # Safety
/// `map` must reference `map_size` readable bytes for the duration of this call.
pub unsafe fn parse_usable_regions(
    map: *const u8,
    map_size: usize,
    descriptor_size: usize,
    descriptor_version: u32,
    output: &mut [MemoryRegion],
) -> Result<usize, AllocError> {
    if map.is_null()
        || map_size == 0
        || map_size > MAX_MEMORY_MAP_BYTES
        || descriptor_version != UEFI_MEMORY_DESCRIPTOR_VERSION
        || descriptor_size < mem::size_of::<UefiMemoryDescriptor>()
        || map_size % descriptor_size != 0
    {
        return Err(AllocError::InvalidMemoryMap);
    }

    let mut count = 0;
    for index in 0..map_size / descriptor_size {
        let offset = index.checked_mul(descriptor_size).ok_or(AllocError::InvalidMemoryMap)?;
        // SAFETY: caller guarantees the full buffer; stride validation guarantees
        // each descriptor's fixed prefix is within the buffer.
        let descriptor = unsafe {
            ptr::read_unaligned(map.add(offset).cast::<UefiMemoryDescriptor>())
        };
        if descriptor.memory_type != UEFI_CONVENTIONAL_MEMORY || descriptor.page_count == 0 {
            continue;
        }
        let bytes = descriptor.page_count.checked_mul(PAGE_SIZE).ok_or(AllocError::InvalidRange)?;
        let end = descriptor.physical_start.checked_add(bytes).ok_or(AllocError::InvalidRange)?;
        let start = align_up(descriptor.physical_start).ok_or(AllocError::InvalidRange)?;
        let end = end & !(PAGE_SIZE - 1);
        if start >= end {
            continue;
        }
        if count == output.len() {
            return Err(AllocError::CapacityExceeded);
        }
        output[count] = MemoryRegion::new(start, end);
        count += 1;
    }
    if count == 0 {
        return Err(AllocError::OutOfMemory);
    }
    Ok(count)
}

fn align_up(value: u64) -> Option<u64> {
    value.checked_add(PAGE_SIZE - 1).map(|v| v & !(PAGE_SIZE - 1))
}

/// A heap-free allocator. `free` and `managed` are caller-owned fixed-capacity
/// arrays; metadata must live in memory reserved from the allocator itself.
pub struct FrameAllocator<'a> {
    free: &'a mut [MemoryRegion],
    free_len: usize,
    managed: &'a mut [MemoryRegion],
    managed_len: usize,
}

impl<'a> FrameAllocator<'a> {
    pub fn new(
        usable: &[MemoryRegion],
        reserved: &[MemoryRegion],
        free_storage: &'a mut [MemoryRegion],
        managed_storage: &'a mut [MemoryRegion],
    ) -> Result<Self, AllocError> {
        let mut allocator = Self {
            free: free_storage,
            free_len: 0,
            managed: managed_storage,
            managed_len: 0,
        };
        for region in usable {
            if region.start >= region.end {
                return Err(AllocError::InvalidRange);
            }
            let start = align_up(region.start).ok_or(AllocError::InvalidRange)?;
            let end = region.end & !(PAGE_SIZE - 1);
            if start >= end {
                continue;
            }
            allocator.push_managed(MemoryRegion::new(start, end))?;
        }
        allocator.sort_and_merge_managed()?;
        for i in 0..allocator.managed_len {
            let region = allocator.managed[i];
            allocator.push_free(region)?;
        }
        for reservation in reserved {
            if reservation.start >= reservation.end {
                return Err(AllocError::InvalidRange);
            }
            allocator.subtract_from_free(*reservation)?;
            allocator.subtract_from_managed(*reservation)?;
        }
        allocator.sort_and_merge_free()?;
        allocator.sort_and_merge_managed()?;
        if allocator.free_len == 0 {
            return Err(AllocError::OutOfMemory);
        }
        Ok(allocator)
    }

    pub fn allocate(&mut self) -> Result<u64, AllocError> {
        if self.free_len == 0 {
            return Err(AllocError::OutOfMemory);
        }
        let frame = self.free[0].start;
        self.free[0].start = self.free[0].start.checked_add(PAGE_SIZE).ok_or(AllocError::InvalidRange)?;
        if self.free[0].start >= self.free[0].end {
            self.remove_free(0);
        }
        Ok(frame)
    }

    pub fn deallocate(&mut self, frame: u64) -> Result<(), AllocError> {
        if frame % PAGE_SIZE != 0 {
            return Err(AllocError::UnalignedFrame);
        }
        let end = frame.checked_add(PAGE_SIZE).ok_or(AllocError::InvalidRange)?;
        if !self.managed[..self.managed_len].iter().any(|r| frame >= r.start && end <= r.end) {
            return Err(AllocError::FrameNotManaged);
        }
        if self.free[..self.free_len].iter().any(|r| frame < r.end && end > r.start) {
            return Err(AllocError::DoubleFree);
        }
        self.insert_free(MemoryRegion::new(frame, end))?;
        self.sort_and_merge_free()?;
        Ok(())
    }

    pub fn free_frame_count(&self) -> u64 {
        self.free[..self.free_len].iter().map(|r| (r.end - r.start) / PAGE_SIZE).sum()
    }

    fn push_managed(&mut self, region: MemoryRegion) -> Result<(), AllocError> {
        if self.managed_len == self.managed.len() { return Err(AllocError::CapacityExceeded); }
        self.managed[self.managed_len] = region;
        self.managed_len += 1;
        Ok(())
    }

    fn push_free(&mut self, region: MemoryRegion) -> Result<(), AllocError> {
        if self.free_len == self.free.len() { return Err(AllocError::CapacityExceeded); }
        self.free[self.free_len] = region;
        self.free_len += 1;
        Ok(())
    }

    fn insert_free(&mut self, region: MemoryRegion) -> Result<(), AllocError> {
        if self.free_len == self.free.len() { return Err(AllocError::CapacityExceeded); }
        let mut index = 0;
        while index < self.free_len && self.free[index].start < region.start { index += 1; }
        for i in (index..self.free_len).rev() { self.free[i + 1] = self.free[i]; }
        self.free[index] = region;
        self.free_len += 1;
        Ok(())
    }

    fn remove_free(&mut self, index: usize) {
        for i in index..self.free_len - 1 { self.free[i] = self.free[i + 1]; }
        self.free_len -= 1;
    }

    fn subtract_from_free(&mut self, cut: MemoryRegion) -> Result<(), AllocError> {
        let mut i = 0;
        while i < self.free_len {
            let region = self.free[i];
            let start = region.start.max(cut.start);
            let end = region.end.min(cut.end);
            if start >= end { i += 1; continue; }
            if start == region.start && end == region.end {
                self.remove_free(i);
            } else if start == region.start {
                self.free[i].start = end;
                i += 1;
            } else if end == region.end {
                self.free[i].end = start;
                i += 1;
            } else {
                if self.free_len == self.free.len() { return Err(AllocError::CapacityExceeded); }
                for j in (i + 1..self.free_len).rev() { self.free[j + 1] = self.free[j]; }
                self.free[i].end = start;
                self.free[i + 1] = MemoryRegion::new(end, region.end);
                self.free_len += 1;
                i += 2;
            }
        }
        Ok(())
    }

    fn subtract_from_managed(&mut self, cut: MemoryRegion) -> Result<(), AllocError> {
        let mut i = 0;
        while i < self.managed_len {
            let region = self.managed[i];
            let start = region.start.max(cut.start);
            let end = region.end.min(cut.end);
            if start >= end { i += 1; continue; }
            if start == region.start && end == region.end {
                for j in i..self.managed_len - 1 { self.managed[j] = self.managed[j + 1]; }
                self.managed_len -= 1;
            } else if start == region.start {
                self.managed[i].start = end;
                i += 1;
            } else if end == region.end {
                self.managed[i].end = start;
                i += 1;
            } else {
                if self.managed_len == self.managed.len() { return Err(AllocError::CapacityExceeded); }
                for j in (i + 1..self.managed_len).rev() { self.managed[j + 1] = self.managed[j]; }
                self.managed[i].end = start;
                self.managed[i + 1] = MemoryRegion::new(end, region.end);
                self.managed_len += 1;
                i += 2;
            }
        }
        Ok(())
    }

    fn sort_and_merge_free(&mut self) -> Result<(), AllocError> {
        self.free[..self.free_len].sort_unstable_by_key(|r| r.start);
        let mut out = 0;
        for i in 0..self.free_len {
            let current = self.free[i];
            if current.start >= current.end { return Err(AllocError::InvalidRange); }
            if out > 0 && self.free[out - 1].end >= current.start {
                if self.free[out - 1].end < current.end { self.free[out - 1].end = current.end; }
            } else {
                self.free[out] = current;
                out += 1;
            }
        }
        self.free_len = out;
        Ok(())
    }

    fn sort_and_merge_managed(&mut self) -> Result<(), AllocError> {
        self.managed[..self.managed_len].sort_unstable_by_key(|r| r.start);
        let mut out = 0;
        for i in 0..self.managed_len {
            let current = self.managed[i];
            if current.start >= current.end { return Err(AllocError::InvalidRange); }
            if out > 0 && self.managed[out - 1].end >= current.start {
                if self.managed[out - 1].end < current.end { self.managed[out - 1].end = current.end; }
            } else {
                self.managed[out] = current;
                out += 1;
            }
        }
        self.managed_len = out;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allocator<'a>(
        free: &'a mut [MemoryRegion; 16],
        managed: &'a mut [MemoryRegion; 16],
        reserved: &[MemoryRegion],
    ) -> FrameAllocator<'a> {
        FrameAllocator::new(&[MemoryRegion::new(0x1000, 0x9000)], reserved, free, managed).unwrap()
    }

    #[test]
    fn allocations_are_page_aligned_and_unique() {
        let (mut free, mut managed) = ([MemoryRegion::EMPTY; 16], [MemoryRegion::EMPTY; 16]);
        let mut a = allocator(&mut free, &mut managed, &[]);
        let first = a.allocate().unwrap();
        let second = a.allocate().unwrap();
        assert_eq!(first % PAGE_SIZE, 0);
        assert_eq!(second % PAGE_SIZE, 0);
        assert_ne!(first, second);
    }

    #[test]
    fn reports_exhaustion() {
        let (mut free, mut managed) = ([MemoryRegion::EMPTY; 16], [MemoryRegion::EMPTY; 16]);
        let mut a = FrameAllocator::new(&[MemoryRegion::new(0x1000, 0x2000)], &[], &mut free, &mut managed).unwrap();
        assert_eq!(a.allocate(), Ok(0x1000));
        assert_eq!(a.allocate(), Err(AllocError::OutOfMemory));
    }

    #[test]
    fn rejects_invalid_ranges_and_unaligned_free() {
        let (mut free, mut managed) = ([MemoryRegion::EMPTY; 16], [MemoryRegion::EMPTY; 16]);
        assert_eq!(FrameAllocator::new(&[MemoryRegion::new(0x3000, 0x2000)], &[], &mut free, &mut managed).err(), Some(AllocError::InvalidRange));
        let mut a = allocator(&mut free, &mut managed, &[]);
        assert_eq!(a.deallocate(0x1001), Err(AllocError::UnalignedFrame));
        assert_eq!(a.deallocate(0xA000), Err(AllocError::FrameNotManaged));
    }

    #[test]
    fn rejects_double_free_and_reuses_returned_frame() {
        let (mut free, mut managed) = ([MemoryRegion::EMPTY; 16], [MemoryRegion::EMPTY; 16]);
        let mut a = allocator(&mut free, &mut managed, &[]);
        let frame = a.allocate().unwrap();
        a.deallocate(frame).unwrap();
        assert_eq!(a.deallocate(frame), Err(AllocError::DoubleFree));
        assert_eq!(a.allocate(), Ok(frame));
    }

    #[test]
    fn reservations_split_regions_and_are_never_allocated() {
        let (mut free, mut managed) = ([MemoryRegion::EMPTY; 16], [MemoryRegion::EMPTY; 16]);
        let mut a = allocator(&mut free, &mut managed, &[MemoryRegion::new(0x3000, 0x5000)]);
        for _ in 0..6 {
            let frame = a.allocate().unwrap();
            assert!(!(0x3000..0x5000).contains(&frame));
        }
        assert_eq!(a.allocate(), Err(AllocError::OutOfMemory));
    }

    #[test]
    fn parses_only_conventional_uefi_descriptors() {
        let descriptors = [
            UefiMemoryDescriptor { memory_type: 7, _padding: 0, physical_start: 0x1000, _virtual_start: 0, page_count: 3, _attributes: 0 },
            UefiMemoryDescriptor { memory_type: 2, _padding: 0, physical_start: 0x9000, _virtual_start: 0, page_count: 4, _attributes: 0 },
        ];
        let bytes = unsafe {
            core::slice::from_raw_parts(descriptors.as_ptr().cast::<u8>(), core::mem::size_of_val(&descriptors))
        };
        let mut out = [MemoryRegion::EMPTY; 4];
        let count = unsafe { parse_usable_regions(bytes.as_ptr(), bytes.len(), mem::size_of::<UefiMemoryDescriptor>(), 1, &mut out) }.unwrap();
        assert_eq!(count, 1);
        assert_eq!(out[0], MemoryRegion::new(0x1000, 0x4000));
    }
}
