use std::process::ExitCode;
use surmount_stalwart_ops::run_free_443;

fn main() -> ExitCode {
    match run_free_443(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let tag = if err.exit_code == 2 { "BLOCKED: " } else { "" };
            eprintln!("free-stalwart-public-443: {tag}{err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
