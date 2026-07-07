use super::keyboard::ScancodeStream;
use super::timer;
use crate::speaker;
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
                // the instruments need the scancode stream itself, so
                // they are handled here instead of in execute()
                match line.trim() {
                    "piano" => piano(&mut scancodes, &mut keyboard).await,
                    "organ" => organ(&mut scancodes, &mut keyboard).await,
                    // reads the score from the keyboard, so it needs the
                    // scancode stream like the instruments do
                    "celebrare -" => {
                        let score = read_score(&mut scancodes, &mut keyboard).await;
                        crate::celebrare::celebrare("-", Some(score)).await;
                    }
                    other => execute(other).await,
                }
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

async fn execute(line: &str) {
    let mut parts = line.split_whitespace();
    let Some(command) = parts.next() else {
        return;
    };
    match command {
        "help" => {
            println!("available commands:");
            println!("  help           this text");
            println!("  echo <text>    print text");
            println!("  clear          clear the screen");
            println!("  uptime         time since boot");
            println!("  heap           allocator statistics");
            println!("  beep [hz] [ms] beep (default 440 Hz, 300 ms)");
            println!("  play <notes>   play notes, e.g. play c4 e4 g4 c5:800");
            println!("  piano          live instrument (pc speaker, mono)");
            println!("  organ          live instrument (ac97, polyphonic)");
            println!("  celebrare <s>  run an Officium score (meteor, or - to type one)");
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
        "beep" => {
            let freq = parts.next().and_then(|s| s.parse().ok()).unwrap_or(440);
            let ms = parts.next().and_then(|s| s.parse().ok()).unwrap_or(300);
            speaker::start_tone(freq);
            timer::sleep_ms(ms).await;
            speaker::stop_tone();
        }
        "play" => {
            // notes like c4, d#4; an optional :ms suffix sets the length
            for token in parts {
                let (note, ms) = match token.split_once(':') {
                    Some((note, ms)) => (note, ms.parse().unwrap_or(400)),
                    None => (token, 400),
                };
                let Some(freq) = speaker::note_freq(note) else {
                    println!("bad note '{}'", note);
                    break;
                };
                speaker::start_tone(freq);
                timer::sleep_ms(ms).await;
                speaker::stop_tone();
                // short gap so repeated notes are distinguishable
                timer::sleep_ms(30).await;
            }
        }
        "celebrare" => {
            let name = parts.next().unwrap_or("meteor");
            crate::celebrare::celebrare(name, None).await;
        }
        unknown => println!("unknown command '{}', try 'help'", unknown),
    }
}

/// Read a multi-line Officium score from the keyboard: lines echo as
/// typed; an empty line ends the score (celebrare '-' mode, §10.2).
async fn read_score(
    scancodes: &mut ScancodeStream,
    keyboard: &mut Keyboard<layouts::Us104Key, ScancodeSet1>,
) -> alloc::string::String {
    use alloc::string::String;

    println!("type the score; an empty line celebrates it");
    let mut score = String::new();
    let mut line = String::new();
    print!("| ");
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
                if line.is_empty() {
                    return score;
                }
                score.push_str(&line);
                score.push('\n');
                line.clear();
                print!("| ");
            }
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
    score
}

/// Live instrument: the tone sounds for as long as the key is held,
/// like an organ. Works on raw key events (down/up) instead of decoded
/// characters.
async fn piano(
    scancodes: &mut ScancodeStream,
    keyboard: &mut Keyboard<layouts::Us104Key, ScancodeSet1>,
) {
    use pc_keyboard::{KeyCode, KeyState};

    println!("piano: a s d f g h j k = c4..c5, w e t y u = sharps, esc exits");

    let mut sounding: Option<KeyCode> = None;
    while let Some(scancode) = scancodes.next().await {
        let Ok(Some(event)) = keyboard.add_byte(scancode) else {
            continue;
        };
        match event.state {
            KeyState::Down => {
                if event.code == KeyCode::Escape {
                    speaker::stop_tone();
                    break;
                }
                if let Some(freq) = key_note_freq(event.code) {
                    speaker::start_tone(freq);
                    sounding = Some(event.code);
                }
            }
            KeyState::Up => {
                // only stop if no newer note took over the speaker
                if sounding == Some(event.code) {
                    speaker::stop_tone();
                    sounding = None;
                }
            }
            _ => {}
        }
    }
}

/// Polyphonic organ on the AC97: every held key is a voice, chords work.
async fn organ(
    scancodes: &mut ScancodeStream,
    keyboard: &mut Keyboard<layouts::Us104Key, ScancodeSet1>,
) {
    use pc_keyboard::{KeyCode, KeyState};

    if !crate::ac97::is_available() {
        println!("ac97 not available");
        return;
    }
    println!("organ: a s d f g h j k = c4..c5, w e t y u = sharps, esc exits");
    println!("polyphonic: hold several keys for a chord");

    while let Some(scancode) = scancodes.next().await {
        let Ok(Some(event)) = keyboard.add_byte(scancode) else {
            continue;
        };
        match event.state {
            KeyState::Down => {
                if event.code == KeyCode::Escape {
                    crate::ac97::all_notes_off();
                    break;
                }
                if let Some(freq) = key_note_freq(event.code) {
                    crate::ac97::note_on(freq);
                }
            }
            KeyState::Up => {
                if let Some(freq) = key_note_freq(event.code) {
                    crate::ac97::note_off(freq);
                }
            }
            _ => {}
        }
    }
}

/// Map the middle keyboard row to one octave, tracker-style.
fn key_note_freq(code: pc_keyboard::KeyCode) -> Option<u32> {
    use pc_keyboard::KeyCode;

    let note = match code {
        KeyCode::A => "c4",
        KeyCode::W => "c#4",
        KeyCode::S => "d4",
        KeyCode::E => "d#4",
        KeyCode::D => "e4",
        KeyCode::F => "f4",
        KeyCode::T => "f#4",
        KeyCode::G => "g4",
        KeyCode::Y => "g#4",
        KeyCode::H => "a4",
        KeyCode::U => "a#4",
        KeyCode::J => "b4",
        KeyCode::K => "c5",
        _ => return None,
    };
    speaker::note_freq(note)
}
