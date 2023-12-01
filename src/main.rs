#![deny(rust_2018_idioms)]

/// catj, rust streaming parser edition
///
/// Displays JSON files in a flat format.
///
/// https://github.com/wfraser/catj-rss
///
/// Copyright 2019-2023 William R. Fraser

use std::char;
use std::cmp::min;
use std::io::{self, Read, Write};
use std::process::exit;
use std::str::{self, Utf8Error};

mod tables;
use tables::{STATES, GOTOS, CATCODE};

#[derive(Debug)]
enum JsonError {
    Truncated,
    Syntax,
    InvalidEscape(String),
    Unicode(Utf8Error),
    IO(io::Error),
}

impl From<io::Error> for JsonError {
    fn from(e: io::Error) -> Self {
        JsonError::IO(e)
    }
}

#[derive(Debug)]
enum Value {
    Object { empty: bool }, // empty: whether we've seen any fields yet while parsing
    List { index: u64 }, // index: the current size of the list while parsing
    Terminal(Terminal),
}

/// Things we print a line for.
#[derive(Debug)]
enum Terminal {
    Null,
    Bool(bool),
    Number(String),
    String(String),
}

impl std::fmt::Display for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Terminal::Null => f.write_str("null"),
            Terminal::Bool(v) => write!(f, "{v:?}"),
            Terminal::Number(s) => f.write_str(s),
            Terminal::String(s) => {
                f.write_str("\"")?;
                let mut tmp = [0u8; 4];
                for c in s.chars() {
                    match c {
                        '"' => f.write_str("\\\"")?,
                        '\\' => f.write_str("\\\\")?,
                        '\x08' => f.write_str("\\b")?,
                        '\t' => f.write_str("\\t")?,
                        '\x0C' => f.write_str("\\f")?,
                        '\n' => f.write_str("\\n")?,
                        '\r' => f.write_str("\\r")?,
                        c if (c as u32) < 0x20 => write!(f, "\\u{:04x}", c as u32)?,
                        // to emit astral plane characters as escaped surrogate pairs:
                        /*c if (c as u32) > 0xFFFF => {
                            let mut pair = [0u16; 2];
                            c.encode_utf16(&mut pair);
                            write!(f, "\\u{:04x}\\u{:04x}", pair[0], pair[1])?;
                        }*/
                        c => f.write_str(c.encode_utf8(&mut tmp))?,
                    }
                }
                f.write_str("\"")
            }
        }
    }
}

impl From<Terminal> for Value {
    fn from(t: Terminal) -> Self {
        Value::Terminal(t)
    }
}

fn parse(input: impl Read, mut output: impl Write) -> Result<(), (u64, u64, JsonError)> {
    let mut stack = vec![];
    let mut state = 0;
    let mut ds: Vec<Value> = vec![];    // data stack
    let mut ss: Vec<u8> = vec![];       // string stack
    let mut es = String::new();         // escape stack
    let mut line = 1;
    let mut col = 0;
    for maybe_ch in input.bytes() {
        let ch = maybe_ch.map_err(|e| (line, col, JsonError::IO(e)))?;
        if ch == b'\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        let cat = CATCODE[min(ch, 0x7e) as usize];
        state = parse_ch(cat, ch, &mut stack, state, &mut ds, &mut ss, &mut es, &mut output)
            .map_err(|e| (line, col, e))?;
    }
    state = parse_ch(CATCODE[32], b'?', &mut stack, state, &mut ds, &mut ss, &mut es, &mut output)
        .map_err(|e| (line, col, e))?;
    if state != 0 {
        return Err((line, col, JsonError::Truncated));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn parse_ch(cat: u8, ch: u8, stack: &mut Vec<u8>, mut state: u8, ds: &mut Vec<Value>,
            ss: &mut Vec<u8>, es: &mut String, mut output: impl Write)
    -> Result<u8, JsonError>
{
    loop {
        let mut code: u16 = STATES[state as usize][cat as usize];
        let mut action: u8 = (code >> 8 & 0xFF) as u8;
        code &= 0xFF;

        if action == 0xFF && code == 0xFF {
            return Err(JsonError::Syntax);
        } else if action >= 0x80 {
            stack.push(GOTOS[state as usize]);
            action -= 0x80;
        }

        if state == 0 && !ds.is_empty() {
            // New top-level value.
            output.write_all(b"\n")?;
            ds.pop();
        }

        if action > 0 {
            do_action(action, ch, ds, ss, es, &mut output)?;
        }

        if code == 0xFF {
            state = stack.pop().unwrap();
        } else {
            state = code as u8;
            return Ok(state);
        }
    }
}

fn do_action(action: u8, ch: u8, ds: &mut Vec<Value>, ss: &mut Vec<u8>, es: &mut String,
             mut output: impl Write)
    -> Result<(), JsonError>
{
    match action {
        0x1 => { // push list
            ds.push(Value::List { index: 0 });
        }
        0x2 => { // push object
            ds.push(Value::Object { empty: true });
        }
        0x3 => { // pop & append
            let v = ds.pop().unwrap();
            if let Value::Terminal(v) = v {
                print_path(ds, &mut output)?;
                writeln!(&mut output, " = {v}")?;
            }
            match ds.last_mut() {
                Some(Value::List { index }) => {
                    *index += 1;
                }
                other => panic!("expected list on top of the stack, not {:?}", other)
            }
        }
        0x4 => { // pop pop & setitem
            let v = ds.pop().unwrap();

            let mut print_lhs = || -> io::Result<()> {
                print_path(&*ds, &mut output)?;
                output.write_all(b" = ")?;
                Ok(())
            };
            match v {
                Value::Terminal(v) => {
                    print_lhs()?;
                    writeln!(&mut output, "{v}")?;
                }
                Value::List { index: 0 } => {
                    print_lhs()?;
                    output.write_all(b"[]\n")?;
                }
                Value::Object { empty: true } => {
                    print_lhs()?;
                    output.write_all(b"{}\n")?;
                }
                Value::List { index: _ } | Value::Object { empty: false } => {
                    // already printed fields for these; nothing to do here.
                }
            }

            // pop key, which we've now printed
            ds.pop().unwrap();

            if let Some(Value::Object { ref mut empty }) = ds.last_mut() {
                *empty = false;
            } else {
                panic!("can't set a field on non-object: {:?}", ds.last());
            }
        }
        0x5 => { // push null
            ds.push(Terminal::Null.into());
        }
        0x6 => { // push true
            ds.push(Terminal::Bool(true).into());
        }
        0x7 => { // push false
            ds.push(Terminal::Bool(false).into());
        }
        0x8 => { // push string
            let s = String::from_utf8(ss.clone())
                .map_err(|e| JsonError::Unicode(e.utf8_error()))?;
            ds.push(Terminal::String(s).into());
            ss.clear();
            es.clear();
        }
        0x9 | 0xA => { // push int, push float
            ds.push(
                Terminal::Number(
                    str::from_utf8(ss)
                        .map_err(JsonError::Unicode)?
                        .to_owned()
                ).into());
            ss.clear();
        }
        0xB => { // push ch to ss
            ss.push(ch);
            if !es.is_empty() {
                let bad = std::mem::take(es);
                return Err(JsonError::InvalidEscape(bad));
            }
            es.clear();
        }
        0xC => { // push ch to es
            if !ch.is_ascii_hexdigit() {
                return Err(JsonError::InvalidEscape(
                        format!("{:?} is not a hex digit", ch as char)));
            }
            es.push(ch as char);
        }
        0xD => { // push escape
            let c: u8 = match ch {
                b'b' => 8,
                b't' => b'\t', //9,
                b'n' => b'\n', //10,
                b'f' => 12,
                b'r' => b'\r', //13,
                _ => { return Err(JsonError::InvalidEscape(format!("\\{}", ch as char))); },
            };
            ss.push(c);
            es.clear();
        }
        0xE => { // push unicode code point
            let codepoint = match es.len() {
                8 => {
                    let high_str = es.get(0..4)
                        .ok_or_else(|| JsonError::InvalidEscape(
                                format!("\\u{es}")))?;
                    let high = u16::from_str_radix(high_str, 16)
                        .map_err(|e| JsonError::InvalidEscape(
                                format!("\\u{high_str}: {e}")))?;
                    if !(0xD800 ..= 0xDBFF).contains(&high) {
                        return Err(JsonError::InvalidEscape(
                                format!("\\u{high_str}: unpaired high surrogate")));
                    }

                    let low_str = es.get(4..8)
                        .ok_or_else(|| JsonError::InvalidEscape(
                                format!("\\u{es}")))?;
                    let low = u16::from_str_radix(low_str, 16)
                        .map_err(|e| JsonError::InvalidEscape(
                                format!("\\u{low_str}: {e}")))?;
                    if !(0xDC00 ..= 0xDFFF).contains(&low) {
                        return Err(JsonError::InvalidEscape(
                                format!("\\u{low_str}: unpaired low surrogate")));
                    }

                    0x1_0000
                        + (high as u32 - 0xD800) * 0x400
                        + (low as u32 - 0xDC00)
                }
                4 => {
                    let two_bytes = u16::from_str_radix(es, 16)
                        .map_err(|e| JsonError::InvalidEscape(format!("\\u{es}: {e}")))?;
                    if (0xD800..0xDBFF).contains(&two_bytes) {
                        // We need to read another surrogate pair to do anything. Keep the 'es'
                        // buffer unchanged, and let more characters accumulate in it.
                        return Ok(());
                    }
                    u32::from(two_bytes)
                }
                _ => {
                    return Err(JsonError::InvalidEscape(
                            format!("\\u{es}: wrong number of digits")));
                }
            };

            if let Some(u) = char::from_u32(codepoint) {
                // push the UTF-8 bytes of it to the string buffer
                let mut buf = [0u8; 4];
                u.encode_utf8(&mut buf);
                ss.extend(&buf[0 .. u.len_utf8()]);
            } else {
                return Err(JsonError::InvalidEscape(format!("\\u{es} ?")));
            }
            es.clear();
        }
        _ => panic!("JSON algorithm bug"),
    }
    Ok(())
}

fn print_path(ds: &[Value], output: &mut impl Write) -> io::Result<()> {
    for item in ds {
        match item {
            Value::Object { .. } => output.write_all(b".")?,
            Value::List { index } => write!(output, "[{index}]")?,
            Value::Terminal(term @ Terminal::String(s)) => {
                if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    // Write it as a bare string
                    write!(output, "{s}")?;
                } else {
                    // Write it with quotes, escapes, etc. as if it were a value
                    write!(output, "{term}")?;
                }
            }
            Value::Terminal(other) => panic!("invalid item in a path: {:?}", other),
        }
    }
    Ok(())
}

fn main() {
    if let Some(arg) = std::env::args().nth(1) {
        if arg == "--version" || arg == "-V" {
            eprintln!("catj-rss v{}", env!("CARGO_PKG_VERSION"));
            eprintln!("Copyright 2019-2023 William R. Fraser");
            eprintln!("https://github.com/wfraser/catj-rss");
        } else {
            eprintln!("usage: {} [-V | --version] < some_file.json", std::env::args().next().unwrap());
            eprintln!("Displays JSON files in a flat format.");
            eprintln!("Reads from standard input, writes to standard output.");
            eprintln!("see https://github.com/wfraser/catj-rss");
        }
        exit(1);
    }

    if let Err((line, col, e)) = parse(io::stdin().lock(), io::stdout().lock()) {
        eprint!("Error in input at line {line} column {col}: ");
        match e {
            JsonError::Truncated => eprintln!("JSON truncated"),
            JsonError::Syntax => eprintln!("invalid JSON syntax"),
            JsonError::InvalidEscape(e) => eprintln!("invalid string escape sequence: {e}"),
            JsonError::Unicode(e) => eprintln!("invalid UTF-8: {e}"),
            JsonError::IO(e) => eprintln!("I/O error: {e}"),
        }
        exit(2);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn run(input: &str) -> String {
        let mut input = io::Cursor::new(input);
        let mut out = io::Cursor::new(vec![]);
        parse(&mut input, &mut out).unwrap();
        String::from_utf8(out.into_inner()).expect("bad utf8").trim().to_owned()
    }

    #[test]
    fn test_empty() {
        assert_eq!("", run(""));
        assert_eq!("", run("{}"));
        assert_eq!("", run("[]"));
        assert_eq!("", run("[{}]"));
    }

    #[test]
    fn test_not_empty() {
        assert_eq!(".foo = []", run(r#"{"foo": []}"#));
    }

    #[test]
    fn test_simple() {
        assert_eq!(".foo = \"bar\"", run(r#"{"foo": "bar"}"#));
    }

    #[test]
    fn test_non_ident_keys() {
        assert_eq!(".bare = \"a\"", run(r#"{"bare": "a"}"#));
        assert_eq!(".\"quoted now\" = \"b\"", run(r#"{"quoted now": "b"}"#));
    }

    #[test]
    fn test_utf8() {
        assert_eq!(".\"⚙🖥\" = \"🦀\"", run(r#"{"⚙🖥": "🦀"}"#));
    }

    #[test]
    fn test_escapes() {
        assert_eq!(".smile = \"😊\"", run(r#"{"smile": "\ud83d\ude0a"}"#));
        assert_eq!(".\"\\b_backspace\" = \"carriage\\r\\nreturn\"", run(r#"{"\b_backspace": "carriage\r\nreturn"}"#));
    }
}
