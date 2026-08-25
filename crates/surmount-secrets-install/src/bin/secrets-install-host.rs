use std::process::ExitCode;

use surmount_secrets_install::run_install;

fn main() -> ExitCode {
    match run_install(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("secrets-install-host: {err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
