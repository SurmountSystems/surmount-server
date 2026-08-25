//! Namecheap forward-zone CLI. Never prints ApiKey.

use std::process::ExitCode;

use surmount_dns_zone::run;

fn main() -> ExitCode {
    ExitCode::from(run(std::env::args_os()))
}
