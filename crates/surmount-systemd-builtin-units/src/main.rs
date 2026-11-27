//! Command line for `surmount-systemd-builtin-units`.
//!
//! systemd 262.2 `src/core/meson.build` calls this program in place of
//! `python src/core/generate-builtin-units.py`. Arguments are unit file
//! paths. Stdout is the text captured as `builtin-units.inc`.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let output = surmount_systemd_builtin_units::run(&paths);
    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    if output.code == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
