//! Shared PASS/FAIL/SKIP row printer for e2e binaries.

use std::io::{self, Write};

#[derive(Debug, Default)]
pub struct Counters {
    pub pass: u32,
    pub fail: u32,
    pub skip: u32,
}

impl Counters {
    pub fn row(&mut self, status: Status, name: &str, detail: Option<&str>) {
        match status {
            Status::Pass => {
                self.pass += 1;
                println!("  [PASS] {name}");
            }
            Status::Fail => {
                self.fail += 1;
                if let Some(d) = detail {
                    println!("  [FAIL] {name}: {d}");
                } else {
                    println!("  [FAIL] {name}");
                }
            }
            Status::Skip => {
                self.skip += 1;
                if let Some(d) = detail {
                    println!("  [SKIP] {name} ({d})");
                } else {
                    println!("  [SKIP] {name}");
                }
            }
        }
        let _ = io::stdout().flush();
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Status {
    Pass,
    Fail,
    Skip,
}
