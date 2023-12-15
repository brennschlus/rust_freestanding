#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]

use bootloader_x86_64_common::logger::LockedLogger;
use conquer_once::spin::OnceCell;
use core::panic::PanicInfo;
use vga_buffer::{Color, background_paint};
pub(crate) static LOGGER: OnceCell<LockedLogger> = OnceCell::uninit();
use bootloader_api::info::FrameBufferInfo;
mod vga_buffer;

bootloader_api::entry_point!(kernel_main);

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

    loop {}
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
