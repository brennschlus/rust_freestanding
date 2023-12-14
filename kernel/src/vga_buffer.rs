use bootloader_api::info::FrameBuffer;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

impl Color {
    pub fn color_value(&self) -> u32 {
        match self {
            Color::Black => 0x000000,
            Color::Blue => 0x0000AA,
            Color::Green => 0x00AA00,
            Color::Cyan => 0x00AAAA,
            Color::Red => 0xAA0000,
            Color::Magenta => 0xAA00AA,
            Color::Brown => 0xAA5500,
            Color::LightGray => 0xAAAAAA,
            Color::DarkGray => 0x555555,
            Color::LightBlue => 0x5555FF,
            Color::LightGreen => 0x55FF55,
            Color::LightCyan => 0x55FFFF,
            Color::LightRed => 0xFF5555,
            Color::Pink => 0xFF55FF,
            Color::Yellow => 0xFFFF55,
            Color::White => 0xFFFFFF,
        }
    }
}

pub fn background_paint(frame_buffer_struct: &mut FrameBuffer, color: Color) {
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