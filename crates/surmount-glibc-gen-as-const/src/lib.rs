//! glibc 2.44 `scripts/gen-as-const.py` and the `compute_c_consts` function
//! it calls from `scripts/glibcextract.py`.
//!
//! The behavior follows those two files on the glibc 2.44 tag
//! (`scripts/gen-as-const.py`, 120 lines, and `scripts/glibcextract.py`).
//! Line numbers below are from that tag:
//! <https://gitlab.com/gnutools/glibc/-/blob/glibc-2.44/scripts/gen-as-const.py>
//! (accessed: 2026-11-25). The same text is in the release tarball
//! `glibc-2.44.tar.xz`.
//!
//! `.sym` loop, `scripts/gen-as-const.py` lines 86-117:
//!
//! ```text
//!         for line in sym_file:
//!             line = line.strip()
//!             if line == '':
//!                 continue
//!             # Pass preprocessor directives through.
//!             if line.startswith('#'):
//!                 sym_data.append(line)
//!                 continue
//!             words = line.split(maxsplit=1)
//!             if not started:
//!                 sym_data.append('START')
//!                 started = True
//!             # Separator.
//!             if words[0] == '--':
//!                 continue
//!             name = words[0]
//!             value = words[1] if len(words) > 1 else words[0]
//!             sym_data.append((name, value))
//!         if not started:
//!             sym_data.append('START')
//!     if args.test:
//!         print(gen_test(sym_data))
//!     elif args.python:
//!         consts = glibcextract.compute_c_consts(sym_data, args.cc)
//!         print('# GENERATED FILE\n'
//!               '\n'
//!               '# Constant definitions.\n'
//!               '# See gen-as-const.py for details.\n')
//!         print(''.join('%s = %s\n' % c for c in sorted(consts.items())), end='')
//!     else:
//!         consts = glibcextract.compute_c_consts(sym_data, args.cc)
//!         print(''.join('#define %s %s\n' % c for c in sorted(consts.items())), end='')
//! ```
//!
//! Compiler probe, `scripts/glibcextract.py` lines 42-43, 49-51, and 62.
//! Line 50 spells `\"` inside a single-quoted string. Python turns that
//! escape into one double quote, so the probe text contains `"i"`.
//!
//! ```text
//!             if arg == 'START':
//!                 out_lines.append('void\ndummy (void)\n{')
//!         out_lines.append('asm ("/* @@@name@@@%s@@@value@@@%%0@@@end@@@ */" '
//!                          ': : \"i\" ((long int) (%s)));'
//!                          % (name, value))
//!         cmd = ('%s -S -o %s -x c - < %s' % (cc, s_file_name, c_file_name))
//! ```
//!
//! Assembly match and the duplicate error, lines 67-74:
//!
//! ```text
//!                 match = re.search('@@@name@@@([^@]*)'
//!                                   '@@@value@@@[^0-9Xxa-fA-F-]*'
//!                                   '([0-9Xxa-fA-F-]+).*@@@end@@@', line)
//!                         raise ValueError('duplicate constant %s'
//!                                          % match.group(1))
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// Text inserted where the Python script records the string `START`
/// while generating a C test (`gen-as-const.py` lines 41-58).
pub const TEST_START: &str = "\
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

/// Text inserted where the Python script records `START` in the compiler
/// probe (`glibcextract.py` lines 42-43).
pub const PROBE_START: &str = "void\ndummy (void)\n{";

/// One item from a `.sym` file, in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymItem {
    /// A stripped line that starts with `#`. Passed through unchanged.
    Directive(String),
    /// Marker inserted before the first constant line, or at end of file
    /// when the file has no constant line (`gen-as-const.py` lines 95-105).
    Start,
    /// A name and the C expression whose value is that constant.
    Constant { name: String, value: String },
}

/// Failure from the compiler probe or from reading the `.sym` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenError {
    /// Message. Duplicate constants use `duplicate constant {name}`,
    /// which is `glibcextract.py` lines 73-74.
    pub message: String,
}

impl std::fmt::Display for GenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GenError {}

/// What the three glibc makefile recipes ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `#define` lines. The default when `--test` and `--python` are absent.
    Header,
    /// C compilation test. `--test` wins when `--python` is also set
    /// (`gen-as-const.py` lines 106-117).
    Test,
    /// Python assignments. `--python`.
    Python,
}

/// stdout, stderr, and the process status for one invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    /// Process status. `2` is a usage error. `1` is a read or compiler error.
    pub code: i32,
    /// Text that would be written to stdout. Header and Python modes already
    /// include the newlines the Python `print` calls emit.
    pub stdout: String,
    /// Text that would be written to stderr.
    pub stderr: String,
}

/// Parse a `.sym` file the way `gen-as-const.py` `main` does (lines 86-105).
///
/// Blank lines are dropped. A line whose first non-whitespace character is
/// `#` is a preprocessor directive and is kept. `--` as the first word is
/// the separator and is dropped, including any text after it on that line.
/// The first non-directive line inserts [`SymItem::Start`] before that line
/// is classified. A file with no such line still gets one `Start` at the end.
/// A line with only a name uses that name as the expression (line 102).
pub fn parse_sym(text: &str) -> Vec<SymItem> {
    let mut items = Vec::new();
    let mut started = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            items.push(SymItem::Directive(line.to_string()));
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("");
        let rest = parts.next().map(str::trim).unwrap_or("");
        if !started {
            items.push(SymItem::Start);
            started = true;
        }
        if name == "--" {
            continue;
        }
        let value = if rest.is_empty() {
            name.to_string()
        } else {
            rest.to_string()
        };
        items.push(SymItem::Constant {
            name: name.to_string(),
            value,
        });
    }
    if !started {
        items.push(SymItem::Start);
    }
    items
}

/// C probe passed to the compiler (`glibcextract.py` lines 39-54).
///
/// Each constant becomes one `asm` line. `%%0` in the Python format string
/// is `%0` in the probe. The source line spells `\"i\"`. That escape is
/// one `"` in the probe. Names are not sorted here. Sorting happens when
/// the header or Python file is printed.
pub fn render_probe(items: &[SymItem]) -> String {
    let mut lines = Vec::new();
    for item in items {
        match item {
            SymItem::Directive(text) => lines.push(text.clone()),
            SymItem::Start => lines.push(PROBE_START.to_string()),
            SymItem::Constant { name, value } => lines.push(format!(
                "asm (\"/* @@@name@@@{name}@@@value@@@%0@@@end@@@ */\" : : \"i\" ((long int) ({value})));"
            )),
        }
    }
    lines.push("}".to_string());
    lines.push(String::new());
    lines.join("\n")
}

/// C test printed for `--test` (`gen-as-const.py` lines 31-67 and 106-107).
///
/// `print` adds one trailing newline after the joined lines.
pub fn render_test(items: &[SymItem]) -> String {
    let mut parts = Vec::new();
    for item in items {
        match item {
            SymItem::Directive(text) => parts.push(text.clone()),
            SymItem::Start => parts.push(TEST_START.to_string()),
            SymItem::Constant { name, value } => parts.push(format!(
                "_Static_assert (U (asconst_{name}) == (c_t) ({value}), \"value of {name}\");"
            )),
        }
    }
    let mut out = parts.join("\n");
    out.push('\n');
    out
}

/// `#define` lines in sorted name order (`gen-as-const.py` line 117).
pub fn render_header(consts: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (name, value) in consts {
        out.push_str("#define ");
        out.push_str(name);
        out.push(' ');
        out.push_str(value);
        out.push('\n');
    }
    out
}

/// Python assignments in sorted name order (`gen-as-const.py` lines 110-114).
///
/// The first `print` adds a newline after a string that already ends in a
/// newline, so there is a blank line between the comment and the constants.
pub fn render_python(consts: &BTreeMap<String, String>) -> String {
    let mut out = String::from(
        "# GENERATED FILE\n\n# Constant definitions.\n# See gen-as-const.py for details.\n\n",
    );
    for (name, value) in consts {
        out.push_str(name);
        out.push_str(" = ");
        out.push_str(value);
        out.push('\n');
    }
    out
}

/// Read constants out of compiler assembly (`glibcextract.py` lines 64-76).
///
/// The pattern is `@@@name@@@`, a name with no `@`, `@@@value@@@`, a run of
/// characters that are not hex digits or `-`, then a hex-or-minus value, then
/// later `@@@end@@@`. The same name with the same value is kept. The same
/// name with a different value is `duplicate constant {name}`.
pub fn parse_assembly(text: &str) -> Result<BTreeMap<String, String>, GenError> {
    let mut consts = BTreeMap::new();
    for line in text.lines() {
        let Some((name, value)) = assembly_match(line) else {
            continue;
        };
        if let Some(previous) = consts.get(&name) {
            if previous != &value {
                return Err(GenError {
                    message: format!("duplicate constant {name}"),
                });
            }
            continue;
        }
        consts.insert(name, value);
    }
    Ok(consts)
}

/// Compile the probe and parse the assembly (`glibcextract.py` lines 55-76).
///
/// `cc` is interpolated without extra quotes, matching line 62:
/// `{cc} -S -o {s_file} -x c - < {c_file}`. The shell is `sh -c`.
/// When the makefile omits `--cc`, Python formats `None` with `%s`, which
/// is the word `None`. Callers that want that pass `cc` as `"None"`.
pub fn compute_c_consts(items: &[SymItem], cc: &str) -> Result<BTreeMap<String, String>, GenError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("surmount-gen-as-const-{}-{n}", std::process::id()));
    let _guard = TempDir(dir.clone());
    fs::create_dir_all(&dir).map_err(|err| GenError {
        message: format!("cannot create {}: {err}", dir.display()),
    })?;
    let c_file = dir.join("test.c");
    let s_file = dir.join("test.s");
    let probe = render_probe(items);
    let mut file = fs::File::create(&c_file).map_err(|err| GenError {
        message: format!("cannot write {}: {err}", c_file.display()),
    })?;
    file.write_all(probe.as_bytes()).map_err(|err| GenError {
        message: format!("cannot write {}: {err}", c_file.display()),
    })?;
    drop(file);
    let cmd = format!(
        "{cc} -S -o {} -x c - < {}",
        shell_path(&s_file),
        shell_path(&c_file)
    );
    let status = Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .status()
        .map_err(|err| GenError {
            message: format!("cannot run compiler command `{cmd}`: {err}"),
        })?;
    if !status.success() {
        return Err(GenError {
            message: format!("compiler command failed: {cmd}"),
        });
    }
    let asm = fs::read_to_string(&s_file).map_err(|err| GenError {
        message: format!("cannot read {}: {err}", s_file.display()),
    })?;
    parse_assembly(&asm)
}

/// Produce the stdout text for one `.sym` document.
pub fn generate(mode: Mode, cc: &str, sym_text: &str) -> Result<String, GenError> {
    let items = parse_sym(sym_text);
    match mode {
        Mode::Test => Ok(render_test(&items)),
        Mode::Header => {
            let consts = compute_c_consts(&items, cc)?;
            Ok(render_header(&consts))
        }
        Mode::Python => {
            let consts = compute_c_consts(&items, cc)?;
            Ok(render_python(&consts))
        }
    }
}

/// Run the command line. `argv0` is the program name. `args` excludes it.
pub fn run(argv0: &str, args: &[String]) -> RunOutput {
    match parse_cli(argv0, args) {
        Cli::Help(text) => RunOutput {
            code: 0,
            stdout: text,
            stderr: String::new(),
        },
        Cli::Usage(text) => RunOutput {
            code: 2,
            stdout: String::new(),
            stderr: text,
        },
        Cli::Work { mode, cc, sym_file } => match fs::read_to_string(&sym_file) {
            Ok(text) => match generate(mode, cc.as_deref().unwrap_or("None"), &text) {
                Ok(stdout) => RunOutput {
                    code: 0,
                    stdout,
                    stderr: String::new(),
                },
                Err(err) => RunOutput {
                    code: 1,
                    stdout: String::new(),
                    stderr: format!("{err}\n"),
                },
            },
            Err(err) => RunOutput {
                code: 1,
                stdout: String::new(),
                stderr: format!("cannot read {sym_file}: {err}\n"),
            },
        },
    }
}

enum Cli {
    Help(String),
    Usage(String),
    Work {
        mode: Mode,
        cc: Option<String>,
        sym_file: String,
    },
}

fn parse_cli(argv0: &str, args: &[String]) -> Cli {
    let mut test = false;
    let mut python = false;
    let mut cc: Option<String> = None;
    let mut sym_file: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "-h" || arg == "--help" {
            return Cli::Help(help_text(argv0));
        }
        if arg == "--test" {
            test = true;
            index += 1;
            continue;
        }
        if arg == "--python" {
            python = true;
            index += 1;
            continue;
        }
        if arg == "--cc" {
            index += 1;
            if index >= args.len() {
                return Cli::Usage(format!(
                    "{}\n{argv0}: error: argument --cc: expected one argument\n",
                    usage_line(argv0)
                ));
            }
            cc = Some(args[index].clone());
            index += 1;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--cc=") {
            // argparse accepts `--cc=` as an empty compiler string.
            // gen-as-const.py lines 74-75 and 109. `%s` of that string
            // is empty, so the probe command starts with ` -S`.
            cc = Some(value.to_string());
            index += 1;
            continue;
        }
        if arg.starts_with('-') && arg != "-" {
            return Cli::Usage(format!(
                "{}\n{argv0}: error: unrecognized arguments: {arg}\n",
                usage_line(argv0)
            ));
        }
        if sym_file.is_some() {
            return Cli::Usage(format!(
                "{}\n{argv0}: error: unrecognized arguments: {arg}\n",
                usage_line(argv0)
            ));
        }
        sym_file = Some(arg.clone());
        index += 1;
    }
    let Some(sym_file) = sym_file else {
        return Cli::Usage(format!(
            "{}\n{argv0}: error: the following arguments are required: sym_file\n",
            usage_line(argv0)
        ));
    };
    let mode = if test {
        Mode::Test
    } else if python {
        Mode::Python
    } else {
        Mode::Header
    };
    Cli::Work { mode, cc, sym_file }
}

fn usage_line(argv0: &str) -> String {
    format!("usage: {argv0} [-h] [--cc CC] [--test] [--python] sym_file")
}

fn help_text(argv0: &str) -> String {
    format!(
        "\
{usage}

Produce headers of assembly constants.

positional arguments:
  sym_file    .sym file to process

options:
  -h, --help  show this help message and exit
  --cc CC     C compiler (including options) to use
  --test      Generate test case instead of header
  --python    Generate Python file instead of header
",
        usage = usage_line(argv0)
    )
}

/// `re.search` tries the next start when an earlier `@@@name@@@` does not
/// finish the pattern (`glibcextract.py` lines 67-69). A name stops at `@`.
fn assembly_match(line: &str) -> Option<(String, String)> {
    let name_mark = "@@@name@@@";
    let mut start = 0;
    while start < line.len() {
        let Some(rel) = line[start..].find(name_mark) else {
            return None;
        };
        let at = start + rel;
        if let Some(parsed) = match_after_name_mark(&line[at + name_mark.len()..]) {
            return Some(parsed);
        }
        start = at + 1;
    }
    None
}

fn match_after_name_mark(after_name: &str) -> Option<(String, String)> {
    let value_mark = "@@@value@@@";
    let end_mark = "@@@end@@@";
    let name_len = after_name.find('@').unwrap_or(after_name.len());
    let name = &after_name[..name_len];
    let after_name_token = &after_name[name_len..];
    if !after_name_token.starts_with(value_mark) {
        return None;
    }
    let after_value_mark = &after_name_token[value_mark.len()..];
    let skip = after_value_mark
        .chars()
        .take_while(|c| !is_value_char(*c))
        .count();
    let value_src = &after_value_mark[byte_index_for_chars(after_value_mark, skip)..];
    let value_chars = value_src.chars().take_while(|c| is_value_char(*c)).count();
    if value_chars == 0 {
        return None;
    }
    let value_bytes = byte_index_for_chars(value_src, value_chars);
    let value = &value_src[..value_bytes];
    let after_value = &value_src[value_bytes..];
    if !after_value.contains(end_mark) {
        return None;
    }
    Some((name.to_string(), value.to_string()))
}

fn is_value_char(c: char) -> bool {
    matches!(c, '0'..='9' | 'A'..='F' | 'a'..='f' | 'X' | 'x' | '-')
}

fn byte_index_for_chars(text: &str, count: usize) -> usize {
    text.chars().take(count).map(char::len_utf8).sum()
}

/// Python interpolates the path with `%s` and does not quote it.
/// Store paths and temp paths used here have no spaces. A path with a
/// space would break the same way the Python command breaks.
fn shell_path(path: &PathBuf) -> String {
    path.display().to_string()
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
