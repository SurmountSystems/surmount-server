use std::process::ExitCode;
use surmount_stalwart_ops::run_vm_lab_directory_guest;

fn main() -> ExitCode {
    match run_vm_lab_directory_guest(std::env::args().skip(1)) {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("vm-lab-directory-setup: {err}");
            ExitCode::from(err.exit_code.min(255) as u8)
        }
    }
}
