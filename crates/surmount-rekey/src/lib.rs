//! SHC zero-knowledge backup re-key helper.
//!
//! Default rung is PGP: mint an X25519 age identity with the `age` crate
//! (rage's library), wrap it with the operator's gpg, print only the public
//! `age1...` recipient for the SHC Backups panel (kind pgp).
//! Never prints AGE-SECRET-KEY-1 on stdout.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use age::x25519::Identity;
use secrecy::ExposeSecret;

#[derive(Debug)]
pub struct RekeyError {
    pub message: String,
}

impl std::fmt::Display for RekeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RekeyError {}

impl RekeyError {
    fn fail(m: impl Into<String>) -> Self {
        Self { message: m.into() }
    }
}

#[derive(Debug, Clone)]
pub struct RekeyRequest {
    pub dir: PathBuf,
    pub gpg_recipient: String,
    pub gpg_bin: String,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct RekeyOutput {
    pub recipient: String,
    pub identity_path: PathBuf,
    pub envelope_path: PathBuf,
    pub card: String,
}

pub fn default_dir() -> PathBuf {
    let data = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        format!("{home}/.local/share")
    });
    PathBuf::from(data).join("surmount/shc-backup")
}

pub fn default_gpg_recipient() -> String {
    std::env::var("SURMOUNT_REKEY_GPG_RECIPIENT")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "hunter@surmount.systems".into())
}

/// Mint identity, write 0600 identity file, wrap with gpg unless dry_run.
pub fn run_rekey(req: &RekeyRequest) -> Result<RekeyOutput, RekeyError> {
    fs::create_dir_all(&req.dir).map_err(|e| RekeyError::fail(format!("mkdir: {e}")))?;
    let identity = Identity::generate();
    let recipient = identity.to_public().to_string();
    if !recipient.starts_with("age1") {
        return Err(RekeyError::fail("age identity produced no age1 recipient"));
    }
    let identity_path = req.dir.join("age-identity.txt");
    let envelope_path = req.dir.join("envelope.asc");
    write_secret_file(&identity_path, identity.to_string().expose_secret())?;
    if req.dry_run {
        fs::write(
            &envelope_path,
            b"-----BEGIN PGP MESSAGE-----\ndry-run\n-----END PGP MESSAGE-----\n",
        )
        .map_err(|e| RekeyError::fail(format!("envelope: {e}")))?;
    } else {
        gpg_wrap(
            &req.gpg_bin,
            &req.gpg_recipient,
            identity.to_string().expose_secret().as_bytes(),
            &envelope_path,
        )?;
    }
    let card = format_card(
        &recipient,
        &identity_path,
        &envelope_path,
        &req.gpg_recipient,
    );
    if card.contains("AGE-SECRET-KEY-1") {
        return Err(RekeyError::fail("refusing to print age identity on stdout"));
    }
    Ok(RekeyOutput {
        recipient,
        identity_path,
        envelope_path,
        card,
    })
}

fn write_secret_file(path: &Path, body: &str) -> Result<(), RekeyError> {
    let mut f = fs::File::create(path).map_err(|e| RekeyError::fail(format!("identity: {e}")))?;
    f.write_all(body.as_bytes())
        .map_err(|e| RekeyError::fail(format!("identity write: {e}")))?;
    f.write_all(b"\n")
        .map_err(|e| RekeyError::fail(format!("identity write: {e}")))?;
    drop(f);
    let mut perm = fs::metadata(path)
        .map_err(|e| RekeyError::fail(format!("identity stat: {e}")))?
        .permissions();
    perm.set_mode(0o600);
    fs::set_permissions(path, perm).map_err(|e| RekeyError::fail(format!("chmod 600: {e}")))?;
    Ok(())
}

fn gpg_wrap(gpg_bin: &str, recipient: &str, identity: &[u8], out: &Path) -> Result<(), RekeyError> {
    if recipient.starts_with('-') {
        return Err(RekeyError::fail("gpg recipient must not start with '-'"));
    }
    let mut child = Command::new(gpg_bin)
        .args([
            "--batch",
            "--yes",
            "--trust-model",
            "always",
            "--encrypt",
            "--armor",
            "-r",
            recipient,
            "-o",
        ])
        .arg(out)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RekeyError::fail(format!("gpg failed to start: {e}")))?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| RekeyError::fail("gpg stdin missing"))?;
        stdin
            .write_all(identity)
            .map_err(|e| RekeyError::fail(format!("gpg stdin: {e}")))?;
        stdin
            .write_all(b"\n")
            .map_err(|e| RekeyError::fail(format!("gpg stdin: {e}")))?;
    }
    let outp = child
        .wait_with_output()
        .map_err(|e| RekeyError::fail(format!("gpg wait: {e}")))?;
    if !outp.status.success() {
        let err = String::from_utf8_lossy(&outp.stderr);
        let safe: String = err
            .lines()
            .filter(|l| !l.contains("AGE-SECRET-KEY-1"))
            .collect::<Vec<_>>()
            .join(" ");
        return Err(RekeyError::fail(format!(
            "gpg --encrypt --armor failed (exit {}). recipient={recipient}. {safe}",
            outp.status.code().unwrap_or(1)
        )));
    }
    if !out.is_file() {
        return Err(RekeyError::fail("gpg produced no envelope.asc"));
    }
    Ok(())
}

pub fn format_card(
    recipient: &str,
    identity_path: &Path,
    envelope_path: &Path,
    gpg_recipient: &str,
) -> String {
    format!(
        "SHC Backups re-key (PGP)\n\
         \n\
         1. SSH Keys tab: add this laptop's ssh-ed25519 public key if missing:\n\
            cat ~/.ssh/id_ed25519.pub\n\
            Tick THAT key as the primary. Untick any other SSH recipient.\n\
         \n\
         2. Add a recovery recipient. Type: PGP\n\
            Paste this age1 line only:\n\
            {recipient}\n\
         \n\
         3. Confirm with DESTROY-MY-BACKUPS (SHC's exact phrase).\n\
         \n\
         GPG wrap recipient: {gpg_recipient}\n\
         Envelope (keep with GPG backups, not git): {envelope}\n\
         Identity file (0600, never stdout): {identity}\n\
         \n\
         After the panel updates, check the SSH blob with:\n\
            awk '{{print $2}}' ~/.ssh/id_ed25519.pub\n\
         The PGP row must match the age1 line above.\n\
         Do not create a backup until both match.\n",
        recipient = recipient,
        gpg_recipient = gpg_recipient,
        envelope = envelope_path.display(),
        identity = identity_path.display(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp_dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "surmount-rekey-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn fake_gpg(dir: &Path) -> PathBuf {
        let bin = dir.join("fake-gpg");
        fs::write(
            &bin,
            "#!/bin/sh\n\
             out=\"\"\n\
             while [ \"$#\" -gt 0 ]; do\n\
               if [ \"$1\" = \"-o\" ]; then out=\"$2\"; shift 2; continue; fi\n\
               shift\n\
             done\n\
             mkdir -p \"$(dirname \"$out\")\"\n\
             printf '%s\\n' '-----BEGIN PGP MESSAGE-----' >\"$out\"\n\
             cat >>\"$out\"\n\
             printf '%s\\n' '-----END PGP MESSAGE-----' >>\"$out\"\n\
             exit 0\n",
        )
        .unwrap();
        let mut p = fs::metadata(&bin).unwrap().permissions();
        p.set_mode(0o755);
        fs::set_permissions(&bin, p).unwrap();
        bin
    }

    #[test]
    fn card_has_pgp_and_age1_not_secret() {
        let rec = "age1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq2xq5s0";
        let card = format_card(
            rec,
            Path::new("/tmp/id"),
            Path::new("/tmp/env.asc"),
            "me@example.test",
        );
        assert!(card.contains("Type: PGP"));
        assert!(card.contains(rec));
        assert!(card.contains("DESTROY-MY-BACKUPS"));
        assert!(!card.contains("AGE-SECRET-KEY-1"));
        assert!(card.contains("cat ~/.ssh/id_ed25519.pub"));
    }

    #[test]
    fn dry_run_writes_identity_0600_and_card() {
        let dir = temp_dir();
        let out = run_rekey(&RekeyRequest {
            dir: dir.clone(),
            gpg_recipient: "me@example.test".into(),
            gpg_bin: "false".into(),
            dry_run: true,
        })
        .unwrap();
        assert!(out.recipient.starts_with("age1"));
        assert!(!out.card.contains("AGE-SECRET-KEY-1"));
        let meta = fs::metadata(&out.identity_path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        let body = fs::read_to_string(&out.identity_path).unwrap();
        assert!(body.contains("AGE-SECRET-KEY-1"));
    }

    #[test]
    fn gpg_wrap_uses_mock_and_keeps_secret_off_stdout() {
        let dir = temp_dir();
        let gpg = fake_gpg(&dir);
        let out = run_rekey(&RekeyRequest {
            dir: dir.clone(),
            gpg_recipient: "me@example.test".into(),
            gpg_bin: gpg.to_string_lossy().into_owned(),
            dry_run: false,
        })
        .unwrap();
        assert!(out.recipient.starts_with("age1"));
        assert!(!out.card.contains("AGE-SECRET-KEY-1"));
        let env = fs::read_to_string(&out.envelope_path).unwrap();
        assert!(env.contains("BEGIN PGP MESSAGE"));
        assert!(env.contains("AGE-SECRET-KEY-1"));
    }

    #[test]
    fn option_shaped_gpg_recipient_rejected() {
        let dir = temp_dir();
        let gpg = fake_gpg(&dir);
        let err = run_rekey(&RekeyRequest {
            dir,
            gpg_recipient: "-evil".into(),
            gpg_bin: gpg.to_string_lossy().into_owned(),
            dry_run: false,
        })
        .unwrap_err();
        assert!(err.message.contains("must not start with '-'"));
    }
}
