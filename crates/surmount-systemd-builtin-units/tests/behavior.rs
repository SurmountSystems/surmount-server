//! Expected rows are the print in systemd 262.2
//! `src/core/generate-builtin-units.py` lines 17-33.
//! This test does not run python3.
//!
//! Accessed: 2026-11-26.
//! <https://raw.githubusercontent.com/systemd/systemd/v262.2/src/core/generate-builtin-units.py>

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use surmount_systemd_builtin_units::run;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn program() -> Command {
    Command::new(env!("CARGO_BIN_EXE_surmount-systemd-builtin-units"))
}

fn scratch() -> PathBuf {
    let n = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("surmount-builtin-units-{}-{n}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_file(dir: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, bytes).unwrap();
    path
}

struct Case {
    stdout: String,
    stderr: String,
    code: i32,
}

fn from_lib(paths: &[PathBuf]) -> Case {
    let output = run(paths);
    Case {
        stdout: output.stdout,
        stderr: output.stderr,
        code: output.code,
    }
}

fn from_bin(paths: &[PathBuf]) -> Case {
    let mut cmd = program();
    for path in paths {
        cmd.arg(path);
    }
    let output = cmd.output().unwrap();
    Case {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        code: output.status.code().unwrap_or(1),
    }
}

fn assert_case(paths: &[PathBuf], code: i32, stdout: &str) {
    for case in [from_lib(paths), from_bin(paths)] {
        assert_eq!(case.code, code, "stderr={}", case.stderr);
        assert_eq!(case.stdout, stdout);
    }
}

fn row(name: &str, escaped: &str) -> String {
    format!("{{ \"{name}\", \"{escaped}\" }},\n")
}

#[test]
fn no_arguments_print_nothing() {
    assert_case(&[], 0, "");
}

#[test]
fn strips_blank_lines_and_hash_or_semicolon_comments() {
    let dir = scratch();
    let path = write_file(
        &dir,
        "basic.target",
        b"Description=Basic\n\n# comment\n; also\nAfter=sysinit.target\n",
    );
    let expected = row(
        "basic.target",
        "Description=Basic\\nAfter=sysinit.target\\n",
    );
    assert_case(&[path], 0, &expected);
}

#[test]
fn empty_file_is_one_escaped_newline() {
    let dir = scratch();
    let path = write_file(&dir, "empty.service", b"");
    assert_case(&[path], 0, &row("empty.service", "\\n"));
}

#[test]
fn only_comments_is_one_escaped_newline() {
    let dir = scratch();
    let path = write_file(&dir, "only.service", b"# a\n; b\n\n");
    assert_case(&[path], 0, &row("only.service", "\\n"));
}

#[test]
fn escapes_backslash_then_quote_then_newline() {
    let dir = scratch();
    let path = write_file(&dir, "quote.service", b"a\\b\"c\n");
    assert_case(&[path], 0, &row("quote.service", "a\\\\b\\\"c\\n"));
}

#[test]
fn does_not_escape_dollar_or_trailing_hash() {
    let dir = scratch();
    let path = write_file(
        &dir,
        "env.service",
        b"Environment=FOO=${BAR}\nFoo=bar # trailing\n",
    );
    let expected = row(
        "env.service",
        "Environment=FOO=${BAR}\\nFoo=bar # trailing\\n",
    );
    assert_case(&[path], 0, &expected);
}

#[test]
fn keeps_a_line_whose_first_character_is_not_a_comment_mark() {
    let dir = scratch();
    let path = write_file(&dir, "space.service", b" # not\n ; also\n   \nKeep\n");
    let expected = row("space.service", " # not\\n ; also\\n   \\nKeep\\n");
    assert_case(&[path], 0, &expected);
}

#[test]
fn basename_is_the_final_component_and_is_not_escaped() {
    let dir = scratch();
    let nested = write_file(&dir, "units/reboot.target", b"A\n");
    let quoted = write_file(&dir, "a\"b.service", b"Z\n");
    let slashed = write_file(&dir, "a\\b.service", b"Z\n");
    assert_case(&[nested], 0, &row("reboot.target", "A\\n"));
    assert_case(&[quoted], 0, &row("a\"b.service", "Z\\n"));
    assert_case(&[slashed], 0, &row("a\\b.service", "Z\\n"));
}

#[test]
fn splitlines_breaks_are_not_rejected() {
    let dir = scratch();
    let crlf = write_file(&dir, "crlf.service", b"A\r\nB\r\n");
    let cr = write_file(&dir, "cr.service", b"A\rB");
    let vt = write_file(&dir, "vt.service", b"A\x0bB");
    let ff = write_file(&dir, "ff.service", b"A\x0cB");
    let sep = write_file(&dir, "sep.service", "A\u{2028}B".as_bytes());
    let next = write_file(&dir, "next.service", "A\u{0085}B".as_bytes());
    let collapsed = write_file(&dir, "blank.service", b"A\n\nB\n");
    let expected = row("crlf.service", "A\\nB\\n");
    assert_case(&[crlf], 0, &expected);
    assert_case(&[cr], 0, &row("cr.service", "A\\nB\\n"));
    assert_case(&[vt], 0, &row("vt.service", "A\\nB\\n"));
    assert_case(&[ff], 0, &row("ff.service", "A\\nB\\n"));
    assert_case(&[sep], 0, &row("sep.service", "A\\nB\\n"));
    assert_case(&[next], 0, &row("next.service", "A\\nB\\n"));
    assert_case(&[collapsed], 0, &row("blank.service", "A\\nB\\n"));
}

#[test]
fn no_trailing_newline_still_gets_one() {
    let dir = scratch();
    let path = write_file(&dir, "one.service", b"Keep");
    assert_case(&[path], 0, &row("one.service", "Keep\\n"));
}

#[test]
fn semicolon_inside_a_line_is_kept() {
    let dir = scratch();
    let path = write_file(&dir, "semi.service", b"Foo=;bar\n");
    assert_case(&[path], 0, &row("semi.service", "Foo=;bar\\n"));
}

#[test]
fn two_files_keep_argv_order() {
    let dir = scratch();
    let first = write_file(&dir, "first.service", b"One\n");
    let second = write_file(&dir, "second.service", b"Two\n");
    let expected = format!(
        "{}{}",
        row("first.service", "One\\n"),
        row("second.service", "Two\\n")
    );
    assert_case(&[first, second], 0, &expected);
}

#[test]
fn tab_is_rejected_and_earlier_rows_stay() {
    let dir = scratch();
    let good = write_file(&dir, "good.service", b"Ok\n");
    let bad = write_file(&dir, "bad.service", b"A\tB\n");
    assert_case(&[bad.clone()], 1, "");
    assert_case(&[good, bad], 1, &row("good.service", "Ok\\n"));
}

#[test]
fn delete_and_non_utf8_are_rejected() {
    let dir = scratch();
    let del = write_file(&dir, "del.service", b"A\x7fB\n");
    let bad = write_file(&dir, "bad.service", b"A\xffB\n");
    let bom = write_file(&dir, "bom.service", "\u{feff}A\n".as_bytes());
    assert_case(&[del], 1, "");
    assert_case(&[bad], 1, "");
    assert_case(&[bom], 1, "");
}

#[test]
fn missing_file_is_rejected_after_earlier_rows() {
    let dir = scratch();
    let good = write_file(&dir, "good.service", b"Ok\n");
    let missing = dir.join("missing.service");
    assert_case(&[missing.clone()], 1, "");
    assert_case(&[good, missing], 1, &row("good.service", "Ok\\n"));
}
