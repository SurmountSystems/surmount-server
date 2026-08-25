//! D1 host hybrid TLS negotiation probe.

use std::process::ExitCode;

use surmount_host_probe::tls_hybrid::run_probe;

fn main() -> ExitCode {
    match run_probe() {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(e.exit_code.min(255) as u8)
        }
    }
}
