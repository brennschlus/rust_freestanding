use super::keyboard::ScancodeStream;
use crate::{print, println};
use alloc::string::String;
use futures_util::stream::StreamExt;
use pc_keyboard::{DecodedKey, HandleControl, Keyboard, ScancodeSet1, layouts};

/// The shell: reads decoded keys from the scancode stream, edits a line
/// buffer with echo, and executes the entered command on enter.
pub async fn run() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );
    let mut line = String::new();

    print!("> ");
    while let Some(scancode) = scancodes.next().await {
        let Ok(Some(key_event)) = keyboard.add_byte(scancode) else {
            continue;
        };
        let Some(key) = keyboard.process_keyevent(key_event) else {
            continue;
        };
        match key {
            DecodedKey::Unicode('\n' | '\r') => {
                println!();
                execute(line.trim());
                line.clear();
                print!("> ");
            }
            // backspace (^H) or delete
            DecodedKey::Unicode('\x08' | '\x7f') => {
                if line.pop().is_some() {
                    print!("\x08");
                }
            }
            DecodedKey::Unicode(c) if !c.is_control() => {
                line.push(c);
                print!("{}", c);
            }
            _ => {}
        }
    }
}

fn execute(line: &str) {
    let mut parts = line.split_whitespace();
    let Some(command) = parts.next() else {
        return;
    };
    match command {
        "help" => {
            println!("available commands:");
            println!("  help          this text");
            println!("  echo <text>   print text");
            println!("  clear         clear the screen");
            println!("  uptime        time since boot");
            println!("  heap          allocator statistics");
        }
        "echo" => {
            // keep the original spacing instead of re-joining the parts
            println!("{}", line[command.len()..].trim_start());
        }
        "clear" => crate::console::clear(),
        "uptime" => {
            // PIT ticks at ~18.2 Hz -> ~55 ms per tick
            let ms = crate::interrupts::ticks() * 55;
            println!("{}.{:03} s", ms / 1000, ms % 1000);
        }
        "heap" => {
            println!(
                "heap: {} bytes used, {} bytes free",
                crate::allocator::used(),
                crate::allocator::free()
            );
        }
        unknown => println!("unknown command '{}', try 'help'", unknown),
    }
}
