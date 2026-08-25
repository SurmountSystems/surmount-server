use std::process::ExitCode;
use surmount_stalwart_ops::run_point_mail_tls;

fn main() -> ExitCode {
    match run_point_mail_tls(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let tag = if err.exit_code == 2 { "BLOCKED: " } else { "" };
            eprintln!("point-stalwart-mail-tls: {tag}{err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
