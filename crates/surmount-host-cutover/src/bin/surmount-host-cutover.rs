use std::process::ExitCode;

use surmount_host_cutover::run;

fn main() -> ExitCode {
    match run(std::env::args()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("host-cutover: {err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
