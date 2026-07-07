#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]

extern crate alloc;

use bootloader_api::config::Mapping;
use bootloader_api::BootloaderConfig;
use core::panic::PanicInfo;
use x86_64::VirtAddr;

mod allocator;
mod console;
mod gdt;
mod interrupts;
mod memory;
mod sb16;
mod speaker;
mod task;
mod vga_buffer;

use task::{Task, executor::Executor};

// ask the bootloader to map the whole physical memory into the virtual
// address space, so the kernel can reach the page tables
pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

bootloader_api::entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut bootloader_api::BootInfo) -> ! {
    // console on the framebuffer: gives us print!/println! and log output
    let frame_buffer = boot_info.framebuffer.as_mut().unwrap();
    let frame_buffer_info = frame_buffer.info();
    console::init(frame_buffer.buffer_mut(), frame_buffer_info);

    gdt::init();
    interrupts::init_idt();

    // paging + heap
    let phys_mem_offset = VirtAddr::new(
        boot_info
            .physical_memory_offset
            .into_option()
            .expect("bootloader did not map physical memory"),
    );
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_regions) };
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    // sound card (before enabling interrupts: its IRQ starts right away)
    match sb16::init(&mut frame_allocator, phys_mem_offset) {
        Ok(()) => log::info!("sb16: initialized"),
        Err(e) => log::warn!("sb16: not available ({})", e),
    }

    interrupts::init_pics();

    println!("rust_freestanding kernel");
    println!("type 'help' for a list of commands");
    println!();

    // the executor takes over as the kernel's idle loop: it polls woken
    // tasks and halts the CPU when there is nothing to do
    let mut executor = Executor::new();
    executor.spawn(Task::new(task::shell::run()));
    executor.spawn(Task::new(sb16::run_synth()));
    executor.run();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}

#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) {
    log::info!("Running {} tests", tests.len());
    for test in tests {
        test();
    }
}
