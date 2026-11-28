//! Test double for the compiler glibcextract.compute_c_consts runs.
//!
//! glibc 2.44 scripts/glibcextract.py lines 60-63 compile with a shell
//! command: `{cc} -S -o test.s -x c - < test.c`. This binary stands in
//! for `{cc}`. It is not installed. The Nix package deletes it after
//! the crate tests.

use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let mut output: Option<String> = None;
    let mut recorded: Vec<String> = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "-o" {
            recorded.push(arg);
            match args.next() {
                Some(path) => {
                    recorded.push(path.clone());
                    output = Some(path);
                }
                None => {
                    eprintln!("fake-cc: -o needs a path");
                    return ExitCode::from(1);
                }
            }
        } else {
            recorded.push(arg);
        }
    }
    let mut stdin = String::new();
    if io::stdin().read_to_string(&mut stdin).is_err() {
        eprintln!("fake-cc: failed to read stdin");
        return ExitCode::from(1);
    }
    if let Ok(dir) = env::var("FAKE_CC_RECORD_DIR") {
        if fs::write(format!("{dir}/argv"), recorded.join("\n")).is_err()
            || fs::write(format!("{dir}/stdin"), &stdin).is_err()
        {
            eprintln!("fake-cc: failed to record argv");
            return ExitCode::from(1);
        }
    }
    if env::var("FAKE_CC_FAIL").ok().as_deref() == Some("1") {
        eprintln!("fake-cc: forced failure");
        return ExitCode::from(1);
    }
    let Some(path) = output else {
        eprintln!("fake-cc: missing -o");
        return ExitCode::from(1);
    };
    let asm = env::var("FAKE_CC_ASM").unwrap_or_default();
    if fs::write(path, asm).is_err() {
        eprintln!("fake-cc: failed to write assembly");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
