use std::ffi::OsString;
use std::path::PathBuf;

use crate::scan::Gate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Staged,
    Tree,
    Paths(Vec<PathBuf>),
}

impl Mode {
    pub fn gate(&self) -> Gate {
        match self {
            Mode::Paths(_) => Gate::Paths,
            Mode::Staged | Mode::Tree => Gate::Product,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Help,
    Run(Mode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Parse argv including argv0. Unknown flags are errors. Default mode is staged.
pub fn parse_args<I, S>(args: I) -> Result<Action, ParseError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut iter = args.into_iter().map(Into::into);
    let _argv0 = iter.next();
    let mut mode: Option<Mode> = None;
    let rest: Vec<OsString> = iter.collect();
    let mut i = 0;
    while i < rest.len() {
        let a = rest[i].to_string_lossy();
        match a.as_ref() {
            "-h" | "--help" => return Ok(Action::Help),
            "--staged" => {
                mode = Some(Mode::Staged);
                i += 1;
            }
            "--tree" => {
                mode = Some(Mode::Tree);
                i += 1;
            }
            "--paths" => {
                i += 1;
                let mut paths = Vec::new();
                while i < rest.len() {
                    let p = rest[i].to_string_lossy();
                    if p.starts_with("--") {
                        break;
                    }
                    paths.push(PathBuf::from(rest[i].clone()));
                    i += 1;
                }
                if paths.is_empty() {
                    return Err(ParseError {
                        message: "--paths requires at least one path".to_string(),
                    });
                }
                mode = Some(Mode::Paths(paths));
            }
            other => {
                return Err(ParseError {
                    message: format!("unknown argument: {other}"),
                });
            }
        }
    }
    Ok(Action::Run(mode.unwrap_or(Mode::Staged)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Action, ParseError> {
        let mut v = vec!["surmount-private-data"];
        v.extend(args);
        parse_args(v)
    }

    #[test]
    fn default_is_staged() {
        assert_eq!(parse(&[]).unwrap(), Action::Run(Mode::Staged));
    }

    #[test]
    fn help_flag() {
        assert_eq!(parse(&["--help"]).unwrap(), Action::Help);
    }

    #[test]
    fn unknown_argument() {
        assert!(parse(&["--nope"]).is_err());
    }

    #[test]
    fn paths_requires_an_argument() {
        assert!(parse(&["--paths"]).is_err());
    }

    #[test]
    fn paths_collects_until_flag() {
        match parse(&["--paths", "a.txt", "b.txt"]).unwrap() {
            Action::Run(Mode::Paths(p)) => {
                assert_eq!(p.len(), 2);
            }
            other => panic!("{other:?}"),
        }
    }
}
