//! Domain posture audit CLI.

use std::process::ExitCode;

use surmount_domain_audit::run;

fn main() -> ExitCode {
    ExitCode::from(run(std::env::args_os()))
}
