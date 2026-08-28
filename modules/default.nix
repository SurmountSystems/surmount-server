# Import all Surmount NixOS modules.
# Host configurations should import this file (or the flake nixosModule).

{
  imports = [
    ./options.nix
    ./secrets.nix
    ./networking.nix
    ./hardening.nix
    # Eternal Terminal (etserver) for reconnecting operator SSH.
    ./eternal-terminal.nix
    # Persistent size-capped journald paper trail (sshd VERBOSE, journal group).
    ./logging.nix
    # Surmount 0.16+ service (dual-disables stock stalwart-mail.nix +
    # stalwart.nix; owns services.stalwart). Before mail.nix.
    ./stalwart-service.nix
    ./mail.nix
    ./web.nix
    ./management-ui.nix
    # Arti onion/hidden service (REQUIRED product surface; management-publish).
    ./arti-hidden-service.nix
    # Domain C human vault (Vaultwarden; sample host stays enable=false).
    ./vaultwarden.nix
    ./backups.nix
    # Operator host console: pkgs.btop (Eternal Terminal / just btop).
    ./btop.nix
    # Guest /root/justfile (diagnose). Same file on every replica after switch.
    ./operator-justfile.nix
    # Last-line RAM defense (surmount-scram --watch). Highest priority.
    ./scram.nix
    # Optional swap file (path from host-local).
    ./swapfile.nix
    # Operator host hardware probe: pkgs.inxi (SSH / just host-inxi; no sudo).
    ./inxi.nix
    # ssh-ng remote builder + MemoryMax on nix-daemon (opt-in).
    ./remote-builder.nix
    # Optional Lean/Lake: default off, niced, MemoryMax on the lake unit.
    ./lake.nix
  ];
}
