//! Post-switch smoke (generation, units, loopback /health). Never logs PEM bodies.

use std::process::ExitCode;

use surmount_deploy_host::run_smoke;

fn main() -> ExitCode {
    match run_smoke(std::env::args()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            if !err.message.starts_with("unknown argument") {
                // PASS/FAIL lines already printed; keep a short trailer on stderr.
                if err.message.starts_with("post-switch smoke failed") {
                    return ExitCode::from(err.exit_code.min(255) as u8);
                }
            }
            eprintln!("deploy-host-post-switch-smoke: {err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
