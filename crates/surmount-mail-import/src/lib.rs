//! Operator Maildir++ import for Stalwart 0.16+ via Vandelay.
//!
//! Never calls `stalwart-cli import`. Never logs token values.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod error {
    use std::fmt;
    use std::io;

    #[derive(Debug)]
    pub struct ToolError {
        pub message: String,
        pub exit_code: i32,
    }

    impl ToolError {
        pub fn fail(message: impl Into<String>) -> Self {
            Self {
                message: message.into(),
                exit_code: 1,
            }
        }
        pub fn blocked(message: impl Into<String>) -> Self {
            Self {
                message: message.into(),
                exit_code: 2,
            }
        }
    }

    impl fmt::Display for ToolError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.message)
        }
    }
    impl std::error::Error for ToolError {}
    impl From<io::Error> for ToolError {
        fn from(e: io::Error) -> Self {
            Self::fail(e.to_string())
        }
    }
}

pub use error::ToolError;
pub type Result<T> = std::result::Result<T, ToolError>;

const USAGE: &str = "\
Usage:
  surmount-mail-import-maildir [options] <account@domain> <path-to-Maildir>

Import a nested Maildir++ tree (MailPlus) into an existing Stalwart account
via Vandelay. stalwart-cli is used only to resolve the Account id.

Options:
  --dry-run           Forward --dry-run to vandelay (no writes)
  --import-only       Fill the local archive only (no JMAP export)
  --export-only       Push an existing archive (skip Maildir read)
  --account-id ID     Skip directory lookup
  --archive PATH      Vandelay SQLite archive path
  -h, --help          Show this help

Never logs token values. Never calls `stalwart-cli import`.
";

fn which(bin: &str) -> Option<PathBuf> {
    if Path::new(bin).is_file() {
        return Some(PathBuf::from(bin));
    }
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(':') {
            let p = Path::new(dir).join(bin);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn load_token() -> Option<String> {
    if let Ok(t) = env::var("STALWART_TOKEN")
        && !t.is_empty()
    {
        return Some(t);
    }
    let token_file = env::var("STALWART_TOKEN_FILE")
        .unwrap_or_else(|_| "/var/lib/surmount/secrets/ui/stalwart-api-token".into());
    fs::read_to_string(token_file).ok().map(|s| {
        s.trim_end_matches(['\r', '\n']).to_string()
    })
}

fn extract_account_id(json: &str) -> Option<String> {
    if let Some(i) = json.find("\"id\"") {
        let rest = &json[i + 4..];
        if let Some(q1) = rest.find('"') {
            let rest = &rest[q1 + 1..];
            if let Some(q2) = rest.find('"') {
                let id = &rest[..q2];
                if !id.is_empty() {
                    return Some(id.to_string());
                }
            }
        }
    }
    None
}

pub fn run<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let argv: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let mut dry_run = false;
    let mut import_only = false;
    let mut export_only = false;
    let mut account_id = env::var("STALWART_ACCOUNT_ID").unwrap_or_default();
    let mut archive = env::var("SURMOUNT_MAIL_IMPORT_ARCHIVE").unwrap_or_default();
    let mut positional = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "--dry-run" => dry_run = true,
            "--import-only" => import_only = true,
            "--export-only" => export_only = true,
            "--account-id" => {
                i += 1;
                account_id = argv
                    .get(i)
                    .cloned()
                    .ok_or_else(|| ToolError::blocked("--account-id needs a value".to_string()))?;
            }
            "--archive" => {
                i += 1;
                archive = argv
                    .get(i)
                    .cloned()
                    .ok_or_else(|| ToolError::blocked("--archive needs a value".to_string()))?;
            }
            "--" => {
                positional.extend(argv[i + 1..].iter().cloned());
                break;
            }
            s if s.starts_with('-') => {
                eprint!("{USAGE}");
                return Err(ToolError::blocked(format!("unknown option: {s}")));
            }
            _ => positional.push(argv[i].clone()),
        }
        i += 1;
    }
    if positional.len() < 2 {
        eprint!("{USAGE}");
        return Err(ToolError::blocked("missing account and Maildir path".to_string()));
    }
    if import_only && export_only {
        return Err(ToolError::blocked(
            "--import-only and --export-only are mutually exclusive".to_string(),
        ));
    }
    let account = positional[0].clone();
    let maildir = positional[1].clone();
    let url = env::var("STALWART_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let exclude = env::var("SURMOUNT_MAIL_IMPORT_EXCLUDE").unwrap_or_else(|_| "All Mail".into());

    if !export_only && !Path::new(&maildir).is_dir() {
        return Err(ToolError::fail(format!("Maildir path not found: {maildir}")));
    }

    let vandelay = which("vandelay").ok_or_else(|| {
        ToolError::blocked(
            "vandelay not on PATH. Stalwart 0.16 / stalwart-cli 1.0.x has no import subcommand. Install the Surmount vandelay package (nix/packages/vandelay.nix) via deploy. Do not curl|sh an installer. Docs: docs/MIGRATION.md".to_string(),
        )
    })?;

    let token = if import_only {
        None
    } else {
        let t = load_token().filter(|s| !s.is_empty());
        if t.is_none() {
            return Err(ToolError::blocked(
                "missing Stalwart API token (STALWART_TOKEN or STALWART_TOKEN_FILE)".to_string(),
            ));
        }
        t
    };

    let safe_account: String = account
        .chars()
        .map(|c| match c {
            '@' | '/' => '_',
            c if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' => c,
            _ => '_',
        })
        .collect();
    if archive.is_empty() {
        archive = format!("/var/lib/surmount/import/vandelay/{safe_account}.sqlite");
    }

    println!("Importing Maildir++ for {account} from {maildir}");
    println!("JMAP target: {url}");
    println!("Vandelay archive: {archive}");
    println!("Exclude regex: {exclude}");

    if !export_only {
        if let Some(parent) = Path::new(&archive).parent() {
            fs::create_dir_all(parent)?;
        }
        println!("vandelay: import maildir -> archive");
        let mut cmd = Command::new(&vandelay);
        cmd.args(["import", "maildir", &maildir, &archive, "--exclude", &exclude]);
        if dry_run {
            cmd.arg("--dry-run");
        }
        let st = cmd.status()?;
        if !st.success() {
            return Err(ToolError::fail(format!(
                "vandelay import failed (rc={})",
                st.code().unwrap_or(1)
            )));
        }
    }

    if !import_only {
        let token = token.unwrap();
        if account_id.is_empty() {
            let cli = which("stalwart-cli").ok_or_else(|| {
                ToolError::blocked(
                    "stalwart-cli not on PATH (needed to resolve Account id)".to_string(),
                )
            })?;
            let where_c = if account.contains('@') {
                format!("emailAddress={account}")
            } else {
                format!("name={account}")
            };
            let out = Command::new(cli)
                .args([
                    "--url",
                    &url,
                    "query",
                    "Account",
                    "--where",
                    &where_c,
                    "--fields",
                    "id,name,emailAddress",
                    "--json",
                ])
                .env("STALWART_URL", &url)
                .env("STALWART_TOKEN", &token)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()?;
            let json = String::from_utf8_lossy(&out.stdout);
            account_id = extract_account_id(&json).ok_or_else(|| {
                ToolError::fail(format!(
                    "could not resolve Stalwart Account id for {account}"
                ))
            })?;
        }
        println!("vandelay: export archive -> JMAP account id {account_id}");
        let mut cmd = Command::new(&vandelay);
        cmd.args([
            "export",
            "--url",
            &url,
            "--auth-bearer",
            "--account-id",
            &account_id,
            "--objects",
            "mailbox,email",
        ]);
        if dry_run {
            cmd.arg("--dry-run");
        }
        cmd.arg(&archive);
        cmd.env("VANDELAY_TOKEN", &token);
        let st = cmd.status()?;
        if !st.success() {
            return Err(ToolError::fail(format!(
                "vandelay export failed (rc={})",
                st.code().unwrap_or(1)
            )));
        }
    }
    println!("done (operator-started; re-run is convergent in Vandelay).");
    Ok(())
}
