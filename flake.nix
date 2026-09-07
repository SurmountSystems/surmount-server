# Surmount Server  -  main flake entry.
# Outputs: nixosConfigurations, packages, apps, checks, devShells, nixosModules.
#
# Hermetic: evaluation and builds use locked inputs (flake.lock). Secrets
# decrypt only on the target host via sops-nix; never at eval time from
# the public tree.
#
# Host CI-style loop: just check  (fmt --check, clippy -D warnings, cargo test)
# Full flake CI aggregate: just ci  (alias check-ci; or: nix build .#checks.<system>.ci)
# GHA: .github/workflows/ci.yml job display name is `just ci` (required-check footgun)
# End-to-end: nix run .#e2e (local) / nix run .#e2e-host (env-gated; never in ci)
# Operator bins: nix run .#<app> -- args (just aliases only nix-run).
# Heavy: mail-vm-test, mail-vps-eval, stalwart-mail package are optional.

{
  description = "Surmount mail + web NixOS foundation (Stalwart, HTTPS edge, management UI)";

  # Host channel: nixos-26.05. Deploy: just deploy-host /
  # nix run .#surmount-deploy-host (public rsync + private host-local +
  # path: flake rebuild). See docs/deploy-host-local.md. Host-local under
  # ./host-local is auto-imported when present (path flake / remote rsync
  # without .git); never committed.

  # Mild discoverability only; host nix.settings already prefer cache.nixos.org.
  nixConfig = {
    extra-substituters = [ "https://cache.nixos.org" ];
    extra-trusted-public-keys = [
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
    ];
  };

  inputs = {
    # Prefer stable for a mail host. Bump deliberately after reading release notes.
    # Host channel: nixos-26.05 (crane wants >= 26.05; operator direction 2026-08-07).
    # Stalwart engine is NOT taken from this channel. See
    # nix/packages/stalwart-mail.nix and modules/stalwart-service.nix.
    # Arti engine is NOT taken from this channel. See
    # nix/packages/arti-onion-service.nix (Surmount-owned 2.5.1 source build).
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

    # Rustc/cargo only for engines whose MSRV exceeds the host channel default.
    # Arti 2.5.1 MSRV is 1.91. Keep a separate input so we can pin newer rustc
    # without waiting on every host-channel package set. Bump when Arti needs it.
    nixpkgs-rust.url = "github:NixOS/nixpkgs/nixos-unstable";

    # Pin input URLs to flake.lock revs for reproducible originals; bump via
    # intentional `nix flake update` (or lock edit), not floating HEAD.
    crane.url = "github:ipetkov/crane/756d6d07c3818ea95d1e2cdac63fa7d02fe3e61b";
    # crane follows its own nixpkgs; we pass pkgs from our nixpkgs in outputs.

    sops-nix = {
      url = "github:Mic92/sops-nix/f1406619a3884cd5c47992a70b8b35c9c0fcb4c9";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Public static site for apex/www. Fetch the git tree (HTML/CSS/JS).
    # Operator bumps: just deploy (nix flake update surmount-site).
    surmount-site = {
      url = "github:SurmountSystems/site";
      flake = false;
    };

    # RustSec advisory-db for hermetic cargo-audit (offline in checks.ci).
    # Bump: nix flake update advisory-db
    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };

    # Grok OSS TUI (bin/grok-oss). Product tip is still branch remote-1
    # (open PR 51; default branch main is #50 and is not this tip).
    # Operator bumps: nix flake update grok-oss.
    # Default-off NixOS module; package is fail-closed when enable=true.
    grok-oss.url = "github:SurmountSystems/grok-oss/remote-1";

    # Splora (Esplora-compatible indexer). Operator bumps with nix flake update splora.
    # NixOS module is nixosModules.splora. REST is one indexer instance plus
    # JSON-RPC and a cookie; a local bitcoind datadir is not required.
    splora.url = "github:SurmountSystems/splora/surmount";
  };

  outputs =
    {
      self,
      nixpkgs,
      nixpkgs-rust,
      crane,
      sops-nix,
      surmount-site,
      advisory-db,
      grok-oss,
      splora,
      ...
    }:
    let
      inherit (nixpkgs) lib;
      # Overlay attrs are also named splora / splora-liquid; keep the flake input.
      sploraInput = splora;

      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forAllSystems = f: nixpkgs.lib.genAttrs systems f;

      mkPkgsBare =
        system:
        import nixpkgs {
          inherit system;
        };

      # nixpkgs-rust: rustc/cargo for Arti MSRV + matching arti for cargoDeps.
      mkPkgsRust =
        system:
        import nixpkgs-rust {
          inherit system;
        };

      # rustPlatform whose rustc meets Arti MSRV; C deps still come from host pkgs.
      mkArtiRustPlatform =
        system:
        let
          pkgsRust = mkPkgsRust system;
        in
        (mkPkgsBare system).makeRustPlatform {
          inherit (pkgsRust) rustc cargo;
        };

      surmountOverlay = system: final: prev: {
        surmount-management-ui = self.packages.${system}.management-ui;
        stalwart-mail = self.packages.${system}.stalwart-mail;
        stalwart = self.packages.${system}.stalwart-mail;
        stalwart-cli = self.packages.${system}.stalwart-cli;
        vandelay = self.packages.${system}.vandelay;
        # Namecheap DNS-01 hook **code** (not secrets). management-ui may default
        # acme.dnsHookPath to this store path when external-hook is enabled.
        acme-dns-hook-namecheap = self.packages.${system}.acme-dns-hook-namecheap;
        # Distinct from stock prev.arti (client-default). Module prefers this
        # when surmount.artiHiddenService.package is null.
        artiOnionService = self.packages.${system}.arti-onion-service;
        # Public apex/www static site (SurmountSystems/site flake input).
        surmount-public-site = self.packages.${system}.surmount-public-site;
        # Ops bins (one callPackage file each; not a mega ops.nix).
        surmount-private-data = self.packages.${system}.surmount-private-data;
        surmount-host-logs = self.packages.${system}.surmount-host-logs;
        surmount-shc = self.packages.${system}.surmount-shc;
        surmount-niced-builder = self.packages.${system}.surmount-niced-builder;
        surmount-scram = self.packages.${system}.surmount-scram;
        surmount-leftover-homes = self.packages.${system}.surmount-leftover-homes;
        surmount-host-probe = self.packages.${system}.surmount-host-probe;
        surmount-static-sites = self.packages.${system}.surmount-static-sites;
        surmount-diskstation = self.packages.${system}.surmount-diskstation;
        surmount-deploy-host = self.packages.${system}.surmount-deploy-host;
        surmount-dns-zone = self.packages.${system}.surmount-dns-zone;
        surmount-domain-audit = self.packages.${system}.surmount-domain-audit;
        surmount-host-cutover = self.packages.${system}.surmount-host-cutover;
        surmount-acme-namecheap = self.packages.${system}.surmount-acme-namecheap;
        surmount-stalwart-ops = self.packages.${system}.surmount-stalwart-ops;
        surmount-mail-import = self.packages.${system}.surmount-mail-import;
        surmount-secrets-install = self.packages.${system}.surmount-secrets-install;
        surmount-secrets-prompt = self.packages.${system}.surmount-secrets-prompt;
        surmount-rekey = self.packages.${system}.surmount-rekey;
        grok-oss = self.packages.${system}.grok-oss;
        # Flake-input packages. Upstream crane src omits .cargo/config.toml
        # (laptop cargo still uses Menhera). Do not wrap src again here.
        splora = self.packages.${system}.splora;
        splora-liquid = self.packages.${system}.splora-liquid;
      };

      mkPkgs =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ (surmountOverlay system) ];
        };

      # Shared crane args for management-ui checks (test/clippy/fmt).
      # Toolchain from nix/rust-toolchain.nix (nixpkgs-rust / nixos-unstable).
      mkManagementUiCrane =
        system:
        let
          pkgs = mkPkgsBare system;
          pkgsRust = mkPkgsRust system;
          rustToolchain = import ./nix/rust-toolchain.nix { pkgs = pkgsRust; };
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
          src = lib.cleanSourceWith {
            src = craneLib.path ./.;
            filter =
              path: type:
              (craneLib.filterCargoSources path type)
              || (builtins.match ".*\\.(html|css|js|svg|png|toml|json|txt|md|sh|nix)$" path != null)
              || (builtins.match ".*authorized_keys$" path != null)
              # Extra-Host NIP-05 fixture: testdata/.../.well-known/nostr.json.
              # filterCargoSources drops dotdirs.
              || (type == "directory" && baseNameOf path == ".well-known");
          };
          commonArgs = {
            inherit src;
            pname = "surmount-management-ui";
            version = "0.1.0";
            strictDeps = true;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ ];
            cargoExtraArgs = "-p surmount-management-ui";
          };
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        in
        {
          inherit
            pkgs
            craneLib
            src
            commonArgs
            cargoArtifacts
            ;
        };

      # Private host-local overlay at flake root ./host-local (gitignored).
      # Public git eval: path missing => empty list (same as today).
      # After deploy-host rsync (no .git + host-local at REMOTE/host-local/),
      # path: flake rebuild sees the dir and applies modules. Known names only
      # (no wildcard *.nix) so stray files cannot surprise switch.
      # Layout: docs/deploy-host-local.md
      hostLocalModules =
        let
          hl = ./host-local;
        in
        if !(builtins.pathExists hl) then
          [ ]
        else if builtins.pathExists (hl + "/default.nix") then
          # Operator-owned entry module (may import hardware, net, keys).
          [ hl ]
        else if builtins.pathExists (hl + "/host-local.nix") then
          # First-deploy style single overlay (imports hardware itself when needed).
          # Holds real hostname / addressing / SSH keys. Never commit this file.
          [ (hl + "/host-local.nix") ]
        else
          (lib.optional (builtins.pathExists (hl + "/hardware-configuration.nix")) (
            hl + "/hardware-configuration.nix"
          ))
          ++ (lib.optional (builtins.pathExists (hl + "/hostname.nix")) (hl + "/hostname.nix"))
          ++ (lib.optional (builtins.pathExists (hl + "/networking.nix")) (hl + "/networking.nix"))
          # Private ACME fragment from host profile render
          # (surmount-render-host-profile-acme -> host-local-acme.nix).
          ++ (lib.optional (builtins.pathExists (hl + "/host-local-acme.nix")) (hl + "/host-local-acme.nix"))
          ++ (
            if builtins.pathExists (hl + "/authorized_keys") then
              [
                (
                  { lib, ... }:
                  let
                    authKeys = hl + "/authorized_keys";
                  in
                  {
                    # B0: wire host-local key file into evaluated SSH config
                    # (driver file check alone is not enough without this).
                    users.users.root.openssh.authorizedKeys.keyFiles = lib.mkAfter [ authKeys ];
                    # Sample host defines surmount; merges when present.
                    users.users.surmount.openssh.authorizedKeys.keyFiles = lib.mkAfter [ authKeys ];
                  }
                )
              ]
            else
              [ ]
          );
    in
    {
      nixosModules = {
        default = {
          imports = [
            ./modules
            sops-nix.nixosModules.sops
            sploraInput.nixosModules.splora
          ];
        };
        surmount = self.nixosModules.default;
      };

      # Primary sample host: mail-vps (nixos-26.05). Generic public path only;
      # real hostname / hardware / SSH keys via ./host-local when present.
      # surmount-mail is a historical product alias (same config).
      # Do not enable public https / Arti / Vaultwarden here without host material.
      nixosConfigurations = {
        mail-vps = nixpkgs.lib.nixosSystem {
          system = "x86_64-linux";
          specialArgs = { inherit self; };
          modules = [
            sops-nix.nixosModules.sops
            ./modules
            sploraInput.nixosModules.splora
            ./hosts/mail-vps/configuration.nix
            (
              { ... }:
              {
                nixpkgs.overlays = [ (surmountOverlay "x86_64-linux") ];
              }
            )
          ]
          ++ hostLocalModules;
        };

        surmount-mail = self.nixosConfigurations.mail-vps;
      };

      packages = forAllSystems (
        system:
        let
          pkgs = mkPkgsBare system;
          pkgsRust = mkPkgsRust system;
          rustToolchain = import ./nix/rust-toolchain.nix { pkgs = pkgsRust; };
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
          e2ePkgs = pkgs.callPackage ./nix/packages/surmount-e2e.nix {
            inherit craneLib pkgsRust;
          };
        in
        {
          management-ui = pkgs.callPackage ./nix/packages/management-ui.nix {
            inherit craneLib pkgsRust;
          };
          stalwart-mail = pkgs.callPackage ./nix/packages/stalwart-mail.nix { };
          stalwart = self.packages.${system}.stalwart-mail;
          stalwart-cli = pkgs.callPackage ./nix/packages/stalwart-cli.nix { };
          vandelay = pkgs.callPackage ./nix/packages/vandelay.nix { };
          stalwart-webui = pkgs.callPackage ./nix/packages/stalwart-webui.nix { };
          stalwart-spam-filter = pkgs.callPackage ./nix/packages/stalwart-spam-filter.nix { };
          # DNS-01 Namecheap helper **code** only (credentials stay Domain A /run).
          acme-dns-hook-namecheap = pkgs.callPackage ./nix/packages/acme-dns-hook-namecheap.nix { };
          # Apex/www static files from locked github:SurmountSystems/site.
          surmount-public-site = pkgs.callPackage ./nix/packages/surmount-public-site.nix {
            src = surmount-site;
          };
          # Surmount-owned Arti 2.5.1 + onion-service-service (HS publish).
          # Not a drop-in for pkgs.arti; heavy cargo build, not in checks.ci.
          # rustPlatform + artiUnstable.cargoDeps from nixpkgs-rust (MSRV 1.91+;
          # vendor handoff avoids crates.io 403). C deps from host pkgs.
          arti-onion-service = pkgs.callPackage ./nix/packages/arti-onion-service.nix {
            rustPlatform = mkArtiRustPlatform system;
            artiUnstable = (mkPkgsRust system).arti;
          };
          # End-to-end runners (Rust). Host app is never a flake check.
          e2e = e2ePkgs.e2e;
          e2e-host = e2ePkgs.e2e-host;
          # Operator tools (Rust + crane). One file each under nix/packages/.
          # just aliases only nix-run. Crate src missing => package evals,
          # build fails closed.
          surmount-private-data = pkgs.callPackage ./nix/packages/surmount-private-data.nix {
            inherit craneLib pkgsRust;
          };
          surmount-host-logs = pkgs.callPackage ./nix/packages/surmount-host-logs.nix {
            inherit craneLib pkgsRust;
          };
          surmount-shc = pkgs.callPackage ./nix/packages/surmount-shc.nix {
            inherit craneLib pkgsRust;
          };
          surmount-niced-builder = pkgs.callPackage ./nix/packages/surmount-niced-builder.nix {
            inherit craneLib pkgsRust;
          };
          surmount-scram = pkgs.callPackage ./nix/packages/surmount-scram.nix {
            inherit craneLib pkgsRust;
          };
          surmount-leftover-homes = pkgs.callPackage ./nix/packages/surmount-leftover-homes.nix {
            inherit craneLib pkgsRust;
          };
          surmount-host-probe = pkgs.callPackage ./nix/packages/surmount-host-probe.nix {
            inherit craneLib pkgsRust;
          };
          surmount-static-sites = pkgs.callPackage ./nix/packages/surmount-static-sites.nix {
            inherit craneLib pkgsRust;
          };
          surmount-diskstation = pkgs.callPackage ./nix/packages/surmount-diskstation.nix {
            inherit craneLib pkgsRust;
          };
          surmount-deploy-host = pkgs.callPackage ./nix/packages/surmount-deploy-host.nix {
            inherit craneLib pkgsRust;
          };
          surmount-dns-zone = pkgs.callPackage ./nix/packages/surmount-dns-zone.nix {
            inherit craneLib pkgsRust;
          };
          surmount-domain-audit = pkgs.callPackage ./nix/packages/surmount-domain-audit.nix {
            inherit craneLib pkgsRust;
          };
          surmount-host-cutover = pkgs.callPackage ./nix/packages/surmount-host-cutover.nix {
            inherit craneLib pkgsRust;
          };
          surmount-acme-namecheap = pkgs.callPackage ./nix/packages/surmount-acme-namecheap.nix {
            inherit craneLib pkgsRust;
          };
          surmount-stalwart-ops = pkgs.callPackage ./nix/packages/surmount-stalwart-ops.nix {
            inherit craneLib pkgsRust;
          };
          surmount-mail-import = pkgs.callPackage ./nix/packages/surmount-mail-import.nix {
            inherit craneLib pkgsRust;
          };
          surmount-secrets-install = pkgs.callPackage ./nix/packages/surmount-secrets-install.nix {
            inherit craneLib pkgsRust;
          };
          surmount-secrets-prompt = pkgs.callPackage ./nix/packages/surmount-secrets-prompt.nix {
            inherit craneLib pkgsRust;
          };
          surmount-rekey = pkgs.callPackage ./nix/packages/surmount-rekey.nix {
            inherit craneLib pkgsRust;
          };
          # Grok OSS from locked github:SurmountSystems/grok-oss (operator-bump).
          # Not in checks.ci (heavy crane). Module stays default-off.
          grok-oss = pkgs.callPackage ./nix/packages/grok-oss.nix {
            grokOssFlake = grok-oss;
            inherit system;
          };
          # Flake-input splora packages. Upstream crane omits
          # .cargo/config.toml from src and builds --offline --locked.
          # Not in checks.ci: the real package is a rocksdb crane build.
          splora =
            sploraInput.packages.${system}.splora
              or (throw "splora flake input has no packages.${system}.splora (fail-closed)");
          splora-liquid =
            sploraInput.packages.${system}.splora-liquid
              or (throw "splora flake input has no packages.${system}.splora-liquid (fail-closed)");
          # RFC-style nixfmt (same as formatter / checks.ci / devShell).
          # Use: nix run .#nixfmt -- file.nix   or   nix shell .#nixfmt -c nixfmt …
          # nixos-26.05: nixfmt-rfc-style is an alias of pkgs.nixfmt (prefer the latter).
          nixfmt = pkgs.nixfmt;
          # Flake-pinned just for GHA (`nix shell .#just -c just ci`) and local
          # bootstrap without a host just install. Same pattern as grok-oss.
          just = pkgs.just;
          default = self.packages.${system}.management-ui;
        }
      );

      apps = forAllSystems (system: {
        e2e = {
          type = "app";
          program = "${self.packages.${system}.e2e}/bin/surmount-e2e";
        };
        e2e-host = {
          type = "app";
          program = "${self.packages.${system}.e2e-host}/bin/surmount-e2e-host";
        };
        surmount-private-data = {
          type = "app";
          program = "${self.packages.${system}.surmount-private-data}/bin/surmount-private-data";
        };
        surmount-host-logs = {
          type = "app";
          program = "${self.packages.${system}.surmount-host-logs}/bin/surmount-host-logs";
        };
        surmount-shc = {
          type = "app";
          program = "${self.packages.${system}.surmount-shc}/bin/surmount-shc";
        };
        surmount-niced-builder = {
          type = "app";
          program = "${self.packages.${system}.surmount-niced-builder}/bin/surmount-niced-builder";
        };
        surmount-scram = {
          type = "app";
          program = "${self.packages.${system}.surmount-scram}/bin/surmount-scram";
        };
        surmount-leftover-homes = {
          type = "app";
          program = "${self.packages.${system}.surmount-leftover-homes}/bin/surmount-leftover-homes";
        };
        surmount-inxi-host = {
          type = "app";
          program = "${self.packages.${system}.surmount-host-probe}/bin/surmount-inxi-host";
        };
        surmount-btop-host = {
          type = "app";
          program = "${self.packages.${system}.surmount-host-probe}/bin/surmount-btop-host";
        };
        surmount-tls-hybrid = {
          type = "app";
          program = "${self.packages.${system}.surmount-host-probe}/bin/surmount-tls-hybrid";
        };
        surmount-et = {
          type = "app";
          program = "${self.packages.${system}.surmount-host-probe}/bin/surmount-et";
        };
        surmount-grok-oss = {
          type = "app";
          program = "${self.packages.${system}.surmount-host-probe}/bin/surmount-grok-oss";
        };
        surmount-deploy-static-sites = {
          type = "app";
          program = "${self.packages.${system}.surmount-static-sites}/bin/surmount-deploy-static-sites";
        };
        surmount-sync-static-sites = {
          type = "app";
          program = "${self.packages.${system}.surmount-static-sites}/bin/surmount-sync-static-sites";
        };
        surmount-diskstation-afp-mount = {
          type = "app";
          program = "${self.packages.${system}.surmount-diskstation}/bin/surmount-diskstation-afp-mount";
        };
        surmount-diskstation-discover = {
          type = "app";
          program = "${self.packages.${system}.surmount-diskstation}/bin/surmount-diskstation-discover";
        };
        surmount-copy-mailplus-uid = {
          type = "app";
          program = "${self.packages.${system}.surmount-diskstation}/bin/surmount-copy-mailplus-uid";
        };
        surmount-fix-public-dashboard = {
          type = "app";
          program = "${self.packages.${system}.surmount-diskstation}/bin/surmount-fix-public-dashboard";
        };
        surmount-deploy-host = {
          type = "app";
          program = "${self.packages.${system}.surmount-deploy-host}/bin/surmount-deploy-host";
        };
        surmount-dns-zone = {
          type = "app";
          program = "${self.packages.${system}.surmount-dns-zone}/bin/surmount-dns-zone";
        };
        surmount-domain-audit = {
          type = "app";
          program = "${self.packages.${system}.surmount-domain-audit}/bin/surmount-domain-audit";
        };
        surmount-host-cutover = {
          type = "app";
          program = "${self.packages.${system}.surmount-host-cutover}/bin/surmount-host-cutover";
        };
        acme-dns-hook-namecheap-bin = {
          type = "app";
          program = "${self.packages.${system}.surmount-acme-namecheap}/bin/acme-dns-hook-namecheap";
        };
        surmount-laptop-renew-cert = {
          type = "app";
          program = "${self.packages.${system}.surmount-acme-namecheap}/bin/surmount-laptop-renew-cert";
        };
        surmount-render-host-profile-acme = {
          type = "app";
          program = "${
            self.packages.${system}.surmount-acme-namecheap
          }/bin/surmount-render-host-profile-acme";
        };
        acme-dns-hook-namecheap-dispatch = {
          type = "app";
          program = "${self.packages.${system}.surmount-acme-namecheap}/bin/acme-dns-hook-namecheap-dispatch";
        };
        acme-dns-hook-lab = {
          type = "app";
          program = "${self.packages.${system}.surmount-acme-namecheap}/bin/acme-dns-hook-lab";
        };
        surmount-deploy-host-post-switch-smoke = {
          type = "app";
          program = "${
            self.packages.${system}.surmount-deploy-host
          }/bin/surmount-deploy-host-post-switch-smoke";
        };
        surmount-host-material-inventory = {
          type = "app";
          program = "${self.packages.${system}.surmount-host-cutover}/bin/surmount-host-material-inventory";
        };
        register-dkim = {
          type = "app";
          program = "${self.packages.${system}.surmount-stalwart-ops}/bin/register-dkim";
        };
        free-stalwart-public-443 = {
          type = "app";
          program = "${self.packages.${system}.surmount-stalwart-ops}/bin/free-stalwart-public-443";
        };
        point-stalwart-mail-tls = {
          type = "app";
          program = "${self.packages.${system}.surmount-stalwart-ops}/bin/point-stalwart-mail-tls";
        };
        add-stalwart-token = {
          type = "app";
          program = "${self.packages.${system}.surmount-stalwart-ops}/bin/add-stalwart-token";
        };
        bootstrap-stalwart-api-token = {
          type = "app";
          program = "${self.packages.${system}.surmount-stalwart-ops}/bin/bootstrap-stalwart-api-token";
        };
        stalwart-recovery-unlock = {
          type = "app";
          program = "${self.packages.${system}.surmount-stalwart-ops}/bin/stalwart-recovery-unlock";
        };
        surmount-mail-import-maildir = {
          type = "app";
          program = "${self.packages.${system}.surmount-mail-import}/bin/surmount-mail-import-maildir";
        };
        secrets-install-host = {
          type = "app";
          program = "${self.packages.${system}.surmount-secrets-install}/bin/secrets-install-host";
        };
        secrets-export-bw-to-staging = {
          type = "app";
          program = "${self.packages.${system}.surmount-secrets-install}/bin/secrets-export-bw-to-staging";
        };
        surmount-secrets-prompt = {
          type = "app";
          program = "${self.packages.${system}.surmount-secrets-prompt}/bin/surmount-secrets-prompt";
        };
        surmount-rekey = {
          type = "app";
          program = "${self.packages.${system}.surmount-rekey}/bin/surmount-rekey";
        };
      });

      checks = forAllSystems (
        system:
        let
          pkgs = mkPkgs system;
          ui = mkManagementUiCrane system;
          inherit (ui)
            craneLib
            commonArgs
            cargoArtifacts
            src
            ;

          management-ui-test = craneLib.cargoTest (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoTestExtraArgs = "-p surmount-management-ui";
            }
          );

          management-ui-clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- -D warnings";
            }
          );

          # Explicit pname/version: workspace root Cargo.toml has no [package]
          # name; crane would otherwise warn and use placeholder 0.0.1.
          management-ui-fmt = craneLib.cargoFmt {
            inherit src;
            pname = "surmount-management-ui";
            version = "0.1.0";
          };

          # Pure e2e helpers + host-gate contracts (no VPS; not host probes).
          e2e-pure-test = craneLib.cargoTest (
            commonArgs
            // {
              inherit cargoArtifacts;
              pname = "surmount-e2e-pure";
              cargoExtraArgs = "-p surmount-e2e";
              cargoTestExtraArgs = "-p surmount-e2e --lib";
            }
          );

          # Clippy for e2e crate bins + lib (management-ui clippy is package-scoped).
          e2e-clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              pname = "surmount-e2e";
              cargoExtraArgs = "-p surmount-e2e";
              cargoClippyExtraArgs = "--all-targets -- -D warnings";
            }
          );

          # Ops crate src present (main or lib). Missing src stays out of
          # checks.ci and out of nix flake check so a half-landed crate does
          # not red the host quality bar.
          wave1CrateLanded =
            name:
            builtins.pathExists (./crates + "/${name}/src/main.rs")
            || builtins.pathExists (./crates + "/${name}/src/lib.rs");

          mkWave1Test =
            pname: extra:
            craneLib.cargoTest (
              commonArgs
              // extra
              // {
                inherit cargoArtifacts;
                inherit pname;
                cargoExtraArgs = "-p ${pname}";
                cargoTestExtraArgs = "-p ${pname}";
              }
            );

          mkWave1Clippy =
            pname: extra:
            craneLib.cargoClippy (
              commonArgs
              // extra
              // {
                inherit cargoArtifacts;
                inherit pname;
                cargoExtraArgs = "-p ${pname}";
                cargoClippyExtraArgs = "--all-targets -- -D warnings";
              }
            );

          waveOps = [
            {
              crate = "surmount-private-data";
              extraNative = [ pkgs.git ];
            }
            {
              crate = "surmount-host-logs";
              extraNative = [ ];
            }
            {
              crate = "surmount-shc";
              extraNative = [ ];
            }
            {
              crate = "surmount-niced-builder";
              extraNative = [
                pkgs.util-linux
                pkgs.coreutils
              ];
            }
            {
              crate = "surmount-scram";
              extraNative = [ ];
            }
            {
              crate = "surmount-leftover-homes";
              extraNative = [ pkgs.git ];
            }
            {
              crate = "surmount-host-probe";
              extraNative = [ ];
            }
            {
              crate = "surmount-static-sites";
              extraNative = [ ];
            }
            {
              crate = "surmount-diskstation";
              extraNative = [ ];
            }
            {
              crate = "surmount-deploy-host";
              extraNative = [ ];
            }
            {
              crate = "surmount-dns-zone";
              extraNative = [ ];
            }
            {
              crate = "surmount-domain-audit";
              extraNative = [ ];
            }
            {
              crate = "surmount-host-cutover";
              extraNative = [ ];
            }
            {
              crate = "surmount-acme-namecheap";
              extraNative = [ ];
            }
            {
              crate = "surmount-stalwart-ops";
              extraNative = [ ];
            }
            {
              crate = "surmount-mail-import";
              extraNative = [ ];
            }
            {
              crate = "surmount-secrets-install";
              extraNative = [ pkgs.coreutils ];
            }
            {
              crate = "surmount-secrets-prompt";
              extraNative = [ pkgs.bash ];
            }
            {
              crate = "surmount-rekey";
              extraNative = [ ];
            }
          ];

          wave1CheckAttrs = lib.foldl' (
            acc: spec:
            let
              landed = wave1CrateLanded spec.crate;
            in
            acc
            // lib.optionalAttrs landed {
              ${spec.crate} = self.packages.${system}.${spec.crate};
              "${spec.crate}-test" = mkWave1Test spec.crate {
                nativeBuildInputs = commonArgs.nativeBuildInputs ++ spec.extraNative;
              };
              "${spec.crate}-clippy" = mkWave1Clippy spec.crate { };
            }
          ) { } waveOps;

          wave1CiExtras = lib.concatMap (
            spec:
            lib.optionals (wave1CrateLanded spec.crate) [
              self.packages.${system}.${spec.crate}
              wave1CheckAttrs."${spec.crate}-test"
              wave1CheckAttrs."${spec.crate}-clippy"
            ]
          ) waveOps;

          # Single full nixosSystem module eval (secrets + Arti + management-ui).
          # Thin pure checks below do not re-import nixosSystem (Issue 7).
          module-eval-contract =
            let
              r = import ./tests/module-eval.nix {
                inherit pkgs lib;
                inherit sops-nix;
                sploraNixosModule = sploraInput.nixosModules.splora;
              };
            in
            pkgs.writeText "module-eval-contract" (builtins.toJSON r.ok);

          # Eval-only: flake-input packages pass through (no crane build).
          splora-package-contract =
            let
              r = import ./tests/splora-package.nix {
                inherit lib system;
                sploraFlake = sploraInput;
              };
            in
            pkgs.writeText "splora-package-contract" (builtins.toJSON r.ok);

          # Pure host-path charset only (no nixosSystem).
          deploy-secrets-contract =
            let
              r = import ./tests/deploy-secrets.nix { inherit lib; };
            in
            pkgs.writeText "deploy-secrets-contract" (builtins.toJSON r.ok);

          # Pure Arti path-shape only (no nixosSystem; full arti in module-eval).
          arti-module-contract =
            let
              r = import ./tests/arti-module.nix { inherit lib; };
            in
            pkgs.writeText "arti-module-contract" (builtins.toJSON r.ok);

          # Eval-only: packaged SurmountSystems/site files (no network after lock).
          public-site-package-contract =
            let
              bare = mkPkgsBare system;
              r = import ./tests/public-site-package.nix {
                pkgs = bare;
                inherit lib;
                src = surmount-site;
              };
            in
            pkgs.writeText "public-site-package-contract" (builtins.toJSON r.ok);

          # Eval-only: arti-onion-service version/features/passthru (no cargo build).
          arti-onion-package-contract =
            let
              # Bare pkgs + same args as packages.*.arti-onion-service.
              bare = mkPkgsBare system;
              r = import ./tests/arti-onion-package.nix {
                pkgs = bare;
                inherit lib;
                rustPlatform = mkArtiRustPlatform system;
                artiUnstable = (mkPkgsRust system).arti;
              };
            in
            pkgs.writeText "arti-onion-package-contract" (builtins.toJSON r.ok);

          # Offline RustSec audit of Cargo.lock (no crates.io).
          cargo-audit =
            pkgs.runCommand "surmount-cargo-audit"
              {
                nativeBuildInputs = [ pkgs.cargo-audit ];
              }
              ''
                set -euo pipefail
                cargo-audit audit --no-fetch --stale \
                  -d ${advisory-db} \
                  -f ${./Cargo.lock}
                mkdir -p "$out"
                echo ok > "$out/ok"
              '';

          # Ban sha1/md5 *crate names* in the lockfile (no cargo metadata).
          # deny.toml is the human/cargo-deny config for a full shell.
          cargo-deny-bans = pkgs.runCommand "surmount-cargo-deny-bans" { } ''
            set -euo pipefail
            if grep -E '^name = "(sha1|sha-1|md5|md-5)"$' ${./Cargo.lock}; then
              echo "banned hasher crate name in Cargo.lock" >&2
              exit 1
            fi
            mkdir -p "$out"
            echo ok > "$out/ok"
          '';

          nix-fmt-check =
            pkgs.runCommand "nix-fmt-check"
              {
                nativeBuildInputs = [ pkgs.nixfmt ];
              }
              ''
                set -euo pipefail
                root=${self}
                fail=0
                mapfile -d "" -t files < <(
                  find "$root/modules" "$root/tests" "$root/hosts" "$root/nix" \
                    -type f -name '*.nix' -print0 2>/dev/null
                  printf '%s\0' "$root/flake.nix"
                )
                for f in "''${files[@]}"; do
                  [ -n "$f" ] || continue
                  if ! nixfmt --check "$f"; then
                    echo "nixfmt failed: $f" >&2
                    fail=1
                  fi
                done
                if [ "$fail" -ne 0 ]; then
                  exit 1
                fi
                echo ok > "$out"
              '';

          # Fast CI aggregate: rust quality + module contracts + nixfmt.
          # Excludes mail-vm-test, mail-vps toplevel, stalwart FODs, host e2e.
          ci = pkgs.releaseTools.aggregate {
            name = "surmount-ci";
            constituents = [
              self.packages.${system}.management-ui
              management-ui-test
              management-ui-clippy
              management-ui-fmt
              e2e-pure-test
              e2e-clippy
              module-eval-contract
              deploy-secrets-contract
              arti-module-contract
              arti-onion-package-contract
              public-site-package-contract
              splora-package-contract
              nix-fmt-check
              cargo-deny-bans
              cargo-audit
            ]
            ++ wave1CiExtras;
          };
        in
        {
          inherit
            management-ui-test
            management-ui-clippy
            management-ui-fmt
            e2e-pure-test
            e2e-clippy
            module-eval-contract
            deploy-secrets-contract
            arti-module-contract
            arti-onion-package-contract
            public-site-package-contract
            splora-package-contract
            nix-fmt-check
            cargo-audit
            cargo-deny-bans
            ci
            ;

          # Package build (also in ci).
          management-ui = self.packages.${system}.management-ui;

          # Optional / heavy (not in ci aggregate).
          stalwart-webui = self.packages.${system}.stalwart-webui;
          stalwart-spam-filter = self.packages.${system}.stalwart-spam-filter;

          # Primary sample host toplevel eval (optional / heavy; not in ci).
          mail-vps-eval =
            if system == "x86_64-linux" then
              self.nixosConfigurations.mail-vps.config.system.build.toplevel
            else
              pkgs.runCommand "mail-vps-eval-skip" { } ''
                echo "skip nixosConfiguration eval on ${system}" > "$out"
              '';

          mail-vm-test =
            if system == "x86_64-linux" then
              import ./tests/mail.nix {
                inherit pkgs;
                inherit (pkgs) lib;
                nixosTest = pkgs.nixosTest or pkgs.testers.runNixOSTest;
                surmountModules = [
                  sops-nix.nixosModules.sops
                  ./modules
                  (
                    { ... }:
                    {
                      nixpkgs.overlays = [ (surmountOverlay system) ];
                    }
                  )
                ];
                managementUi = self.packages.${system}.management-ui;
              }
            else
              pkgs.runCommand "mail-vm-test-skip" { } ''
                echo "skip vm test on ${system}" > "$out"
              '';
        }
        // wave1CheckAttrs
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = mkPkgs system;
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              # Match crane toolchain (nix/rust-toolchain.nix via nixpkgs-rust).
              (import ./nix/rust-toolchain.nix { pkgs = mkPkgsRust system; })
              rust-analyzer
              pkg-config
              nixfmt
              just
              sops
              age
              ssh-to-age
              git
              jq
            ];
            RUST_LOG = "info,surmount_management_ui=debug";
            shellHook = ''
              echo "Surmount dev shell  -  crates/ for Rust, modules/ for NixOS"
              echo "  just dev            # local management console → http://127.0.0.1:8080/"
              echo "  just check          # fmt --check + clippy + test (CI-style host bar)"
              echo "  just ci             # full flake checks.<system>.ci (GHA quality job)"
              echo "  just audit          # cargo-audit --offline (RustSec advisory-db)"
              echo "  just deny           # cargo-deny bans (no sha1/md5 crates)"
              echo "  just fmt-write      # apply cargo fmt + flake nixfmt"
              echo "  just e2e            # local hermetic end-to-end (nix run .#e2e)"
              echo "  just e2e-host       # host probes (needs SURMOUNT_E2E_HOST=1)"
              echo "  cargo test -p surmount-management-ui"
              echo "  cargo test -p surmount-e2e --lib"
              echo "  nix run .#surmount-private-data -- --tree"
              echo "  nix run .#surmount-host-logs -- --status"
              echo "  nix run .#surmount-shc -- --help"
              echo "  nix run .#surmount-inxi-host -- --help"
              echo "  nix run .#surmount-leftover-homes -- --tree"
              echo "  nix build .#management-ui"
              echo "  nix build .#stalwart-mail   # long cargo build of engine 0.16+"
            '';
          };
        }
      );

      formatter = forAllSystems (system: (mkPkgs system).nixfmt);
    };
}
