use std::process::ExitCode;
use surmount_stalwart_ops::run_add_token;

fn main() -> ExitCode {
    match run_add_token(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("add-stalwart-token: {err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
