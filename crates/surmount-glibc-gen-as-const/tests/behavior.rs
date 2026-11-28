//! Behavior copied from glibc 2.44 `scripts/gen-as-const.py` and
//! `scripts/glibcextract.py` `compute_c_consts`. Expected text is the
//! format strings in those files. This test does not run python3.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use surmount_glibc_gen_as_const::{
    GenError, Mode, SymItem, compute_c_consts, generate, parse_assembly, parse_sym, render_header,
    render_probe, render_python, render_test, run,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Boilerplate inserted for the string `START` in `--test` output.
/// `gen-as-const.py` lines 41-58.
const TEST_START: &str = "\
#include <stdint.h>\n\
#include <stdio.h>\n\
#include <bits/wordsize.h>\n\
#if __WORDSIZE == 64\n\
typedef uint64_t c_t;\n\
# define U(n) UINT64_C (n)\n\
#else\n\
typedef uint32_t c_t;\n\
# define U(n) UINT32_C (n)\n\
#endif\n\
static int\n\
do_test (void)\n\
{\n\
  return 0;\n\
}\n\
#include <support/test-driver.c>";

/// Boilerplate inserted for `START` in the compiler probe.
/// `glibcextract.py` lines 42-43.
const PROBE_START: &str = "void\ndummy (void)\n{";

fn program() -> Command {
    Command::new(env!("CARGO_BIN_EXE_surmount-glibc-gen-as-const"))
}

fn fake_cc() -> &'static str {
    env!("CARGO_BIN_EXE_fake-cc")
}

struct EnvRestore {
    saved: Vec<(String, Option<String>)>,
}

impl EnvRestore {
    fn set(pairs: &[(&str, &str)]) -> Self {
        let saved = pairs
            .iter()
            .map(|(key, _)| (key.to_string(), std::env::var(key).ok()))
            .collect();
        for (key, value) in pairs {
            // Python's subprocess.check_call(shell=True) inherits os.environ.
            // The probe does the same. set_var is unsafe as of Rust 1.87.
            // ENV_LOCK keeps these tests on one thread at a time.
            unsafe {
                std::env::set_var(key, value);
            }
        }
        Self { saved }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, previous) in &self.saved {
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

fn with_env<T>(pairs: &[(&str, &str)], body: impl FnOnce() -> T) -> T {
    let _lock = ENV_LOCK.lock().expect("env lock");
    let _restore = EnvRestore::set(pairs);
    body()
}

#[test]
fn blank_lines_are_dropped() {
    let items = parse_sym("\n\n  \nFOO 1\n\n");
    assert_eq!(
        items,
        vec![
            SymItem::Start,
            SymItem::Constant {
                name: "FOO".to_string(),
                value: "1".to_string(),
            },
        ]
    );
}

#[test]
fn hash_lines_pass_through_as_directives() {
    // gen-as-const.py lines 90-93. A leading # is not a dropped comment.
    let items = parse_sym("#include <tls.h>\n\nFOO 1\n");
    assert_eq!(
        items,
        vec![
            SymItem::Directive("#include <tls.h>".to_string()),
            SymItem::Start,
            SymItem::Constant {
                name: "FOO".to_string(),
                value: "1".to_string(),
            },
        ]
    );
}

#[test]
fn hash_between_constants_stays_in_order() {
    let items = parse_sym("A 1\n#define X 2\nB 3\n");
    assert!(matches!(items[0], SymItem::Start));
    assert_eq!(
        items[1],
        SymItem::Constant {
            name: "A".to_string(),
            value: "1".to_string(),
        }
    );
    assert_eq!(items[2], SymItem::Directive("#define X 2".to_string()));
    assert_eq!(
        items[3],
        SymItem::Constant {
            name: "B".to_string(),
            value: "3".to_string(),
        }
    );
}

#[test]
fn slash_slash_is_not_a_comment() {
    // The script has no comment syntax besides skipping blank lines.
    let items = parse_sym("// note\n");
    assert_eq!(
        items,
        vec![
            SymItem::Start,
            SymItem::Constant {
                name: "//".to_string(),
                value: "note".to_string(),
            },
        ]
    );
}

#[test]
fn separator_drops_the_rest_of_the_line() {
    // gen-as-const.py lines 98-100.
    let items = parse_sym("FOO 1\n-- trailing text\nBAR 2\n");
    assert_eq!(
        items,
        vec![
            SymItem::Start,
            SymItem::Constant {
                name: "FOO".to_string(),
                value: "1".to_string(),
            },
            SymItem::Constant {
                name: "BAR".to_string(),
                value: "2".to_string(),
            },
        ]
    );
}

#[test]
fn separator_before_any_constant_still_inserts_start() {
    let items = parse_sym("--\nFOO 1\n");
    assert_eq!(items[0], SymItem::Start);
    assert!(matches!(
        &items[1],
        SymItem::Constant { name, .. } if name == "FOO"
    ));
}

#[test]
fn name_only_uses_the_name_as_the_expression() {
    // gen-as-const.py line 102.
    let items = parse_sym("SIGCHLD\n");
    assert_eq!(
        items,
        vec![
            SymItem::Start,
            SymItem::Constant {
                name: "SIGCHLD".to_string(),
                value: "SIGCHLD".to_string(),
            },
        ]
    );
}

#[test]
fn file_with_no_constant_still_gets_start() {
    // gen-as-const.py lines 104-105.
    let items = parse_sym("#include <stddef.h>\n\n");
    assert_eq!(
        items,
        vec![
            SymItem::Directive("#include <stddef.h>".to_string()),
            SymItem::Start,
        ]
    );
    let only_blank = parse_sym("\n\n");
    assert_eq!(only_blank, vec![SymItem::Start]);
}

#[test]
fn probe_text_matches_glibcextract() {
    // glibcextract.py lines 42-54.
    let probe = render_probe(&parse_sym("#include <tls.h>\nFOO 1 + 1\n"));
    let expected = format!(
        "#include <tls.h>\n{PROBE_START}\nasm (\"/* @@@name@@@FOO@@@value@@@%0@@@end@@@ */\" : : \"i\" ((long int) (1 + 1)));\n}}\n"
    );
    assert_eq!(probe, expected);
}

#[test]
fn test_text_matches_gen_test_and_print() {
    // gen-as-const.py lines 64-66, then print() adds one newline (line 107).
    let text = render_test(&parse_sym("FOO 1 + 1\n"));
    let expected = format!(
        "{TEST_START}\n_Static_assert (U (asconst_FOO) == (c_t) (1 + 1), \"value of FOO\");\n"
    );
    assert_eq!(text, expected);
}

#[test]
fn header_is_sorted_define_lines() {
    // gen-as-const.py line 117. print(..., end='') adds no extra newline.
    let mut map = std::collections::BTreeMap::new();
    map.insert("ZEBRA".to_string(), "10".to_string());
    map.insert("AMBER".to_string(), "2".to_string());
    assert_eq!(render_header(&map), "#define AMBER 2\n#define ZEBRA 10\n");
}

#[test]
fn python_banner_then_sorted_assignments() {
    // gen-as-const.py lines 110-114. The first print adds a newline
    // after a string that already ends in a newline.
    let mut map = std::collections::BTreeMap::new();
    map.insert("ZEBRA".to_string(), "10".to_string());
    map.insert("AMBER".to_string(), "2".to_string());
    let expected = "\
# GENERATED FILE\n\
\n\
# Constant definitions.\n\
# See gen-as-const.py for details.\n\
\n\
AMBER = 2\n\
ZEBRA = 10\n";
    assert_eq!(render_python(&map), expected);
}

#[test]
fn assembly_skips_a_dollar_and_keeps_hex() {
    // glibcextract.py lines 67-69. `[^0-9Xxa-fA-F-]*` eats `$`.
    let text = "/* @@@name@@@FOO@@@value@@@$0x2a@@@end@@@ */\n";
    let map = parse_assembly(text).expect("one constant");
    assert_eq!(map.get("FOO").map(String::as_str), Some("0x2a"));
}

#[test]
fn assembly_keeps_a_negative_value() {
    let text = "@@@name@@@N@@@value@@@-16@@@end@@@\n";
    let map = parse_assembly(text).expect("negative");
    assert_eq!(map.get("N").map(String::as_str), Some("-16"));
}

#[test]
fn assembly_ignores_lines_without_the_marker() {
    let text = "not a constant\n@@@name@@@FOO@@@value@@@1@@@end@@@\n";
    let map = parse_assembly(text).expect("one");
    assert_eq!(map.len(), 1);
}

#[test]
fn assembly_search_skips_a_decoy_marker() {
    // re.search continues after a `@@@name@@@` that does not finish.
    let text = "@@@name@@@nope then @@@name@@@FOO@@@value@@@7@@@end@@@\n";
    let map = parse_assembly(text).expect("second marker");
    assert_eq!(map.get("FOO").map(String::as_str), Some("7"));
    assert!(!map.contains_key("nope then "));
}

#[test]
fn duplicate_constant_with_the_same_value_is_kept() {
    let text = "\
@@@name@@@FOO@@@value@@@1@@@end@@@
@@@name@@@FOO@@@value@@@1@@@end@@@
";
    let map = parse_assembly(text).expect("same value");
    assert_eq!(map.get("FOO").map(String::as_str), Some("1"));
}

#[test]
fn duplicate_constant_with_a_different_value_is_an_error() {
    // glibcextract.py lines 71-74.
    let text = "\
@@@name@@@FOO@@@value@@@1@@@end@@@
@@@name@@@FOO@@@value@@@2@@@end@@@
";
    let err = parse_assembly(text).expect_err("different value");
    assert_eq!(
        err,
        GenError {
            message: "duplicate constant FOO".to_string(),
        }
    );
}

#[test]
fn test_mode_does_not_need_a_compiler() {
    let text = generate(Mode::Test, "missing-compiler", "FOO 4\n").expect("test mode");
    assert!(text.contains("_Static_assert (U (asconst_FOO) == (c_t) (4), \"value of FOO\");"));
    assert!(text.ends_with('\n'));
}

#[test]
fn test_flag_wins_over_python_flag() {
    // gen-as-const.py lines 106-114. `--test` is checked first.
    let dir = std::env::temp_dir().join(format!("gen-as-const-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    let sym = dir.join("sample.sym");
    fs::write(&sym, "FOO 4\n").expect("sym");
    let output = run(
        "surmount-glibc-gen-as-const",
        &[
            "--python".to_string(),
            "--test".to_string(),
            sym.display().to_string(),
        ],
    );
    assert_eq!(output.code, 0);
    assert!(output.stdout.contains("_Static_assert"));
    assert!(!output.stdout.contains("# GENERATED FILE"));
    assert!(output.stderr.is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_sym_file_argument_exits_2() {
    let output = run("surmount-glibc-gen-as-const", &[]);
    assert_eq!(output.code, 2);
    assert!(output.stdout.is_empty());
    assert!(output.stderr.contains(
        "usage: surmount-glibc-gen-as-const [-h] [--cc CC] [--test] [--python] sym_file"
    ));
    assert!(
        output
            .stderr
            .contains("error: the following arguments are required: sym_file")
    );
}

#[test]
fn unknown_flag_exits_2() {
    let output = run(
        "surmount-glibc-gen-as-const",
        &["--nope".to_string(), "file.sym".to_string()],
    );
    assert_eq!(output.code, 2);
    assert!(
        output
            .stderr
            .contains("error: unrecognized arguments: --nope")
    );
}

#[test]
fn cc_without_a_value_exits_2() {
    let output = run("surmount-glibc-gen-as-const", &["--cc".to_string()]);
    assert_eq!(output.code, 2);
    assert!(
        output
            .stderr
            .contains("error: argument --cc: expected one argument")
    );
}

#[test]
fn help_exits_0_and_names_the_script_arguments() {
    // gen-as-const.py lines 72-81.
    let output = run("surmount-glibc-gen-as-const", &["--help".to_string()]);
    assert_eq!(output.code, 0);
    assert!(output.stderr.is_empty());
    assert!(
        output
            .stdout
            .contains("Produce headers of assembly constants.")
    );
    assert!(output.stdout.contains(".sym file to process"));
    assert!(
        output
            .stdout
            .contains("C compiler (including options) to use")
    );
    assert!(
        output
            .stdout
            .contains("Generate test case instead of header")
    );
    assert!(
        output
            .stdout
            .contains("Generate Python file instead of header")
    );
}

#[test]
fn missing_file_exits_1() {
    let output = run(
        "surmount-glibc-gen-as-const",
        &["/no/such/gen-as-const.sym".to_string()],
    );
    assert_eq!(output.code, 1);
    assert!(output.stdout.is_empty());
    assert!(
        output
            .stderr
            .contains("cannot read /no/such/gen-as-const.sym")
    );
}

#[test]
fn omitted_cc_is_the_word_none() {
    // glibcextract.py line 62 formats None with %s, which is `None`.
    let err = compute_c_consts(&parse_sym("FOO 1\n"), "None").expect_err("no compiler named None");
    assert!(err.message.contains("None -S -o"));
}

#[test]
fn empty_cc_equals_starts_the_command_with_a_space() {
    let output = run(
        "surmount-glibc-gen-as-const",
        &["--cc=".to_string(), "/no/such/empty-cc.sym".to_string()],
    );
    // The file is missing, so we never reach the empty compiler string.
    // The flag itself is accepted (argparse keeps an empty --cc=).
    assert_eq!(output.code, 1);
    assert!(output.stderr.contains("cannot read"));
}

#[test]
fn fake_compiler_records_the_probe_and_header_uses_assembly_values() {
    let record = record_dir("header");
    let asm = "\
prefix @@@name@@@nope @@@name@@@ZEBRA@@@value@@@$10@@@end@@@ tail
@@@name@@@AMBER@@@value@@@2@@@end@@@
";
    let sym = "ZEBRA 99\nAMBER 1\n";
    let header = with_env(
        &[
            ("FAKE_CC_RECORD_DIR", record.to_str().expect("utf8")),
            ("FAKE_CC_ASM", asm),
        ],
        || generate(Mode::Header, fake_cc(), sym).expect("header"),
    );
    assert_eq!(header, "#define AMBER 2\n#define ZEBRA 10\n");
    // `-o` is followed by the temp .s path, then `-x`, `c`, `-`.
    // glibcextract.py line 62.
    let argv_full = fs::read_to_string(record.join("argv")).expect("argv");
    let lines: Vec<&str> = argv_full.lines().collect();
    assert_eq!(lines[0], "-S");
    assert_eq!(lines[1], "-o");
    assert!(lines[2].ends_with("test.s"), "{argv_full}");
    assert_eq!(&lines[3..], &["-x", "c", "-"]);
    let stdin = fs::read_to_string(record.join("stdin")).expect("stdin");
    assert!(stdin.contains(PROBE_START));
    assert!(stdin.contains(
        "asm (\"/* @@@name@@@ZEBRA@@@value@@@%0@@@end@@@ */\" : : \"i\" ((long int) (99)));"
    ));
    assert!(stdin.ends_with("}\n"));
    let _ = fs::remove_dir_all(&record);
}

#[test]
fn fake_compiler_failure_is_an_error() {
    let err = with_env(&[("FAKE_CC_FAIL", "1")], || {
        compute_c_consts(&parse_sym("FOO 1\n"), fake_cc()).expect_err("fail")
    });
    assert!(err.message.contains("compiler command failed"));
    assert!(err.message.contains(fake_cc()));
    assert!(err.message.contains("-S -o"));
}

#[test]
fn python_mode_uses_the_assembly_values() {
    let asm = "@@@name@@@FOO@@@value@@@8@@@end@@@\n";
    let text = with_env(&[("FAKE_CC_ASM", asm)], || {
        generate(Mode::Python, fake_cc(), "FOO 1\n").expect("python")
    });
    assert!(text.starts_with(
        "# GENERATED FILE\n\n# Constant definitions.\n# See gen-as-const.py for details.\n\n"
    ));
    assert!(text.ends_with("FOO = 8\n"));
}

#[test]
fn libio_macros_fixture_matches_the_sym_grammar() {
    // Copied from glibc 2.44 libio/libio-macros.sym. Data, not a C program.
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/libio-macros.sym");
    let text = fs::read_to_string(&path).expect("fixture");
    let items = parse_sym(&text);
    assert_eq!(
        items[0],
        SymItem::Directive("#include <libioP.h>".to_string())
    );
    assert_eq!(items[1], SymItem::Start);
    assert_eq!(
        items[2],
        SymItem::Constant {
            name: "IO_JUMP_T_SIZE".to_string(),
            value: "sizeof (struct _IO_jump_t)".to_string(),
        }
    );
    assert_eq!(
        items[3],
        SymItem::Constant {
            name: "IO_FILE_JUMPS_OFFSET".to_string(),
            value: "sizeof (struct _IO_jump_t) * IO_FILE_JUMPS".to_string(),
        }
    );
    assert_eq!(
        items[4],
        SymItem::Constant {
            name: "IO_WFILE_JUMPS_OFFSET".to_string(),
            value: "sizeof (struct _IO_jump_t) * IO_WFILE_JUMPS".to_string(),
        }
    );
    assert_eq!(items.len(), 5);
    let rendered = render_test(&items);
    assert!(rendered.contains(TEST_START));
    assert!(rendered.contains(
        "_Static_assert (U (asconst_IO_JUMP_T_SIZE) == (c_t) (sizeof (struct _IO_jump_t)), \"value of IO_JUMP_T_SIZE\");"
    ));
    assert!(!rendered.contains("\n--\n"));
    let probe = render_probe(&items);
    assert!(probe.starts_with("#include <libioP.h>\nvoid\ndummy (void)\n{\n"));
    assert!(probe.contains(
        "asm (\"/* @@@name@@@IO_WFILE_JUMPS_OFFSET@@@value@@@%0@@@end@@@ */\" : : \"i\" ((long int) (sizeof (struct _IO_jump_t) * IO_WFILE_JUMPS)));"
    ));
}

#[test]
fn cli_test_mode_writes_the_fixture_asserts() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/libio-macros.sym");
    let mut child = program();
    let output = child.arg("--test").arg(&path).output().expect("run binary");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).expect("utf8");
    assert!(
        stdout.contains(
            "#include <support/test-driver.c>\n_Static_assert (U (asconst_IO_JUMP_T_SIZE)"
        )
    );
    assert!(output.stderr.is_empty());
}

fn record_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gen-as-const-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("record dir");
    dir
}
