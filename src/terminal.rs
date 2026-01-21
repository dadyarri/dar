use std::io::prelude::*;

use term;

pub fn success(message: &str) {
    match term::stdout() {
        Some(mut t) => {
            match t.fg(term::color::GREEN) {
                Ok(_) => {
                    write!(t, "{}", message).unwrap();
                    t.reset().unwrap();
                }
                Err(_) => writeln!(t, "{}", message).unwrap(),
            };
        }
        _ => {
            eprint!("{}", message);
        }
    }
}
