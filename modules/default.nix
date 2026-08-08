# Import all Surmount NixOS modules.
# Host configurations should import this file (or the flake nixosModule).

{
  imports = [
    ./options.nix
    ./secrets.nix
    ./networking.nix
    ./hardening.nix
    # Surmount 0.16+ service (dual-disables stock stalwart-mail.nix +
    # stalwart.nix; owns services.stalwart). Before mail.nix.
    ./stalwart-service.nix
    ./mail.nix
    ./web.nix
    ./management-ui.nix
    # Arti onion/hidden service (REQUIRED product surface; management-publish).
    ./arti-hidden-service.nix
    ./backups.nix
  ];
}
