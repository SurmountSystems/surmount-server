//! SHC customer user-api CLI (rDNS PTR + support tickets).
//!
//! Default is dry-run. Pass --live for the 409 confirmation_required dance
//! (`X-User-Api-Confirm`). Never prints ApiKey. Mock via SURMOUNT_RDNS_SHC_MOCK_DIR.

use std::io::Write;
use std::process::ExitCode;

use clap::Parser;
use surmount_shc::{Cli, run};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => ExitCode::from(code as u8),
        Err(err) => {
            let _ = writeln!(std::io::stderr(), "{err}");
            ExitCode::from(1)
        }
    }
}
