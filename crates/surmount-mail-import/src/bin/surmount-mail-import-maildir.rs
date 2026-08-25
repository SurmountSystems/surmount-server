use std::process::ExitCode;

use surmount_mail_import::run;

fn main() -> ExitCode {
    match run(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
