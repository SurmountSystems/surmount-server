# Surmount Server  -  main flake entry.
# Outputs: nixosConfigurations, packages, apps, checks, devShells, nixosModules.
#
# Hermetic: evaluation and builds use locked inputs (flake.lock). Secrets
# decrypt only on the target host via sops-nix; never at eval time from
# the public tree.
#
# CI entry: nix build .#checks.<system>.ci   (or: just check)
# End-to-end: nix run .#e2e (local) / nix run .#e2e-host (env-gated; never in ci)
# Heavy: mail-vm-test, mail-vps-eval, stalwart-mail package are optional.

{
  description = "Surmount mail + web NixOS foundation (Stalwart, HTTPS edge, management UI)";

  inputs = {
    # Prefer stable for a mail host. Bump deliberately after reading release notes.
    # Stalwart engine is NOT taken from this channel (0.11.8). See
    # nix/packages/stalwart-mail.nix and modules/stalwart-service.nix.
    # Arti engine is NOT taken from this channel (1.4.2). See
    # nix/packages/arti-onion-service.nix (Surmount-owned 2.5.0 source build).
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";

    # Rustc/cargo only for engines whose MSRV exceeds nixos-25.05 (default
    # 1.86; rustPackages_* max 1.89). Arti 2.5.0 MSRV is 1.91. Not a full OS
    # channel bump. Bump this input when Arti needs a newer rustc.
    nixpkgs-rust.url = "github:NixOS/nixpkgs/nixos-unstable";

    crane.url = "github:ipetkov/crane";
    # crane follows its own nixpkgs; we pass pkgs from our nixpkgs in outputs.

    sops-nix = {
      url = "github:Mic92/sops-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      nixpkgs-rust,
      crane,
      sops-nix,
      ...
    }:
    let
      inherit (nixpkgs) lib;

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
        # Distinct from stock prev.arti (client-default). Module prefers this
        # when surmount.artiHiddenService.package is null.
        artiOnionService = self.packages.${system}.arti-onion-service;
      };

      mkPkgs =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ (surmountOverlay system) ];
        };

      # Shared crane args for management-ui checks (test/clippy/fmt).
      # Leptos 0.8 SSR wants rustc >= 1.88; nixpkgs default on 25.05 is 1.86.
      mkManagementUiCrane =
        system:
        let
          pkgs = mkPkgsBare system;
          rustToolchain = import ./nix/rust-toolchain.nix { inherit pkgs; };
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
          src = lib.cleanSourceWith {
            src = craneLib.path ./crates;
            filter =
              path: type:
              (craneLib.filterCargoSources path type)
              || (builtins.match ".*\\.(html|css|js|svg|png|toml)$" path != null);
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
    in
    {
      nixosModules = {
        default = {
          imports = [
            ./modules
            sops-nix.nixosModules.sops
          ];
        };
        surmount = self.nixosModules.default;
      };

      nixosConfigurations = {
        mail-vps = nixpkgs.lib.nixosSystem {
          system = "x86_64-linux";
          specialArgs = { inherit self; };
          modules = [
            sops-nix.nixosModules.sops
            ./modules
            ./hosts/mail-vps/configuration.nix
            (
              { ... }:
              {
                nixpkgs.overlays = [ (surmountOverlay "x86_64-linux") ];
              }
            )
          ];
        };

        surmount-mail = self.nixosConfigurations.mail-vps;
      };

      packages = forAllSystems (
        system:
        let
          pkgs = mkPkgsBare system;
          craneLib = crane.mkLib pkgs;
          e2ePkgs = pkgs.callPackage ./nix/packages/surmount-e2e.nix { inherit craneLib; };
        in
        {
          management-ui = pkgs.callPackage ./nix/packages/management-ui.nix { inherit craneLib; };
          stalwart-mail = pkgs.callPackage ./nix/packages/stalwart-mail.nix { };
          stalwart = self.packages.${system}.stalwart-mail;
          stalwart-cli = pkgs.callPackage ./nix/packages/stalwart-cli.nix { };
          stalwart-webui = pkgs.callPackage ./nix/packages/stalwart-webui.nix { };
          stalwart-spam-filter = pkgs.callPackage ./nix/packages/stalwart-spam-filter.nix { };
          # Surmount-owned Arti 2.5.0 + onion-service-service (HS publish).
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

          management-ui-fmt = craneLib.cargoFmt {
            inherit src;
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

          # Single full nixosSystem module eval (secrets + Arti + management-ui).
          # Thin pure checks below do not re-import nixosSystem (Issue 7).
          module-eval-contract =
            let
              r = import ./tests/module-eval.nix {
                inherit pkgs lib;
                inherit sops-nix;
              };
            in
            pkgs.writeText "module-eval-contract" (builtins.toJSON r.ok);

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

          nix-fmt-check =
            pkgs.runCommand "nix-fmt-check"
              {
                nativeBuildInputs = [ pkgs.nixfmt-rfc-style ];
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
              nix-fmt-check
            ];
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
            nix-fmt-check
            ci
            ;

          # Package build (also in ci).
          management-ui = self.packages.${system}.management-ui;

          # Optional / heavy (not in ci aggregate).
          stalwart-webui = self.packages.${system}.stalwart-webui;
          stalwart-spam-filter = self.packages.${system}.stalwart-spam-filter;

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
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = mkPkgs system;
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              # Match crane management-ui toolchain (Leptos SSR MSRV 1.88).
              rustPackages_1_88.rustc
              rustPackages_1_88.cargo
              rustPackages_1_88.rustfmt
              rustPackages_1_88.clippy
              rust-analyzer
              pkg-config
              nixfmt-rfc-style
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
              echo "  just check          # CI quality bar (flake checks.ci)"
              echo "  just e2e            # local hermetic end-to-end (nix run .#e2e)"
              echo "  just e2e-host       # host probes (needs SURMOUNT_E2E_HOST=1)"
              echo "  cargo test -p surmount-management-ui"
              echo "  cargo test -p surmount-e2e --lib"
              echo "  nix build .#management-ui"
              echo "  nix build .#stalwart-mail   # long cargo build of engine 0.16+"
            '';
          };
        }
      );

      formatter = forAllSystems (system: (mkPkgs system).nixfmt-rfc-style);
    };
}
