#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]

use bootloader_x86_64_common::logger::LockedLogger;
use conquer_once::spin::OnceCell;
use core::panic::PanicInfo;
use vga_buffer::{Color, background_paint};
pub(crate) static LOGGER: OnceCell<LockedLogger> = OnceCell::uninit();
use bootloader_api::config::Mapping;
use bootloader_api::info::FrameBufferInfo;
use bootloader_api::BootloaderConfig;
use x86_64::VirtAddr;
mod gdt;
mod interrupts;
mod memory;
mod vga_buffer;

// ask the bootloader to map the whole physical memory into the virtual
// address space, so the kernel can reach the page tables
pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

bootloader_api::entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

pub(crate) fn init_logger(buffer: &'static mut [u8], info: FrameBufferInfo) {
    let logger = LOGGER.get_or_init(move || LockedLogger::new(buffer, info, true, false));
    log::set_logger(logger).expect("Logger already set");
    log::set_max_level(log::LevelFilter::Trace);
}

fn kernel_main(boot_info: &'static mut bootloader_api::BootInfo) -> ! {
    // free the doubly wrapped framebuffer from the boot info struct
    let frame_buffer_optional = &mut boot_info.framebuffer;

    // framebuffer from the FFI-safe abstraction provided by bootloader_api
    let frame_buffer = frame_buffer_optional.as_mut().unwrap();


    // extract the framebuffer info and, to satisfy the borrow checker
    let frame_buffer_info = frame_buffer.info();

    let background_color = Color::LightRed;

    background_paint(frame_buffer, background_color);

    let raw_frame_buffer = frame_buffer.buffer_mut();

    init_logger(raw_frame_buffer, frame_buffer_info);

    log::info!("Hello world!");

    gdt::init();
    interrupts::init_idt();

    // trigger a breakpoint exception to check that the IDT works
    x86_64::instructions::interrupts::int3();

    log::info!("It did not crash!");

    // --- paging ---
    let phys_mem_offset = VirtAddr::new(
        boot_info
            .physical_memory_offset
            .into_option()
            .expect("bootloader did not map physical memory"),
    );
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_regions) };

    // translate a few addresses to see the page tables in action
    {
        use x86_64::structures::paging::Translate;

        let stack_var = 0u64;
        let addresses = [
            phys_mem_offset.as_u64(),          // -> physical address 0
            kernel_main as *const () as u64,   // kernel code
            &stack_var as *const u64 as u64,   // kernel stack
        ];
        for &address in &addresses {
            let virt = VirtAddr::new(address);
            let phys = mapper.translate_addr(virt);
            log::info!("{:?} -> {:?}", virt, phys);
        }
    }

    // map a fresh page and write through it to prove mappings work
    {
        use x86_64::structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags};

        let page: Page = Page::containing_address(VirtAddr::new(0x_4444_4444_0000));
        let frame = frame_allocator.allocate_frame().expect("no usable frame");
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            mapper
                .map_to(page, frame, flags, &mut frame_allocator)
                .expect("map_to failed")
                .flush();
        }

        let ptr: *mut u64 = page.start_address().as_mut_ptr();
        unsafe { ptr.write_volatile(0xf021_f077_f065_f04e) };
        let readback = unsafe { ptr.read_volatile() };
        log::info!(
            "mapped {:?} -> {:?}, readback: {:#x}",
            page.start_address(),
            frame.start_address(),
            readback
        );
    }

    interrupts::init_pics();

    // halt the CPU until the next interrupt instead of busy-looping
    loop {
        x86_64::instructions::hlt();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}






#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) {
    log::info!("Running {} tests", tests.len());
    for test in tests {
        test();
    }
}
