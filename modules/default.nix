# Import all Surmount NixOS modules.
# Host configurations should import this file (or the flake nixosModule).

{
  imports = [
    ./options.nix
    ./secrets.nix
    ./networking.nix
    ./hardening.nix
    # Surmount 0.16+ service (disables nixpkgs TOML module). Before mail.nix.
    ./stalwart-service.nix
    ./mail.nix
    ./web.nix
    ./management-ui.nix
    # Arti onion/hidden service (REQUIRED product surface; management-publish).
    ./arti-hidden-service.nix
    ./backups.nix
  ];
}
