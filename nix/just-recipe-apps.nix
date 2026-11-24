# Flake apps for just recipes. Each program is a symlink whose file name
# is the recipe. surmount-deploy-host reads argv0 and runs that recipe.
# Names that already point at another package stay in flake.nix so this
# file does not replace them.
# deploy is not in this list. nix run .#deploy is nix/deploy-app.nix
# (the deploy-rs switch). deploy-dry-run and deploy-host stay here.
{ self, system }:
let
  bin = "${self.packages.${system}.surmount-deploy-host}/bin/surmount-deploy-host";
  linkFarm = self.inputs.nixpkgs.legacyPackages.${system}.linkFarm;
  names = [
    "default"
    "check"
    "ci"
    "check-remote"
    "audit-remote"
    "remote-nix-build"
    "audit"
    "deny"
    "check-ci"
    "fmt"
    "fmt-write"
    "test"
    "clippy"
    "dev"
    "publish"
    "deploy-dry-run"
    "deploy-host"
    "btop"
    "host-inxi"
    "et"
    "grok-oss"
    "grok-oss-running"
    "host-logs"
    "host-status"
    "scram"
    "test-logging-eval"
    "test-sshd-hostkeys-eval"
    "test-eternal-terminal-eval"
    "test-inxi-eval"
    "test-btop-eval"
    "test-perf-tools-eval"
    "test-remote-builder-eval"
    "check-private-data"
    "check-leftover-agent-homes"
    "check-tls-hybrid"
    "secrets-prompt"
    "rekey"
    "diskstation-afp-mount"
    "diskstation-discover"
    "sync-static-sites-from-ds3018xs"
    "mailplus-copy-uid"
    "bootstrap-stalwart-token"
    "fix-public-dashboard"
    "host-cutover"
    "cutover-step"
    "render-host-profile-acme"
    "domain-audit"
    "dns-zone-namecheap"
    "rdns-shc"
    "laptop-renew-cert"
    "check-heavy"
    "vendor"
    "update-flakes"
    "update-software"
  ];
  farm = linkFarm "surmount-just-recipe-apps" (
    map (name: {
      name = name;
      path = bin;
    }) names
  );
in
builtins.listToAttrs (
  map (name: {
    inherit name;
    value = {
      type = "app";
      program = "${farm}/${name}";
    };
  }) names
)
