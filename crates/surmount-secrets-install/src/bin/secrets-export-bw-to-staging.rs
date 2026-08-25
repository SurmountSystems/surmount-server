use std::process::ExitCode;

use surmount_secrets_install::run_export;

fn main() -> ExitCode {
    match run_export(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("secrets-export-bw-to-staging: {err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
