//! Read the host systemd journal (follow or `--status`). No sudo.

use std::process::ExitCode;

use surmount_host_logs::run;

fn main() -> ExitCode {
    match run(std::env::args()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("host-logs: {err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
