//! Line-oriented lexer for the plain form (§7.1). `;` starts a comment
//! to end of line — with one liturgical exception (§7.3): on a line
//! that begins `label "sung text"` (a single uppercase rhyme label
//! followed by a string), the first `;` is a separator token dividing
//! the sung text from the statement it lowers to. Any later `;` on the
//! line is a comment again. Newlines are significant tokens: they
//! terminate statements and expressions (except inside parentheses).

use alloc::string::String;
use alloc::vec::Vec;

use officium_core::types::Dissonance;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Num(f64),
    Str(String),
    // punctuation
    LBrace,
    RBrace,
    LParen,
    RParen,
    Eq,
    At,
    Arrow,     // ->
    BindArrow, // <-
    Lambda,    // \
    Plus,
    Minus,
    Star,
    Slash,
    Semi, // liturgical separator: label "text" ; stmt (§7.3)
    Newline,
}

/// A rhyme label is a single uppercase ASCII letter (A, B, C ...).
pub fn rhyme_label(s: &str) -> Option<char> {
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_uppercase() => Some(c),
        _ => None,
    }
}

pub struct Lexed {
    pub toks: Vec<(Tok, u32)>, // token + line number
}

pub fn lex(src: &str) -> Result<Lexed, Dissonance> {
    let mut toks = Vec::new();
    let mut line: u32 = 1;
    let bytes = src.as_bytes();
    let mut i = 0;

    let err = |line: u32, msg: &str| Dissonance::Parse { line, msg: String::from(msg) };

    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            '\n' => {
                // collapse runs of newlines into one token
                if !matches!(toks.last(), Some((Tok::Newline, _)) | None) {
                    toks.push((Tok::Newline, line));
                }
                line += 1;
                i += 1;
            }
            ' ' | '\t' | '\r' => i += 1,
            ';' => {
                // liturgical line head? tokens since the last newline
                // must be exactly [rhyme label, sung text] (§7.3)
                let line_start = toks
                    .iter()
                    .rposition(|(t, _)| matches!(t, Tok::Newline))
                    .map(|p| p + 1)
                    .unwrap_or(0);
                let head = &toks[line_start..];
                let is_sung = matches!(
                    head,
                    [(Tok::Ident(l), _), (Tok::Str(_), _)] if rhyme_label(l).is_some()
                );
                if is_sung {
                    toks.push((Tok::Semi, line));
                    i += 1;
                } else {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
            }
            '{' => {
                toks.push((Tok::LBrace, line));
                i += 1;
            }
            '}' => {
                toks.push((Tok::RBrace, line));
                i += 1;
            }
            '(' => {
                toks.push((Tok::LParen, line));
                i += 1;
            }
            ')' => {
                toks.push((Tok::RParen, line));
                i += 1;
            }
            '=' => {
                toks.push((Tok::Eq, line));
                i += 1;
            }
            '@' => {
                toks.push((Tok::At, line));
                i += 1;
            }
            '\\' => {
                toks.push((Tok::Lambda, line));
                i += 1;
            }
            '+' => {
                toks.push((Tok::Plus, line));
                i += 1;
            }
            '*' => {
                toks.push((Tok::Star, line));
                i += 1;
            }
            '/' => {
                toks.push((Tok::Slash, line));
                i += 1;
            }
            '-' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                    toks.push((Tok::Arrow, line));
                    i += 2;
                } else {
                    toks.push((Tok::Minus, line));
                    i += 1;
                }
            }
            '<' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                    toks.push((Tok::BindArrow, line));
                    i += 2;
                } else {
                    return Err(err(line, "stray '<'"));
                }
            }
            '"' => {
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && bytes[j] != b'"' && bytes[j] != b'\n' {
                    j += 1;
                }
                if j >= bytes.len() || bytes[j] != b'"' {
                    return Err(err(line, "unterminated string"));
                }
                match core::str::from_utf8(&bytes[start..j]) {
                    Ok(s) => toks.push((Tok::Str(String::from(s)), line)),
                    Err(_) => return Err(err(line, "invalid utf-8 in string")),
                }
                i = j + 1;
            }
            c if c.is_ascii_digit() => {
                let start = i;
                let mut j = i;
                while j < bytes.len()
                    && (bytes[j].is_ascii_digit() || bytes[j] == b'.' || bytes[j] == b'e'
                        || bytes[j] == b'E'
                        || ((bytes[j] == b'+' || bytes[j] == b'-')
                            && j > start
                            && (bytes[j - 1] == b'e' || bytes[j - 1] == b'E')))
                {
                    j += 1;
                }
                let s = core::str::from_utf8(&bytes[start..j]).unwrap_or("");
                match s.parse::<f64>() {
                    Ok(n) => toks.push((Tok::Num(n), line)),
                    Err(_) => return Err(err(line, "malformed number")),
                }
                i = j;
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                let mut j = i;
                while j < bytes.len()
                    && ((bytes[j] as char).is_ascii_alphanumeric() || bytes[j] == b'_')
                {
                    j += 1;
                }
                let s = core::str::from_utf8(&bytes[start..j]).unwrap_or("");
                toks.push((Tok::Ident(String::from(s)), line));
                i = j;
            }
            _ => return Err(err(line, "unexpected character")),
        }
    }
    Ok(Lexed { toks })
}
