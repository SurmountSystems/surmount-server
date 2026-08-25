//! Private-data admission scanner. Patterns only. Never prints secret lines.

use std::process::exit;

use surmount_private_data::{dispatch, parse_args, usage};

fn main() {
    match parse_args(std::env::args_os()) {
        Ok(action) => exit(dispatch(action)),
        Err(e) => {
            eprintln!("surmount-private-data: {e}");
            eprint!("{}", usage());
            exit(1);
        }
    }
}
