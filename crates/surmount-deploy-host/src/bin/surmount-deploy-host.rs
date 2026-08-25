//! Operator-driven deploy for flake attr #mail-vps.

use std::process::ExitCode;

use surmount_deploy_host::run;

fn main() -> ExitCode {
    match run(std::env::args()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("deploy-host: {err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
