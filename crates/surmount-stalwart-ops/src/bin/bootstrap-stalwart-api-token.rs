use std::process::ExitCode;
use surmount_stalwart_ops::run_bootstrap;

fn main() -> ExitCode {
    match run_bootstrap(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("bootstrap-stalwart-api-token: {err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
