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

mod tables;
use tables::{STATES, GOTOS, CATCODE};

#[derive(Debug)]
pub enum JsonError {
    Truncated,
    Syntax,
    IntParse(ParseIntError),
    FloatParse(ParseFloatError),
    InvalidEscape(String),
    IO(io::Error),
}

#[derive(Debug)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(u64),
    Object,
}

impl Value {
    fn as_list(&mut self) -> &mut u64 {
        match self {
            Value::List(ref mut l) => l,
            _ => panic!("wrong type - expected List, got {:?}", self),
        }
    }
}

pub fn parse(input: impl Read) -> Result<(), (u64, u64, JsonError)> {
    let mut stack = vec![];
    let mut state = 0;
    let mut ds: Vec<Value> = vec![];    // data stack
    let mut ss = String::new();         // string stack
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

fn parse_ch(cat: u8, ch: u8, stack: &mut Vec<u8>, mut state: u8,
            ds: &mut Vec<Value>, ss: &mut String, es: &mut String)
        -> Result<u8, JsonError> {
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

fn do_action(action: u8, ch: u8, ds: &mut Vec<Value>, ss: &mut String,
             es: &mut String) -> Result<(), JsonError> {
    match action {
        0x1 => { // push list
            ds.push(Value::List(0));
        },
        0x2 => { // push object
            ds.push(Value::Object);
        },
        0x3 => { // pop & append
            ds.pop().unwrap();
            // we don't actually store the value, but increment the index of the list
            *ds.last_mut().unwrap().as_list() += 1;
        },
        0x4 => { // pop pop & setitem
            let v = ds.pop().unwrap();
            let k = ds.pop().unwrap();
            match v {
                Value::List(_) | Value::Object => {
                    // Don't print complex values; the terminals below them should already have
                    // been printed.
                }
                _ => {
                    for item in &*ds {
                        match item {
                            Value::Object => print!("."),
                            Value::String(s) => print!("{}", s),
                            Value::List(n) => print!("[{}]", n),
                            _ => panic!("invalid item in a path: {:?}", item),
                        }
                    }
                    match k {
                        Value::String(s) => print!("{}", s),
                        _ => panic!("object field must be string, not {:?}", k),
                    }
                    print!(" = ");
                    match v {
                        Value::Null => println!("null"),
                        Value::Bool(v) => println!("{:?}", v),
                        Value::Int(v) => println!("{}", v),
                        Value::Float(v) => println!("{}", v),
                        Value::String(s) => println!("{:?}", s),
                        _ => panic!("can't have complex thing as a value: {:?}", v),
                    }
                }
            }
        },
        0x5 => { // push null
            ds.push(Value::Null);
        },
        0x6 => { // push true
            ds.push(Value::Bool(true));
        },
        0x7 => { // push false
            ds.push(Value::Bool(false));
        },
        0x8 => { // push string
            ds.push(Value::String(ss.clone()));
            ss.clear();
            es.clear();
        },
        0x9 => { // push int
            ds.push(Value::Int(ss.parse().map_err(JsonError::IntParse)?));
            ss.clear();
        },
        0xA => { // push float
            ds.push(Value::Float(ss.parse().map_err(JsonError::FloatParse)?));
            ss.clear();
        },
        0xB => { // push ch to ss
            ss.push(ch as char);
        },
        0xC => { // push ch to es
            es.push(ch as char);
        }
        0xD => { // push escape
            let c: u8 = match ch as char {
                'b' => 8,
                't' => 9,
                'n' => 10,
                'f' => 12,
                'r' => 13,
                _ => { return Err(JsonError::InvalidEscape(format!("\\{}", ch))); },
            };
            ss.push(c as char);
            es.clear();
        },
        0xE => { // push unicode code point
            let n = u16::from_str_radix(es, 16).map_err(|_|
                    JsonError::InvalidEscape(format!("\\u{}", es)))?;
            if let Some(u) = char::from_u32(u32::from(n)) {
                ss.push(u);
            } else {
                return Err(JsonError::InvalidEscape(format!("\\u{}", es)));
            }
            es.clear();
        },
        _ => panic!("JSON algorithm bug"),
    }
    Ok(())
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
            JsonError::IO(e) => eprintln!("I/O error: {}", e),
        }
        exit(2);
    }
}
