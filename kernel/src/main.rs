#![no_std]
#![no_main]

use bootloader_x86_64_common::logger::LockedLogger;
use conquer_once::spin::OnceCell;
use core::panic::PanicInfo;
use vga_buffer::Color;
pub(crate) static LOGGER: OnceCell<LockedLogger> = OnceCell::uninit();
use bootloader_api::info::{FrameBuffer, FrameBufferInfo};
mod logger;
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

    // free the wrapped framebuffer from the FFI-safe abstraction provided by bootloader_api
    let frame_buffer_option = frame_buffer_optional.as_mut();

    // unwrap the framebuffer
    let frame_buffer_struct = frame_buffer_option.unwrap();

    // extract the framebuffer info and, to satisfy the borrow checker, clone it
    let frame_buffer_info = frame_buffer_struct.info().clone();

    let background_color = Color::LightRed;

    background_paint(frame_buffer_struct, background_color);

    let raw_frame_buffer = frame_buffer_struct.buffer_mut();

    init_logger(raw_frame_buffer, frame_buffer_info);



    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

fn background_paint(frame_buffer_struct: &mut FrameBuffer, color: Color) {
    // extract the framebuffer info and, to satisfy the borrow checker, clone it
    let frame_buffer_info = frame_buffer_struct.info().clone();

    // get the framebuffer's mutable raw byte slice
    let raw_frame_buffer = frame_buffer_struct.buffer_mut();

    let width = frame_buffer_info.width;
    let height = frame_buffer_info.height;
    let bytes_per_pixel = frame_buffer_info.bytes_per_pixel;

    let color_value = color.color_value();

    // Split the color value into four bytes
    let blue = (color_value & 0xFF) as u8;
    let green = ((color_value >> 8) & 0xFF) as u8;
    let red = ((color_value >> 16) & 0xFF) as u8;
    let alpha = 0xFF;

    // Paint background color
    for bytes in raw_frame_buffer.chunks_exact_mut(bytes_per_pixel) {
        bytes[0] = blue; // blue
        bytes[1] = green; // green
        bytes[2] = red; // red
        bytes[3] = alpha; // alpha
    }

    // Loop over the rows of the screen
    for row in 0..height {
        // Loop over the columns of the screen
        for col in 0..width {
            // Calculate the index of the first byte of the current pixel
            let index = (row * width + col) * bytes_per_pixel;

            // Check if the current position is on the edge of the screen
            if row < 3 || row > height - 4 || col < 3 || col > width - 4 {
                // Set the bytes to 0xFF for white color
                raw_frame_buffer[index] = 0xFF; // blue
                raw_frame_buffer[index + 1] = 0xFF; // green
                raw_frame_buffer[index + 2] = 0xFF; // red
                raw_frame_buffer[index + 3] = 0xFF; // alpha
            }
        }
    }
}
