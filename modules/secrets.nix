# Deploy secrets on host (bucket 1). NEVER put secrets in the public git tree:
# plaintext OR ciphertext. No .env, no age private keys, no LUKS unlock
# material, no "encrypted keyfile in git". Secrets live on the host /
# operator channels only. See docs/hygiene.md, docs/SECRETS.md.
#
# Three-layer architecture (do not mash):
#   A. Deploy / Nix activation secrets -> host paths + optional sops-nix
#      Material is host-local / out-of-band, not committed. Tool can change
#      (Q-DEP-1 open); the *need* for host material at activation is fixed.
#   B. Human / team vault              -> Vaultwarden planned (not here)
#   C. Disk at rest                    -> LUKS2 when install allows
#
# Fail-loud contract: when surmount.secrets.requireDeployMaterial is true,
# activation fails if any requiredHostPaths entry is missing on the host.
# sops-nix remains a scaffold option for decrypt-into-those-paths workflows.
#
# Path checks: requiredHostPaths is a list of { path; kind; } with kind
# file (-f) or directory (-d). Existence uses the matching test after strict
# charset validation so paths never become shell syntax.

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  inherit (lib)
    mkIf
    mkDefault
    mkMerge
    concatMapStringsSep
    escapeShellArg
    ;

  hostPaths = import ./lib/host-paths.nix { inherit lib; };

  requiredEntries = cfg.secrets.requiredHostPaths;
  requiredPathStrings = map (e: e.path) requiredEntries;

  testOpFor = kind: if kind == "directory" then "-d" else "-f";

  # Fail-loud activation helper. Every path use is escapeShellArg'd.
  # Paths are also charset-asserted before this script is generated.
  failLoudScript = pkgs.writeShellScript "surmount-check-deploy-secrets" ''
    set -eu
    missing=0
    ${concatMapStringsSep "\n" (e: ''
      if [ ! ${testOpFor e.kind} ${escapeShellArg e.path} ]; then
        echo "surmount: required deploy secret path missing (${e.kind}): ${escapeShellArg e.path}" >&2
        missing=1
      fi
    '') requiredEntries}
    if [ "$missing" -ne 0 ]; then
      echo "surmount: refuse to continue; install deploy material on the host (never in git)." >&2
      echo "surmount: see secrets/README.md and docs/SECRETS.md" >&2
      exit 1
    fi
  '';
in
{
  config = mkIf cfg.enable (mkMerge [
    {
      # sops-nix is imported from the flake (inputs.sops-nix.nixosModules.sops).
      # Defaults below are safe no-ops until host-local secret files exist.
      sops = {
        # defaultSopsFile must be a host-local path once you have material.
        # Do NOT commit encrypted secrets under secrets/ in this public tree.
        age = {
          keyFile = mkDefault "/var/lib/sops-nix/key.txt";
          sshKeyPaths = mkDefault [ "/etc/ssh/ssh_host_ed25519_key" ];
        };
      };

      assertions = [
        {
          assertion = (!cfg.secrets.requireDeployMaterial) || (requiredEntries != [ ]);
          message = ''
            surmount.secrets.requireDeployMaterial is true but
            surmount.secrets.requiredHostPaths is empty.
            List { path; kind; } entries (kind = file | directory) for TLS
            PEMs, Arti HS state dir, etc. Paths are on the host only; never
            put secret material in git.
          '';
        }
        {
          assertion = hostPaths.allStrictHostPaths requiredPathStrings;
          message = ''
            surmount.secrets.requiredHostPaths.*.path entries must be absolute
            host paths matching /[A-Za-z0-9._/-]+ with no ".." or metacharacters.
            Deploy material is operator-placed on the host only (never in git).
          '';
        }
        {
          assertion = hostPaths.optionalStrictHostPath cfg.secrets.deployMaterialDir;
          message = ''
            surmount.secrets.deployMaterialDir must be a strict absolute host
            path (charset /[A-Za-z0-9._/-]+).
          '';
        }
        {
          assertion = hostPaths.optionalStrictHostPath cfg.secrets.durableMaterialDir;
          message = ''
            surmount.secrets.durableMaterialDir must be a strict absolute host
            path (charset /[A-Za-z0-9._/-]+). Default /var/lib/surmount/secrets.
          '';
        }
      ];

      # Durable Domain B root only (no secret payloads). Root-owned, mode 0755
      # so service users can traverse to UI-owned leaves. Leaf dirs (ui/, acme/)
      # come from management-ui tmpfiles when that unit is enabled; install
      # bridge also creates parents. Laptop Domain A remains custody SoT.
      systemd.tmpfiles.rules = [
        "d ${cfg.secrets.durableMaterialDir} 0755 root root - -"
      ];
    }

    (mkIf (cfg.secrets.requireDeployMaterial && requiredEntries != [ ]) {
      system.activationScripts.surmountDeploySecrets = {
        deps = [ "specialfs" ];
        text = ''
          echo "surmount: checking required deploy secret paths..."
          ${failLoudScript}
        '';
      };

      systemd.services.surmount-deploy-secrets-check = {
        description = "Surmount deploy-secrets path check (fail loud if missing)";
        wantedBy = [ "multi-user.target" ];
        before = [
          "surmount-management-ui.service"
          "stalwart-mail.service"
        ];
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          ExecStart = "${failLoudScript}";
        };
      };
    })
  ]);
}
