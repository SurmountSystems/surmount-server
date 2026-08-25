use std::process::ExitCode;

use surmount_deploy_host::discover_repo_root;
use surmount_host_cutover::{INVENTORY_USAGE, InventoryParse, parse_inventory_args, run_inventory};

fn main() -> ExitCode {
    match parse_inventory_args(std::env::args()) {
        Ok(InventoryParse::Help) => {
            print!("{INVENTORY_USAGE}");
            ExitCode::SUCCESS
        }
        Ok(InventoryParse::Run(opts)) => {
            let repo = if let Ok(p) = std::env::var("SURMOUNT_CUTOVER_REPO_ROOT") {
                if p.is_empty() {
                    discover_repo_root()
                } else {
                    std::path::PathBuf::from(p)
                }
            } else {
                discover_repo_root()
            };
            match run_inventory(&opts, &repo) {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    if err.message != "inventory incomplete" {
                        eprintln!("host-material-inventory: {err}");
                    }
                    ExitCode::from(err.exit_code.min(255) as u8)
                }
            }
        }
        Err(err) => {
            eprintln!("host-material-inventory: {err}");
            ExitCode::from(1)
        }
    }
}
