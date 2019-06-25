// catj, rust streaming parser edition
// Displays JSON files in a flat format.
// https://github.com/wfraser/catj-rss
// Copyright 2019 William R. Fraser

#![deny(rust_2018_idioms)]

use std::char;
use std::cmp::min;
use std::io::{self, Read};
use std::num::{ParseFloatError, ParseIntError};
use std::process::exit;
use std::str::{self, Utf8Error};

mod tables;
use tables::{STATES, GOTOS, CATCODE};

#[derive(Debug)]
enum JsonError {
    Truncated,
    Syntax,
    IntParse(ParseIntError),
    FloatParse(ParseFloatError),
    InvalidEscape(String),
    Unicode(Utf8Error),
    IO(io::Error),
}

#[derive(Debug)]
enum Value {
    Object,
    List { index: u64 }, // just the current size of the list as we parse the JSON
    Terminal(Terminal),
}

/// Things we print a line for.
#[derive(Debug)]
enum Terminal {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl std::fmt::Display for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Terminal::Null => f.write_str("null"),
            Terminal::Bool(v) => write!(f, "{:?}", v),
            Terminal::Int(v) => write!(f, "{}", v),
            Terminal::Float(v) => write!(f, "{}", v),
            Terminal::String(s) => write!(f, "{:?}", s),
        }
    }
}

impl Into<Value> for Terminal {
    fn into(self) -> Value {
        Value::Terminal(self)
    }
}

fn parse(input: impl Read) -> Result<(), (u64, u64, JsonError)> {
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
        state = parse_ch(cat, ch, &mut stack, state, &mut ds, &mut ss, &mut es)
            .map_err(|e| (line, col, e))?;
    }
    state = parse_ch(CATCODE[32], b'?', &mut stack, state, &mut ds, &mut ss, &mut es)
        .map_err(|e| (line, col, e))?;
    if state != 0 {
        return Err((line, col, JsonError::Truncated));
    }
    Ok(())
}

fn parse_ch(cat: u8, ch: u8, stack: &mut Vec<u8>, mut state: u8, ds: &mut Vec<Value>,
            ss: &mut Vec<u8>, es: &mut String)
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

        if action > 0 {
            do_action(action, ch, ds, ss, es)?;
        }

        if code == 0xFF {
            state = stack.pop().unwrap();
        } else {
            state = code as u8;
            return Ok(state);
        }
    }
}

fn do_action(action: u8, ch: u8, ds: &mut Vec<Value>, ss: &mut Vec<u8>, es: &mut String)
    -> Result<(), JsonError>
{
    match action {
        0x1 => { // push list
            ds.push(Value::List { index: 0 });
        }
        0x2 => { // push object
            ds.push(Value::Object);
        }
        0x3 => { // pop & append
            let v = ds.pop().unwrap();
            if let Value::Terminal(v) = v {
                print_path(ds);
                println!(" = {}", v);
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
            let k = ds.pop().unwrap();
            if let Value::Terminal(v) = v {
                print_path(&*ds);
                match k {
                    Value::Terminal(Terminal::String(s)) => print!("{}", s),
                    _ => panic!("object field must be a string, not {:?}", k),
                }
                println!(" = {}", v);
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
        0x9 => { // push int
            ds.push(
                Terminal::Int(
                    str::from_utf8(&ss)
                        .map_err(JsonError::Unicode)?
                        .parse()
                        .map_err(JsonError::IntParse)?
                ).into());
            ss.clear();
        }
        0xA => { // push float
            ds.push(
                Terminal::Float(
                    str::from_utf8(&ss)
                        .map_err(JsonError::Unicode)?
                        .parse()
                        .map_err(JsonError::FloatParse)?
                ).into());
            ss.clear();
        }
        0xB => { // push ch to ss
            ss.push(ch);
        }
        0xC => { // push ch to es
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
            let n = u16::from_str_radix(es, 16).map_err(|_|
                    JsonError::InvalidEscape(format!("\\u{}", es)))?;
            if let Some(u) = char::from_u32(u32::from(n)) {
                // push the UTF-8 bytes of it to the string buffer
                let mut buf = [0u8; 4];
                u.encode_utf8(&mut buf);
                ss.extend(&buf[0 .. u.len_utf8()]);
            } else {
                return Err(JsonError::InvalidEscape(format!("\\u{}", es)));
            }
            es.clear();
        }
        _ => panic!("JSON algorithm bug"),
    }
    Ok(())
}

fn print_path(ds: &[Value]) {
    for item in ds {
        match item {
            Value::Object => print!("."),
            Value::List { index } => print!("[{}]", index),
            Value::Terminal(Terminal::String(s)) => print!("{}", s),
            Value::Terminal(other) => panic!("invalid item in a path: {:?}", other),
        }
    }
}

fn main() {
    if let Some(arg) = std::env::args().nth(1) {
        if arg == "--version" || arg == "-V" {
            eprintln!("catj-rss v{}", env!("CARGO_PKG_VERSION"));
            eprintln!("Copyright 2019 William R. Fraser");
            eprintln!("https://github.com/wfraser/catj-rss");
            exit(1);
        } else {
            eprintln!("usage: {} [-V | --version] < some_file.json", std::env::args().next().unwrap());
            eprintln!("Displays JSON files in a flat format.");
            eprintln!("Reads from standard input, writes to standard output.");
            eprintln!("see https://github.com/wfraser/catj-rss");
            exit(1);
        }
    }

    if let Err((line, col, e)) = parse(io::stdin().lock()) {
        eprint!("Error in input at line {} column {}: ", line, col);
        match e {
            JsonError::Truncated => eprintln!("JSON truncated"),
            JsonError::Syntax => eprintln!("invalid JSON syntax"),
            JsonError::IntParse(e) => eprintln!("invalid integer: {}", e),
            JsonError::FloatParse(e) => eprintln!("invalid floating-point number: {}", e),
            JsonError::InvalidEscape(e) => eprintln!("invalid string escape sequence: {}", e),
            JsonError::Unicode(e) => eprintln!("invalid UTF-8: {}", e),
            JsonError::IO(e) => eprintln!("I/O error: {}", e),
        }
        exit(2);
    }
}
