use std::io::prelude::*;

use term;

// Some examples on how to use the term crate

pub fn error(message: &str) {
    match term::stderr() { Some(mut t) => {
        match t.fg(term::color::BRIGHT_RED) {
            Ok(_) => {
                write!(t, "{}", message).unwrap();
                t.reset().unwrap();
            },
            Err(_) => writeln!(t, "{}", message).unwrap()
        };
    } _ => {
        eprint!("{}", message);
    }}
}


pub fn success(message: &str) {
    match term::stdout() { Some(mut t) => {
        match t.fg(term::color::GREEN) {
            Ok(_) => {
                write!(t, "{}", message).unwrap();
                t.reset().unwrap();
            },
            Err(_) => writeln!(t, "{}", message).unwrap()
        };
    } _ => {
        eprint!("{}", message);
    }}
}

