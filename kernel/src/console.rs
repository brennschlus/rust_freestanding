use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use conquer_once::spin::OnceCell;
use core::fmt::{self, Write};
use font8x8::UnicodeFonts;
use spinning_top::Spinlock;

use crate::vga_buffer::Color;

// 8x8 font scaled up 2x -> 16x16 px cells, 80x45 chars on 1280x720
const SCALE: usize = 2;
const CHAR_W: usize = 8 * SCALE;
const CHAR_H: usize = 8 * SCALE;

static CONSOLE: OnceCell<Spinlock<Console>> = OnceCell::uninit();

/// Set up the global console on the framebuffer and route the `log`
/// macros through it.
pub fn init(buffer: &'static mut [u8], info: FrameBufferInfo) {
    let mut console = Console {
        buffer,
        info,
        col: 0,
        row: 0,
        fg: Color::LightGray.color_value(),
    };
    console.clear();
    CONSOLE.init_once(|| Spinlock::new(console));

    log::set_logger(&ConsoleLogger).expect("logger already set");
    log::set_max_level(log::LevelFilter::Info);
}

/// Clear the screen (used by the shell's `clear` command).
pub fn clear() {
    if let Ok(console) = CONSOLE.try_get() {
        x86_64::instructions::interrupts::without_interrupts(|| console.lock().clear());
    }
}

pub struct Console {
    buffer: &'static mut [u8],
    info: FrameBufferInfo,
    col: usize,
    row: usize,
    fg: u32,
}

impl Console {
    fn width_chars(&self) -> usize {
        self.info.width / CHAR_W
    }

    fn height_chars(&self) -> usize {
        self.info.height / CHAR_H
    }

    fn clear(&mut self) {
        self.buffer.fill(0);
        self.col = 0;
        self.row = 0;
    }

    fn set_pixel(&mut self, x: usize, y: usize, color: u32) {
        let bytes_per_pixel = self.info.bytes_per_pixel;
        let index = (y * self.info.stride + x) * bytes_per_pixel;
        let red = (color >> 16) as u8;
        let green = (color >> 8) as u8;
        let blue = color as u8;

        let pixel = &mut self.buffer[index..index + bytes_per_pixel];
        match self.info.pixel_format {
            PixelFormat::Rgb => {
                pixel[0] = red;
                pixel[1] = green;
                pixel[2] = blue;
            }
            PixelFormat::Bgr => {
                pixel[0] = blue;
                pixel[1] = green;
                pixel[2] = red;
            }
            // grayscale or unknown format: use the brightest channel
            _ => pixel[0] = red.max(green).max(blue),
        }
    }

    fn draw_glyph(&mut self, col: usize, row: usize, glyph: [u8; 8]) {
        let x0 = col * CHAR_W;
        let y0 = row * CHAR_H;
        for (y, row_bits) in glyph.iter().enumerate() {
            for x in 0..8 {
                let color = if row_bits & (1 << x) != 0 { self.fg } else { 0 };
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        self.set_pixel(x0 + x * SCALE + dx, y0 + y * SCALE + dy, color);
                    }
                }
            }
        }
    }

    fn newline(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row >= self.height_chars() {
            self.scroll();
            self.row = self.height_chars() - 1;
        }
    }

    fn backspace(&mut self) {
        if self.col > 0 {
            self.col -= 1;
            self.draw_glyph(self.col, self.row, [0; 8]);
        }
    }

    /// Shift the whole screen up by one text row.
    fn scroll(&mut self) {
        let row_bytes = CHAR_H * self.info.stride * self.info.bytes_per_pixel;
        let len = self.buffer.len();
        self.buffer.copy_within(row_bytes..len, 0);
        self.buffer[len - row_bytes..].fill(0);
    }

    fn put_char(&mut self, c: char) {
        match c {
            '\n' => self.newline(),
            '\r' => self.col = 0,
            '\x08' => self.backspace(),
            c => {
                if self.col >= self.width_chars() {
                    self.newline();
                }
                let glyph = font8x8::BASIC_FONTS
                    .get(c)
                    .or_else(|| font8x8::BASIC_FONTS.get('?'))
                    .unwrap_or([0; 8]);
                self.draw_glyph(self.col, self.row, glyph);
                self.col += 1;
            }
        }
    }
}

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.put_char(c);
        }
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use x86_64::instructions::interrupts;

    if let Ok(console) = CONSOLE.try_get() {
        // disable interrupts while holding the lock: an interrupt
        // handler that prints would otherwise deadlock on it
        interrupts::without_interrupts(|| {
            console.lock().write_fmt(args).expect("printing failed");
        });
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::console::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

/// Adapter so the `log` macros (used all over the kernel) end up on the
/// console as well.
struct ConsoleLogger;

impl log::Log for ConsoleLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        crate::println!("[{}] {}", record.level(), record.args());
    }

    fn flush(&self) {}
}
