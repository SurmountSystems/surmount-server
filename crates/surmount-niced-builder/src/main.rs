//! Niced ssh-ng nix-daemon helper. Not a mail wrapper.

use std::process::ExitCode;

use surmount_niced_builder::{parse_args, print_help, run, Mode, Settings};

fn main() -> ExitCode {
    match parse_args(std::env::args()) {
        Ok(Mode::Help) => {
            let _ = print_help();
            ExitCode::SUCCESS
        }
        Ok(Mode::Run { cmd }) => match Settings::from_env().and_then(|s| run(&s, &cmd)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("niced-builder: {err}");
                if err.message.contains("command required") {
                    eprint!("{}", surmount_niced_builder::USAGE);
                }
                ExitCode::from(err.exit_code.min(255) as u8)
            }
        },
        Err(err) => {
            eprintln!("niced-builder: {err}");
            eprint!("{}", surmount_niced_builder::USAGE);
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
