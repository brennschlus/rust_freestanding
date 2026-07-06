use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{
    FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

/// Create an `OffsetPageTable` for the currently active level 4 table.
///
/// # Safety
/// The caller must guarantee that the complete physical memory is mapped
/// at `physical_memory_offset` (bootloader config `mappings.physical_memory`)
/// and that this function is only called once, to avoid aliasing `&mut`
/// references to the page tables.
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = unsafe { active_level_4_table(physical_memory_offset) };
    unsafe { OffsetPageTable::new(level_4_table, physical_memory_offset) }
}

/// Return a mutable reference to the active level 4 page table.
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    // CR3 holds the physical address of the level 4 table
    let (level_4_table_frame, _flags) = Cr3::read();

    // we cannot access physical addresses directly, but every physical
    // address is mapped at `physical_memory_offset + phys`
    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    unsafe { &mut *page_table_ptr }
}

/// A frame allocator that hands out usable frames from the bootloader's
/// memory map.
pub struct BootInfoFrameAllocator {
    memory_regions: &'static MemoryRegions,
    next: usize,
}

impl BootInfoFrameAllocator {
    /// # Safety
    /// The caller must guarantee that the passed memory map is valid, i.e.
    /// all frames marked `Usable` in it are really unused.
    pub unsafe fn init(memory_regions: &'static MemoryRegions) -> Self {
        BootInfoFrameAllocator {
            memory_regions,
            next: 0,
        }
    }

    /// An iterator over all frames the memory map marks as usable.
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> + '_ {
        self.memory_regions
            .iter()
            .filter(|region| region.kind == MemoryRegionKind::Usable)
            // usable regions are page aligned, so step over whole frames
            .flat_map(|region| (region.start..region.end).step_by(4096))
            .map(|frame_start| PhysFrame::containing_address(PhysAddr::new(frame_start)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        // recomputing the iterator every time is slow but simple; a real
        // allocator would keep its own state
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}
