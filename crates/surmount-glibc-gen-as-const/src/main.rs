//! Command line for `surmount-glibc-gen-as-const`.
//!
//! glibc 2.44 `Makerules` calls this program in place of
//! `$(PYTHON) scripts/gen-as-const.py`.

fn main() {
    let mut args = std::env::args();
    let argv0 = args
        .next()
        .unwrap_or_else(|| "surmount-glibc-gen-as-const".to_string());
    let rest: Vec<String> = args.collect();
    let output = surmount_glibc_gen_as_const::run(&argv0, &rest);
    eprint!("{}", output.stderr);
    print!("{}", output.stdout);
    std::process::exit(output.code);
}
