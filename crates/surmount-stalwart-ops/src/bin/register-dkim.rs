use std::process::ExitCode;
use surmount_stalwart_ops::run_register_dkim;

fn main() -> ExitCode {
    match run_register_dkim(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let tag = if err.exit_code == 2 { "BLOCKED: " } else { "" };
            eprintln!("register-dkim: {tag}{err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
