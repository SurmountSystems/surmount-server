use std::process::ExitCode;
use surmount_stalwart_ops::run_recovery_unlock;

fn main() -> ExitCode {
    match run_recovery_unlock(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let tag = if err.exit_code == 2 { "BLOCKED: " } else { "" };
            eprintln!("stalwart-recovery-unlock: {tag}{err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
