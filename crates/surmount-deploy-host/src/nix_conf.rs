//! Pure nix.conf transform for the mail-host gccarch bootstrap.
//!
//! The running daemon does not see `nix.settings.extra-system-features`
//! until a switch activates a new nix.conf. The system build needs
//! `gccarch-x86-64-v3` on the daemon that is already running. This module
//! decides the conf text. The remote driver in `gccarch_bootstrap_remote`
//! applies it on the target before `nixos-rebuild`.

/// Feature the v3 stdenv requires. Nix 2.34 folds `extra-system-features`
/// into `system-features` on `nix config show`.
pub(crate) const GCCARCH_X86_64_V3: &str = "gccarch-x86-64-v3";

/// Remote-builder feature. The bootstrap must not drop it.
pub(crate) const SURMOUNT_REMOTE: &str = "surmount-remote";

/// Copy target. Never write the store nix.conf in place. A bind of this
/// path does not survive reboot.
pub(crate) const RUN_NIX_CONF: &str = "/run/surmount-nix.conf";

/// What to do with the daemon conf before the system build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GccarchConfDecision {
    /// `system-features` already lists [`GCCARCH_X86_64_V3`].
    /// Do not rewrite the conf. Do not restart the daemon.
    LeaveUnchanged,
    /// Write this text to [`RUN_NIX_CONF`], bind-mount it over the store
    /// nix.conf, and restart nix-daemon.
    Rewrite(String),
}

impl GccarchConfDecision {
    pub(crate) fn rewrites_conf(&self) -> bool {
        matches!(self, Self::Rewrite(_))
    }

    /// Restart only follows a rewrite. [`Self::LeaveUnchanged`] does not.
    pub(crate) fn restarts_daemon(&self) -> bool {
        matches!(self, Self::Rewrite(_))
    }
}

/// Decide the conf text from the file the daemon is using.
///
/// `system-features` is last-assignment-wins, same as Nix. A commented
/// line does not count. `extra-system-features` is not that key.
/// When the last `system-features` line lacks the token, every
/// `extra-system-features` line gains `surmount-remote` and
/// `gccarch-x86-64-v3` without dropping tokens already on the line.
/// The `system-features` line text is copied unchanged.
pub(crate) fn bootstrap_gccarch_conf(conf: &str) -> GccarchConfDecision {
    let lines = conf_lines(conf);
    if last_system_features_value(&lines).is_some_and(|value| value_has_arch(&value)) {
        return GccarchConfDecision::LeaveUnchanged;
    }
    let mut out = String::new();
    let mut saw_extra = false;
    for line in &lines {
        if assignment_key(line) == Some("extra-system-features") {
            saw_extra = true;
            out.push_str(&rewrite_extra_line(line));
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !saw_extra {
        out.push_str("extra-system-features = ");
        out.push_str(SURMOUNT_REMOTE);
        out.push(' ');
        out.push_str(GCCARCH_X86_64_V3);
        out.push('\n');
    }
    GccarchConfDecision::Rewrite(out)
}

fn conf_lines(conf: &str) -> Vec<String> {
    if conf.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<String> = conf.split('\n').map(str::to_string).collect();
    if conf.ends_with('\n') {
        lines.pop();
    }
    lines
}

fn trim_ascii_ws(s: &str) -> &str {
    s.trim_matches([' ', '\t'])
}

fn assignment_key(line: &str) -> Option<&str> {
    let rest = trim_ascii_ws(line);
    if rest.is_empty() || rest.starts_with('#') {
        return None;
    }
    let eq = rest.find('=')?;
    let key = trim_ascii_ws(&rest[..eq]);
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(key)
}

fn last_system_features_value(lines: &[String]) -> Option<String> {
    let mut last = None;
    for line in lines {
        if assignment_key(line) == Some("system-features")
            && let Some(eq) = line.find('=')
        {
            last = Some(line[eq + 1..].to_string());
        }
    }
    last
}

fn ascii_ws_tokens(value: &str) -> Vec<String> {
    let mut trimmed = trim_ascii_ws(value);
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        trimmed = &trimmed[1..trimmed.len() - 1];
    }
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed
        .split(|c| c == ' ' || c == '\t' || c == '\n')
        .filter(|tok| !tok.is_empty())
        .map(str::to_string)
        .collect()
}

fn value_has_arch(value: &str) -> bool {
    ascii_ws_tokens(value)
        .iter()
        .any(|tok| tok == GCCARCH_X86_64_V3)
}

fn ensure_extra_tokens(value: &str) -> String {
    let mut toks = ascii_ws_tokens(value);
    if !toks.iter().any(|tok| tok == SURMOUNT_REMOTE) {
        toks.push(SURMOUNT_REMOTE.to_string());
    }
    if !toks.iter().any(|tok| tok == GCCARCH_X86_64_V3) {
        toks.push(GCCARCH_X86_64_V3.to_string());
    }
    toks.join(" ")
}

fn rewrite_extra_line(line: &str) -> String {
    let eq = line
        .find('=')
        .expect("extra-system-features assignment has '='");
    let prefix = &line[..eq];
    let tokens = ensure_extra_tokens(&line[eq + 1..]);
    format!("{prefix}= {tokens}")
}

const SHELL_FUNCTIONS: &str = r#"
deploy_host_trim() {
  val=$1
  tab=$(printf '\t')
  while :; do
    case "$val" in
      ' '*) val=${val# } ;;
      "$tab"*) val=${val#"$tab"} ;;
      *) break ;;
    esac
  done
  while :; do
    case "$val" in
      *' ') val=${val% } ;;
      *"$tab") val=${val%"$tab"} ;;
      *) break ;;
    esac
  done
  printf '%s' "$val"
}

deploy_host_assignment_key() {
  rest=$(deploy_host_trim "$1")
  case "$rest" in
    ''|\#*) printf '\n'; return ;;
  esac
  case "$rest" in
    *=*) ;;
    *) printf '\n'; return ;;
  esac
  key=$(deploy_host_trim "${rest%%=*}")
  case "$key" in
    *[!A-Za-z0-9_-]*|'') printf '\n'; return ;;
  esac
  printf '%s\n' "$key"
}

deploy_host_list_has_arch() {
  oldifs=$IFS
  IFS=$(printf ' \t\nX')
  IFS=${IFS%X}
  set -f
  found=0
  for tok in $1; do
    if [ "$tok" = "$deploy_host_gccarch_token" ]; then
      found=1
      break
    fi
  done
  set +f
  IFS=$oldifs
  [ "$found" -eq 1 ]
}

deploy_host_value_has_arch() {
  val=$(deploy_host_trim "$1")
  case "$val" in
    \"*\")
      val=${val#\"}
      val=${val%\"}
      ;;
  esac
  deploy_host_list_has_arch "$val"
}

deploy_host_blob_has_token() {
  blob=$(printf '%s' "$1" | sed 's/[][",]/ /g')
  deploy_host_list_has_arch "$blob"
}

deploy_host_file_system_features_has_token() {
  lastval=
  found=0
  while IFS= read -r line || [ -n "$line" ]; do
    k=$(deploy_host_assignment_key "$line")
    if [ "$k" = system-features ]; then
      found=1
      lastval=${line#*=}
    fi
  done < "$1"
  if [ "$found" -eq 1 ]; then
    deploy_host_value_has_arch "$lastval"
  else
    return 1
  fi
}

deploy_host_ensure_extra_tokens() {
  val=$(deploy_host_trim "$1")
  case "$val" in
    \"*\")
      val=${val#\"}
      val=${val%\"}
      ;;
  esac
  has_remote=0
  has_arch=0
  out=
  oldifs=$IFS
  IFS=$(printf ' \t\nX')
  IFS=${IFS%X}
  set -f
  for tok in $val; do
    if [ -z "$tok" ]; then
      continue
    fi
    if [ "$tok" = surmount-remote ]; then
      has_remote=1
    fi
    if [ "$tok" = "$deploy_host_gccarch_token" ]; then
      has_arch=1
    fi
    if [ -z "$out" ]; then
      out=$tok
    else
      out="$out $tok"
    fi
  done
  set +f
  IFS=$oldifs
  if [ "$has_remote" -eq 0 ]; then
    if [ -z "$out" ]; then
      out=surmount-remote
    else
      out="$out surmount-remote"
    fi
  fi
  if [ "$has_arch" -eq 0 ]; then
    if [ -z "$out" ]; then
      out=$deploy_host_gccarch_token
    else
      out="$out $deploy_host_gccarch_token"
    fi
  fi
  printf '%s' "$out"
}

deploy_host_rewrite_extra_line() {
  line=$1
  prefix=${line%%=*}
  val=${line#*=}
  tokens=$(deploy_host_ensure_extra_tokens "$val")
  printf '%s= %s' "$prefix" "$tokens"
}

deploy_host_rewrite_conf() {
  src=$1
  dest=$2
  tmp=$(mktemp "${dest}.XXXXXX") || return 1
  found_extra=0
  while IFS= read -r line || [ -n "$line" ]; do
    k=$(deploy_host_assignment_key "$line")
    if [ "$k" = extra-system-features ]; then
      found_extra=1
      printf '%s\n' "$(deploy_host_rewrite_extra_line "$line")" >> "$tmp"
    else
      printf '%s\n' "$line" >> "$tmp"
    fi
  done < "$src"
  if [ "$found_extra" -eq 0 ]; then
    printf '%s\n' "extra-system-features = surmount-remote $deploy_host_gccarch_token" >> "$tmp"
  fi
  if ! cat "$tmp" > "$dest"; then
    rm -f "$tmp"
    return 1
  fi
  rm -f "$tmp"
  chmod 644 "$dest" || true
  return 0
}

deploy_host_bind_already() {
  if ! command -v findmnt >/dev/null 2>&1; then
    return 1
  fi
  findmnt -n -o SOURCE -T "$1" 2>/dev/null | grep -F -- "$deploy_host_run_nix_conf" >/dev/null 2>&1
}
"#;

const SHELL_DRIVER: &str = r#"
# Bootstrap gccarch before the system build.
# Read the conf the daemon is using. Do not write that store file in place.
# Write /run/surmount-nix.conf and bind-mount it over the store path.
# The /run bind does not survive reboot. A successful switch puts the
# feature in the generation nix.conf.
# If the daemon already advertises the feature, do not rewrite and do not restart.
deploy_host_daemon_features=$(nix config show system-features 2>/dev/null || true)
if deploy_host_blob_has_token "$deploy_host_daemon_features"; then
  :
else
  store_conf=$(readlink -f /etc/nix/nix.conf) || {
    echo "deploy-host: cannot resolve /etc/nix/nix.conf; gccarch-x86-64-v3 still absent; not starting the system build" >&2
    exit 1
  }
  if [ ! -f "$store_conf" ]; then
    echo "deploy-host: nix.conf the daemon reads is missing; gccarch-x86-64-v3 still absent; not starting the system build" >&2
    exit 1
  fi
  if deploy_host_file_system_features_has_token "$store_conf"; then
    echo "deploy-host: nix.conf system-features already lists gccarch-x86-64-v3 but the daemon does not advertise it; not rewriting and not restarting; not starting the system build" >&2
    exit 1
  fi
  if ! deploy_host_rewrite_conf "$store_conf" "$deploy_host_run_nix_conf"; then
    echo "deploy-host: failed to write /run/surmount-nix.conf; gccarch-x86-64-v3 still absent; not starting the system build" >&2
    exit 1
  fi
  if [ "$store_conf" != "$deploy_host_run_nix_conf" ]; then
    mount_err=$(mktemp /run/surmount-nix-mount.XXXXXX) || {
      echo "deploy-host: cannot record the bind-mount error; gccarch-x86-64-v3 still absent; not starting the system build" >&2
      exit 1
    }
    if mount --bind "$deploy_host_run_nix_conf" "$store_conf" 2>"$mount_err"; then
      rm -f "$mount_err"
    elif grep -E -q 'already mounted|already a mount point' "$mount_err"; then
      cat "$mount_err" >&2
      rm -f "$mount_err"
    elif deploy_host_bind_already "$store_conf"; then
      rm -f "$mount_err"
    else
      echo "deploy-host: bind-mount of nix.conf failed; gccarch-x86-64-v3 still absent; not starting the system build" >&2
      cat "$mount_err" >&2
      rm -f "$mount_err"
      exit 1
    fi
  fi
  if ! systemctl restart nix-daemon; then
    echo "deploy-host: nix-daemon restart failed; gccarch-x86-64-v3 still absent; not starting the system build" >&2
    exit 1
  fi
  deploy_host_daemon_features=$(nix config show system-features 2>/dev/null || true)
  if ! deploy_host_blob_has_token "$deploy_host_daemon_features"; then
    echo "deploy-host: gccarch-x86-64-v3 still absent from nix config show system-features; not starting the system build" >&2
    printf '%s\n' "deploy-host: nix config show system-features printed:" >&2
    printf '%s\n' "$deploy_host_daemon_features" >&2
    exit 1
  fi
fi
"#;

/// Shell that runs on the target as root, before the system build.
/// `nix config show extra-system-features` is not used: Nix 2.34.9 has
/// no such key. The show output for `system-features` already includes
/// extra tokens.
pub(crate) fn gccarch_bootstrap_remote() -> String {
    format!(
        "set +e\ndeploy_host_gccarch_token={GCCARCH_X86_64_V3}\ndeploy_host_run_nix_conf={RUN_NIX_CONF}\n{SHELL_FUNCTIONS}\n{SHELL_DRIVER}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SYSTEM_FEATURES_LINE: &str = "system-features = nixos-test benchmark big-parallel kvm\n";

    fn live_daemon_conf() -> &'static str {
        "extra-system-features = surmount-remote\nsystem-features = nixos-test benchmark big-parallel kvm\n"
    }

    fn system_feature_lines(conf: &str) -> Vec<String> {
        conf_lines(conf)
            .into_iter()
            .filter(|line| assignment_key(line) == Some("system-features"))
            .collect()
    }

    #[test]
    fn conf_that_lacks_gccarch_is_not_left_unchanged() {
        let input = live_daemon_conf();
        let decision = bootstrap_gccarch_conf(input);
        assert!(
            decision.rewrites_conf(),
            "a conf that lacks {GCCARCH_X86_64_V3} must not be left unchanged"
        );
        assert!(
            decision.restarts_daemon(),
            "a missing token is a rewrite, and a rewrite restarts the daemon"
        );
        match decision {
            GccarchConfDecision::LeaveUnchanged => {
                panic!("a conf that lacks {GCCARCH_X86_64_V3} must not be left unchanged")
            }
            GccarchConfDecision::Rewrite(out) => {
                assert_ne!(
                    out, input,
                    "a conf that lacks {GCCARCH_X86_64_V3} must not be left unchanged"
                );
                assert!(
                    out.contains(GCCARCH_X86_64_V3),
                    "rewritten conf dropped {GCCARCH_X86_64_V3}: {out}"
                );
            }
        }
    }

    #[test]
    fn extra_line_gains_gccarch_and_system_features_line_is_unchanged() {
        let input = live_daemon_conf();
        let GccarchConfDecision::Rewrite(out) = bootstrap_gccarch_conf(input) else {
            panic!("missing token must rewrite");
        };
        assert!(
            out.contains("extra-system-features = surmount-remote gccarch-x86-64-v3\n"),
            "{out}"
        );
        assert!(out.contains(SYSTEM_FEATURES_LINE), "{out}");
        assert_eq!(
            system_feature_lines(input),
            system_feature_lines(&out),
            "system-features line must stay byte-for-byte"
        );
        assert!(
            !out.contains("system-features = nixos-test benchmark big-parallel kvm gccarch"),
            "system-features line must not gain the token: {out}"
        );
    }

    #[test]
    fn system_features_line_with_gccarch_does_not_rewrite_or_restart() {
        let input = "\
extra-system-features = surmount-remote\n\
system-features = nixos-test benchmark big-parallel kvm gccarch-x86-64-v3\n";
        let decision = bootstrap_gccarch_conf(input);
        assert_eq!(decision, GccarchConfDecision::LeaveUnchanged);
        assert!(!decision.rewrites_conf());
        assert!(!decision.restarts_daemon());
    }

    #[test]
    fn extra_line_keeps_other_tokens_and_does_not_duplicate() {
        let input = "\
extra-system-features = surmount-remote other-token\n\
system-features = nixos-test benchmark big-parallel kvm\n";
        let GccarchConfDecision::Rewrite(out) = bootstrap_gccarch_conf(input) else {
            panic!("must rewrite");
        };
        assert!(
            out.contains("extra-system-features = surmount-remote other-token gccarch-x86-64-v3\n"),
            "{out}"
        );
        assert_eq!(system_feature_lines(input), system_feature_lines(&out));

        let already = "\
extra-system-features = gccarch-x86-64-v3 surmount-remote extra\n\
system-features = nixos-test benchmark big-parallel kvm\n";
        let GccarchConfDecision::Rewrite(out) = bootstrap_gccarch_conf(already) else {
            panic!("system-features still lacks the token, so this is still a rewrite");
        };
        assert!(
            out.contains("extra-system-features = gccarch-x86-64-v3 surmount-remote extra\n"),
            "{out}"
        );
        assert_eq!(out.matches("gccarch-x86-64-v3").count(), 1, "{out}");
    }

    #[test]
    fn substring_and_comment_do_not_count_as_the_token() {
        let substring = "\
extra-system-features = surmount-remote\n\
system-features = nixos-test gccarch-x86-64-v30 kvm\n";
        assert!(bootstrap_gccarch_conf(substring).rewrites_conf());

        let comment = "\
# system-features = nixos-test benchmark big-parallel kvm gccarch-x86-64-v3\n\
# extra-system-features = surmount-remote\n\
system-features = nixos-test benchmark big-parallel kvm\n";
        let GccarchConfDecision::Rewrite(out) = bootstrap_gccarch_conf(comment) else {
            panic!("a comment must not suppress the rewrite");
        };
        assert!(out.contains(
            "# system-features = nixos-test benchmark big-parallel kvm gccarch-x86-64-v3\n"
        ));
        assert!(out.contains("# extra-system-features = surmount-remote\n"));
        assert!(
            out.contains("extra-system-features = surmount-remote gccarch-x86-64-v3\n"),
            "{out}"
        );
        assert_eq!(system_feature_lines(comment), system_feature_lines(&out));
    }

    #[test]
    fn last_system_features_line_wins() {
        let earlier_only = "\
system-features = gccarch-x86-64-v3\n\
system-features = nixos-test benchmark big-parallel kvm\n\
extra-system-features = surmount-remote\n";
        assert!(bootstrap_gccarch_conf(earlier_only).rewrites_conf());

        let later = "\
system-features = nixos-test benchmark big-parallel kvm\n\
extra-system-features = surmount-remote\n\
system-features = nixos-test benchmark big-parallel kvm gccarch-x86-64-v3\n";
        let decision = bootstrap_gccarch_conf(later);
        assert!(!decision.rewrites_conf());
        assert!(!decision.restarts_daemon());
    }

    #[test]
    fn missing_extra_line_is_appended_without_touching_system_features() {
        let input = "max-jobs = 8\nsystem-features = nixos-test benchmark big-parallel kvm\nsandbox = true\n";
        let GccarchConfDecision::Rewrite(out) = bootstrap_gccarch_conf(input) else {
            panic!("must rewrite");
        };
        assert!(out.contains("max-jobs = 8\n"), "{out}");
        assert!(out.contains("sandbox = true\n"), "{out}");
        assert!(out.contains(SYSTEM_FEATURES_LINE), "{out}");
        assert!(
            out.contains("extra-system-features = surmount-remote gccarch-x86-64-v3\n"),
            "{out}"
        );
        assert_eq!(system_feature_lines(input), system_feature_lines(&out));
    }

    #[test]
    fn extra_system_features_key_is_not_system_features() {
        let input = "extra-system-features = gccarch-x86-64-v3\n";
        let decision = bootstrap_gccarch_conf(input);
        assert!(
            decision.rewrites_conf(),
            "the token on extra-system-features is not the system-features line"
        );
        let GccarchConfDecision::Rewrite(out) = decision else {
            panic!("must rewrite");
        };
        assert!(
            out.contains("extra-system-features = gccarch-x86-64-v3 surmount-remote\n")
                || out.contains("extra-system-features = gccarch-x86-64-v3 surmount-remote"),
            "{out}"
        );
        assert!(out.contains(SURMOUNT_REMOTE), "{out}");
    }

    fn shell_decision(conf: &str) -> Result<ShellDecision, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("surmount-gccarch-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let result = shell_decision_in(&dir, conf);
        let _ = fs::remove_dir_all(&dir);
        result
    }

    fn shell_decision_in(dir: &Path, conf: &str) -> Result<ShellDecision, String> {
        let src = dir.join("nix.conf");
        let dest = dir.join("out.conf");
        fs::write(&src, conf).map_err(|err| err.to_string())?;
        let script = format!(
            "deploy_host_gccarch_token={GCCARCH_X86_64_V3}\n\
             deploy_host_run_nix_conf={RUN_NIX_CONF}\n\
             {SHELL_FUNCTIONS}\n\
             if deploy_host_file_system_features_has_token \"$1\"; then\n\
               echo LEAVE\n\
             else\n\
               deploy_host_rewrite_conf \"$1\" \"$2\" || exit 2\n\
               echo REWRITE\n\
             fi\n"
        );
        let out = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .arg("sh")
            .arg(&src)
            .arg(&dest)
            .output()
            .map_err(|err| format!("sh failed to start: {err}"))?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        if !out.status.success() {
            return Err(format!(
                "sh exit {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                out.status.code()
            ));
        }
        if stdout.trim() == "LEAVE" {
            return Ok(ShellDecision::Leave);
        }
        if stdout.trim() == "REWRITE" {
            let body = fs::read_to_string(&dest).map_err(|err| err.to_string())?;
            return Ok(ShellDecision::Rewrite(body));
        }
        Err(format!(
            "unexpected shell stdout:\n{stdout}\nstderr:\n{stderr}"
        ))
    }

    enum ShellDecision {
        Leave,
        Rewrite(String),
    }

    #[test]
    fn shell_transform_matches_rust_decision() {
        let fixtures = [
            live_daemon_conf(),
            "",
            "extra-system-features = surmount-remote other-token\nsystem-features = nixos-test benchmark big-parallel kvm\n",
            "extra-system-features=surmount-remote\nsystem-features = nixos-test benchmark big-parallel kvm\n",
            "extra-system-features = surmount-remote\nsystem-features = nixos-test benchmark big-parallel kvm gccarch-x86-64-v3\n",
            "max-jobs = 8\nsystem-features = nixos-test benchmark big-parallel kvm\nsandbox = true\n",
            "# system-features = gccarch-x86-64-v3\nsystem-features = nixos-test benchmark big-parallel kvm\n",
            "extra-system-features = gccarch-x86-64-v3 surmount-remote extra\nsystem-features = nixos-test gccarch-x86-64-v30\n",
            "system-features = gccarch-x86-64-v3\nsystem-features = nixos-test benchmark big-parallel kvm\nextra-system-features = surmount-remote\n",
            "system-features = nixos-test benchmark big-parallel kvm\nextra-system-features = surmount-remote\nsystem-features = nixos-test benchmark big-parallel kvm gccarch-x86-64-v3\n",
            "  extra-system-features = surmount-remote\nsystem-features = nixos-test benchmark big-parallel kvm\n",
            "extra-system-features = surmount-remote\nextra-system-features = other\nsystem-features = nixos-test benchmark big-parallel kvm\n",
            "extra-system-features = \"surmount-remote\"\nsystem-features = nixos-test benchmark big-parallel kvm\n",
            "\n",
            "system-features = nixos-test benchmark big-parallel kvm",
        ];
        for (idx, fixture) in fixtures.iter().enumerate() {
            let rust = bootstrap_gccarch_conf(fixture);
            let shell = shell_decision(fixture).unwrap_or_else(|err| {
                panic!("fixture {idx} shell failed: {err}\ninput:\n{fixture}")
            });
            match (rust, shell) {
                (GccarchConfDecision::LeaveUnchanged, ShellDecision::Leave) => {}
                (GccarchConfDecision::Rewrite(expected), ShellDecision::Rewrite(got)) => {
                    assert_eq!(
                        got, expected,
                        "fixture {idx} shell drifted from the rust transform\ninput:\n{fixture}"
                    );
                }
                (rust, _) => {
                    panic!(
                        "fixture {idx} shell decision does not match rust: {rust:?}\ninput:\n{fixture}"
                    )
                }
            }
        }
    }

    #[test]
    fn shell_blob_token_is_exact() {
        let script = format!(
            "deploy_host_gccarch_token={GCCARCH_X86_64_V3}\n\
             deploy_host_run_nix_conf={RUN_NIX_CONF}\n\
             {SHELL_FUNCTIONS}\n\
             if deploy_host_blob_has_token \"$1\"; then echo YES; else echo NO; fi\n"
        );
        let cases = [
            (
                "benchmark big-parallel kvm nixos-test surmount-remote",
                "NO",
            ),
            (
                "benchmark big-parallel kvm nixos-test surmount-remote gccarch-x86-64-v3",
                "YES",
            ),
            ("[ \"benchmark\" \"gccarch-x86-64-v3\" ]", "YES"),
            ("gccarch-x86-64-v30", "NO"),
            ("gccarch-x86-64", "NO"),
        ];
        for (blob, want) in cases {
            let out = Command::new("sh")
                .arg("-c")
                .arg(&script)
                .arg("sh")
                .arg(blob)
                .output()
                .expect("sh");
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(
                out.status.success(),
                "blob {blob:?} stderr {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(stdout.trim(), want, "blob {blob:?} stdout {stdout}");
        }
    }

    #[test]
    fn remote_driver_restarts_only_when_the_daemon_omits_the_feature() {
        let script = gccarch_bootstrap_remote();
        assert!(script.contains(GCCARCH_X86_64_V3));
        assert!(script.contains(SURMOUNT_REMOTE));
        assert!(script.contains(RUN_NIX_CONF));
        assert!(script.contains("nix config show system-features"));
        assert!(
            !script.contains("nix config show extra-system-features"),
            "Nix 2.34.9 has no extra-system-features show key"
        );
        assert!(!script.contains("> \"$store_conf\""));
        assert!(
            script
                .contains("deploy_host_rewrite_conf \"$store_conf\" \"$deploy_host_run_nix_conf\"")
        );
        assert!(script.contains("mount --bind"));
        assert!(script.contains("systemctl restart nix-daemon"));
        assert!(script.contains(
            "gccarch-x86-64-v3 still absent from nix config show system-features; not starting the system build"
        ));
        let check = script
            .find("if deploy_host_blob_has_token \"$deploy_host_daemon_features\"")
            .expect("daemon check");
        let else_at = script[check..].find("\nelse\n").expect("else branch") + check;
        let restart = script
            .find("systemctl restart nix-daemon")
            .expect("restart");
        assert!(
            restart > else_at,
            "restart must sit in the branch that runs only when the feature is missing"
        );
        assert!(
            !script[..else_at].contains("systemctl restart"),
            "already-advertised feature must not restart nix-daemon"
        );
        assert_eq!(script.matches("systemctl restart nix-daemon").count(), 1);
    }
}
