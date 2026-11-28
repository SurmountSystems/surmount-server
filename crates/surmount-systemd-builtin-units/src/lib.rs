//! systemd 262.2 `src/core/generate-builtin-units.py`.
//!
//! The behavior follows that file on the public tag v262.2
//! (33 lines). Accessed: 2026-11-26.
//! <https://raw.githubusercontent.com/systemd/systemd/v262.2/src/core/generate-builtin-units.py>
//!
//! Strip and escape, lines 17-27, then the argv loop, lines 30-33:
//!
//! ```text
//! def strip_comments(text: str) -> str:
//!     lines = (line for line in text.splitlines()
//!              if line and line[0] not in '#;')
//!     return '\n'.join(lines) + '\n'
//!
//! def c_escape(text: str) -> str:
//!     assert set(text) <= PRINTABLE
//!     return text.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n')
//!
//! for path in sys.argv[1:]:
//!     path = pathlib.Path(path)
//!     data = strip_comments(path.read_text())
//!     print(f'{{ "{path.name}", "{c_escape(data)}" }},')
//! ```
//!
//! `PRINTABLE` is `string.printable` without tab, CR, VT, or FF.
//! That set is newline plus ASCII space through `~`.
//! `path.name` is the final component and is not escaped.
//! `str.splitlines` drops the break characters, including VT, FF,
//! U+0085, U+2028, and U+2029. An empty argument list prints nothing.

use std::fs;
use std::path::{Path, PathBuf};

/// One invocation. `stdout` is what the Python script prints.
/// `code` is 0 on success and 1 when the script would raise.
pub struct RunOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

struct Fail {
    partial: String,
    message: String,
}

/// Read each path, drop empty lines and lines whose first character is
/// `#` or `;`, C-escape the body, and print one row per file.
pub fn run(paths: &[PathBuf]) -> RunOutput {
    match embed(paths) {
        Ok(stdout) => RunOutput {
            stdout,
            stderr: String::new(),
            code: 0,
        },
        Err(err) => RunOutput {
            stdout: err.partial,
            stderr: format!("{}\n", err.message),
            code: 1,
        },
    }
}

fn embed(paths: &[PathBuf]) -> Result<String, Fail> {
    let mut stdout = String::new();
    for path in paths {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) => {
                return Err(Fail {
                    partial: stdout,
                    message: format!(
                        "surmount-systemd-builtin-units: cannot read {}: {err}",
                        path.display()
                    ),
                });
            }
        };
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => {
                return Err(Fail {
                    partial: stdout,
                    message: format!(
                        "surmount-systemd-builtin-units: {} is not utf-8 text",
                        path.display()
                    ),
                });
            }
        };
        let stripped = strip_comments(&text);
        let escaped = match c_escape(&stripped) {
            Ok(escaped) => escaped,
            Err(()) => {
                return Err(Fail {
                    partial: stdout,
                    message: format!(
                        "surmount-systemd-builtin-units: {} contains a character the generator rejects",
                        path.display()
                    ),
                });
            }
        };
        let name = match unit_name(path) {
            Ok(name) => name,
            Err(message) => {
                return Err(Fail {
                    partial: stdout,
                    message,
                });
            }
        };
        stdout.push_str(&format!("{{ \"{name}\", \"{escaped}\" }},\n"));
    }
    Ok(stdout)
}

fn unit_name(path: &Path) -> Result<String, String> {
    let Some(name) = path.file_name() else {
        return Err(format!(
            "surmount-systemd-builtin-units: {} has no basename",
            path.display()
        ));
    };
    let Some(name) = name.to_str() else {
        return Err(format!(
            "surmount-systemd-builtin-units: {} basename is not utf-8",
            path.display()
        ));
    };
    Ok(name.to_string())
}

/// `str.splitlines()` with `keepends` false, including the extra
/// separators Python documents: VT, FF, file/group/record separators,
/// next line, line separator, and paragraph separator.
fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if !is_line_break(ch) {
            continue;
        }
        lines.push(&text[start..idx]);
        let next_start = if ch == '\r' {
            if let Some(&(next_idx, '\n')) = chars.peek() {
                let _ = chars.next();
                next_idx + '\n'.len_utf8()
            } else {
                idx + ch.len_utf8()
            }
        } else {
            idx + ch.len_utf8()
        };
        start = next_start;
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

fn is_line_break(ch: char) -> bool {
    matches!(
        ch,
        '\n' | '\r'
            | '\u{000b}'
            | '\u{000c}'
            | '\u{001c}'
            | '\u{001d}'
            | '\u{001e}'
            | '\u{0085}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

fn strip_comments(text: &str) -> String {
    let kept: Vec<&str> = split_lines(text)
        .into_iter()
        .filter(|line| {
            let Some(first) = line.chars().next() else {
                return false;
            };
            first != '#' && first != ';'
        })
        .collect();
    let mut out = kept.join("\n");
    out.push('\n');
    out
}

/// `string.printable` without tab, CR, VT, or FF is newline plus
/// ASCII space through `~`. Replacements are not scanned again.
fn c_escape(text: &str) -> Result<String, ()> {
    if text.chars().any(|ch| !char_allowed(ch)) {
        return Err(());
    }
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    Ok(out)
}

fn char_allowed(ch: char) -> bool {
    ch == '\n' || (' '..='~').contains(&ch)
}
