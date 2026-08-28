//! Operator face: `just rekey`. Prints SHC PGP paste. Never prints the age secret.

use std::process::ExitCode;

use surmount_rekey::{RekeyRequest, default_dir, default_gpg_recipient, run_rekey};

const USAGE: &str = "\
Usage:
  surmount-rekey [--help]
  surmount-rekey [--dry-run] [--dir DIR] [--gpg-recipient ADDR] [--gpg-bin PATH]

Mint an age identity (rage/age crate), wrap it with gpg, print the public
age1 recipient for SHC Backups re-key (type PGP). Does not print
AGE-SECRET-KEY-1. Does not talk to SHC.

Default GPG recipient: SURMOUNT_REKEY_GPG_RECIPIENT, else hunter@surmount.systems.
Default dir: $XDG_DATA_HOME/surmount/shc-backup (else ~/.local/share/surmount/shc-backup).

SHC still wants an SSH or passphrase primary. This tool's default output
tells you to tick this laptop's ssh-ed25519 as well.

Options:
  --dry-run          Skip gpg (tests / no keyring). Still writes identity 0600.
  --dir DIR          Output directory.
  --gpg-recipient A  gpg -r (must not start with '-').
  --gpg-bin PATH     gpg binary (default: gpg).
  -h, --help         Show this help.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("surmount-rekey: {}", e.message);
            ExitCode::from(1)
        }
    }
}

fn run(args: &[String]) -> Result<(), surmount_rekey::RekeyError> {
    let mut dry_run = false;
    let mut dir = default_dir();
    let mut gpg_recipient = default_gpg_recipient();
    let mut gpg_bin = std::env::var("SURMOUNT_REKEY_GPG_BIN")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "gpg".into());
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "--dry-run" => dry_run = true,
            "--dir" => {
                i += 1;
                if i >= args.len() {
                    return Err(surmount_rekey::RekeyError {
                        message: "--dir requires a value".into(),
                    });
                }
                dir = args[i].clone().into();
            }
            "--gpg-recipient" => {
                i += 1;
                if i >= args.len() {
                    return Err(surmount_rekey::RekeyError {
                        message: "--gpg-recipient requires a value".into(),
                    });
                }
                gpg_recipient = args[i].clone();
            }
            "--gpg-bin" => {
                i += 1;
                if i >= args.len() {
                    return Err(surmount_rekey::RekeyError {
                        message: "--gpg-bin requires a value".into(),
                    });
                }
                gpg_bin = args[i].clone();
            }
            other => {
                return Err(surmount_rekey::RekeyError {
                    message: format!("unknown argument: {other} (try --help)"),
                });
            }
        }
        i += 1;
    }
    let out = run_rekey(&RekeyRequest {
        dir,
        gpg_recipient,
        gpg_bin,
        dry_run,
    })?;
    print!("{}", out.card);
    Ok(())
}
