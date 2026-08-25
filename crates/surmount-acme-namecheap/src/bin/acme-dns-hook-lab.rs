use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(surmount_acme_namecheap::lab::run(std::env::args_os()))
}
