use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(surmount_acme_namecheap::dispatch::run(std::env::args_os()))
}
