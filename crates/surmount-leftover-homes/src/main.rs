//! Leftover agent-home mkdir scanner.

use std::process::exit;

use surmount_leftover_homes::{USAGE, dispatch, parse_args};

fn main() {
    match parse_args(std::env::args()) {
        Ok(mode) => exit(dispatch(mode)),
        Err(e) => {
            eprintln!("surmount-leftover-homes: {e}");
            eprint!("{USAGE}");
            exit(1);
        }
    }
}
