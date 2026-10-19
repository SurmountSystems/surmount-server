use std::process::ExitCode;
use surmount_stalwart_ops::run_vm_lab_smtp_accept;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(rcpt) = args.next() else {
        eprintln!("vm-lab-smtp-accept: usage: vm-lab-smtp-accept local@example.test");
        return ExitCode::from(1);
    };
    if args.next().is_some() {
        eprintln!("vm-lab-smtp-accept: usage: vm-lab-smtp-accept local@example.test");
        return ExitCode::from(1);
    }
    let report = run_vm_lab_smtp_accept(&rcpt);
    for line in &report.lines {
        println!("{line}");
    }
    if let Some(err) = &report.error {
        eprintln!("vm-lab-smtp-accept: {err}");
    }
    if report.success {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
