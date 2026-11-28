//! Remote system build and switch command strings.
//!
//! `build_line` is only `nix build` of the toplevel. It does not name
//! `nixos-rebuild` and it does not run. `switch_line` builds that same
//! toplevel into a result symlink and execs that result's
//! `bin/switch-to-configuration`. The activator on generation 75 starts
//! with bash, so this string is not a refusal of another language.
//! Nothing here executes either string.

/// `path:/dir#attr` becomes the toplevel installable `nix build` accepts.
fn toplevel_installable(flake_q: &str) -> String {
    match flake_q.split_once('#') {
        Some((base, attr)) => {
            format!("{base}#nixosConfigurations.{attr}.config.system.build.toplevel")
        }
        None => flake_q.to_string(),
    }
}

pub fn build_line(flake_q: &str) -> String {
    build_line_in_store(flake_q, None)
}

/// Same command as [`build_line`]. When `store_uri` is set, the nix
/// client uses that store (`nix --store URI build`) and an empty
/// `build-users-group`. A root client with a non-empty group chowns
/// the store directory. Empty skips that chown. The build still runs.
/// None keeps the default store. This string still does not switch
/// the generation.
pub fn build_line_in_store(flake_q: &str, store_uri: Option<&str>) -> String {
    let installable = toplevel_installable(flake_q);
    let store_flag = match store_uri {
        Some(uri) if !uri.is_empty() => {
            format!(
                "--store {} --option build-users-group '' ",
                crate::paths::shell_quote(uri)
            )
        }
        _ => String::new(),
    };
    format!(
        "nice -n 19 nix {store_flag}build --extra-experimental-features 'nix-command flakes' --keep-going --no-link --print-out-paths --option max-jobs 32 --option cores 32 {installable}"
    )
}

/// Build the toplevel into `./result`, point the boot profile at that
/// store path, then exec that result's activator.
///
/// `switch-to-configuration` updates `/run/current-system` and the
/// bootloader. It does not change `/nix/var/nix/profiles/system`. Without
/// the `nix-env --set`, a reboot keeps the previous generation. The
/// profile is set before the activator so the bootloader install reads
/// the generation that is about to be activated. The exec is in a
/// subshell so the caller can record its exit status. Not executed by
/// this function.
pub fn switch_line(flake_q: &str) -> String {
    let installable = toplevel_installable(flake_q);
    format!(
        "nice -n 19 nix build --extra-experimental-features 'nix-command flakes' --print-out-paths --option max-jobs 32 --option cores 32 {installable} --out-link result && nix-env -p /nix/var/nix/profiles/system --set \"$(readlink -f result)\" && (exec result/bin/switch-to-configuration switch)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_hides_rebuild_or_python(cmd: &str) -> bool {
        // `nixos-rebuild` is also the prefix of the ng name and of the
        // wrapped filename, so one check rejects both.
        cmd.contains("nixos-rebuild") || cmd.contains("python")
    }

    #[test]
    fn rebuild_line_is_nix_build_and_rejects_hidden_python() {
        let installable =
            "path:/tmp/repo#nixosConfigurations.mail-vps.config.system.build.toplevel";
        let build = build_line("path:/tmp/repo#mail-vps");
        let switch = switch_line("path:/tmp/repo#mail-vps");
        assert_eq!(
            build,
            format!(
                "nice -n 19 nix build --extra-experimental-features 'nix-command flakes' --keep-going --no-link --print-out-paths --option max-jobs 32 --option cores 32 {installable}"
            )
        );
        assert!(build.starts_with("nice -n 19 nix build "));
        let with_store = build_line_in_store(
            "path:/tmp/repo#mail-vps",
            Some("local:///mnt/hdd/projects/server/vendor/nix-store"),
        );
        assert!(
            with_store.contains(
                "nice -n 19 nix --store local:///mnt/hdd/projects/server/vendor/nix-store --option build-users-group '' build "
            ),
            "{with_store}"
        );
        assert!(with_store.contains("--keep-going"), "{with_store}");
        assert!(with_store.contains("--no-link"), "{with_store}");
        assert!(
            !with_store.contains("switch-to-configuration"),
            "{with_store}"
        );
        assert_eq!(build_line_in_store("path:/tmp/repo#mail-vps", None), build);
        assert!(build.contains("--keep-going"));
        assert!(build.contains("--no-link"));
        assert!(build.contains("--print-out-paths"));
        assert!(build.contains("--option max-jobs 32"));
        assert!(build.contains("--option cores 32"));
        assert!(build.contains("nix-command flakes"));
        assert!(!build.contains("&&"), "{build}");
        assert!(!build.contains("switch-to-configuration"), "{build}");
        assert!(!build.contains("--out-link"), "{build}");
        assert!(switch.contains("--out-link result"), "{switch}");
        assert!(
            switch.contains(
                "nix-env -p /nix/var/nix/profiles/system --set \"$(readlink -f result)\""
            ),
            "{switch}"
        );
        let profile_at = switch
            .find("nix-env -p /nix/var/nix/profiles/system --set")
            .expect("profile set");
        let activate_at = switch
            .find("exec result/bin/switch-to-configuration switch")
            .expect("activator");
        assert!(
            profile_at < activate_at,
            "boot profile must be set before the activator: {switch}"
        );
        assert!(
            switch.contains("exec result/bin/switch-to-configuration switch"),
            "{switch}"
        );
        assert!(!switch.contains("--keep-going"), "{switch}");
        assert!(!switch.contains("--no-link"), "{switch}");
        for cmd in [&build, &switch] {
            assert!(cmd.contains("nix build"), "{cmd}");
            assert!(
                !command_hides_rebuild_or_python(cmd),
                "command must not name nixos-rebuild or python: {cmd}"
            );
        }
    }
}
