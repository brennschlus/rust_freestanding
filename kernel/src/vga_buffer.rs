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

