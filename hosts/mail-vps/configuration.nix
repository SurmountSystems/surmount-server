# Sample host: mail VPS for surmount.systems
#
# Operator TODOs before production:
#   1. Set a real networking.public IPv4/IPv6 and DNS (docs/DNS.md).
#   2. Install age key at /var/lib/sops-nix/key.txt (or rely on SSH host key).
#   3. Add your SSH public keys below.
#
#   --- (A) Product public edge (default: web.enable false) ---
#   4. Place TLS PEMs on the HOST only (never in git), e.g.
#      /run/surmount-secrets/tls/{cert,key}.pem (key mode 0600).
#   5. Uncomment managementUi listenMode=https + public bind + PEM paths below.
#   6. Set surmount.secrets.requireDeployMaterial = true and list PEM (and
#      Arti) paths in requiredHostPaths (loud activation gate). Unit also
#      uses ConditionPathExists on PEMs for https and restart caps.
#   7. Host smoke: RESIDUAL.md highest-value next / Validation SoT
#      (`nix run .#e2e-host` / just e2e-host with SURMOUNT_E2E_HOST=1): MDWE, curl /health over
#      TLS, nginx inactive when web off, UI no CAP_NET_ADMIN.
#
#   --- (B) Dual-run escape only (transitional nginx) ---
#   8. Set surmount.web.enable = true; keep managementUi listenMode=http
#      on loopback (module asserts against dual-run + public UI https).
#   9. Point ACME DNS A/AAAA at services/mail/apex for nginx vhosts.
#
#  10. Import Maildir via surmount-mail-import-maildir (docs/MIGRATION.md).
#  11. Configure Stalwart TLS cert paths once certs exist on host.
#  12. Review fail2ban/ssh exposure; lock SSH to admin nets if possible.
#  13. Enable surmount.artiHiddenService when HS identity is on host.
#      With https UI, leave arti backendAddress null so the module auto-
#      binds loopback cleartext API for the onion reverse-proxy (or set
#      managementUi.localCleartextListen / explicit backend).
#
# Apply (on the host, after first boot with nixos-anywhere / manual install):
#   nixos-rebuild switch --flake github:SurmountSystems/surmount-server#mail-vps
#   # or from a checkout:
#   nixos-rebuild switch --flake .#mail-vps

{
  config,
  lib,
  pkgs,
  modulesPath,
  ...
}:
{
  imports = [
    # Uncomment when you have hardware-configuration from nixos-generate-config:
    # ./hardware-configuration.nix
    (modulesPath + "/profiles/qemu-guest.nix")
  ];

  # ---- Surmount stack ------------------------------------------------------
  surmount = {
    enable = true;
    primaryDomain = "surmount.systems";
    mailHostname = "mail.surmount.systems";
    servicesHostname = "services.surmount.systems";
    acmeEmail = "admin@surmount.systems";

    # Legacy / extra domains (SPF/DKIM/DMARC each).
    additionalDomains = [
      # "old-brand.example"
    ];

    mailAccounts = [
      {
        localPart = "admin";
        displayName = "Admin";
        # passwordSecret = "mail/accounts/admin"; # after sops is wired
      }
      {
        localPart = "postmaster";
        displayName = "Postmaster";
      }
    ];

    managementUi = {
      enable = true;
      # Product public edge (web.enable default false): in-process rustls.
      # Place PEMs on the HOST only (never in git), then uncomment:
      #   listenMode = "https";
      #   listenAddress = "0.0.0.0";  # or the public address
      #   port = 443;
      #   tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
      #   tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      #   redirectHttpToHttps = true;  # plain HTTP redirect-only (default :80)
      #   # httpRedirectListen = "0.0.0.0:80";  # default; empty skips bind
      # When https + arti enable and backendAddress null, module auto-binds
      # loopback cleartext full API for onion rproxy (default 127.0.0.1:8090
      # when primary is public :443). Optional override:
      #   localCleartextListen = "127.0.0.1:8090";
      # Redirect-only :80 is NOT the cleartext API (no ACME product invent).
      # Until PEMs exist, leave listenMode=http (loopback) and use SSH tunnel.
      # Unit: StartLimitBurst/Interval + ConditionPathExists on PEMs (https);
      # CAP_NET_BIND_SERVICE when :80/:443. ACME HTTP-01 on product :80 parked.
      #
      # Dual-run escape (transitional nginx owns public :80/:443):
      #   listenMode = "http";  # loopback only -- not public https
      #   redirectHttpToHttps = false;  # required; eval fails if true with web
      #   # and set surmount.web.enable = true below
      # Eval fails if web.enable && https on :443 or non-loopback, or
      # web.enable && redirectHttpToHttps.
    };
    # Product default false (Axum-first). Escape while migrating:
    #   web.enable = true;
    hardening.enable = true;

    # Access control / kernel firewall ban sets (scaffold; Q-ACL policy open).
    # Default off. When ready for host lab enforce + helper:
    # accessControl = {
    #   enable = true;
    #   enforcement = "enforce";  # or "dry-run" first
    #   backend = "nft";
    #   nftHelper = true;         # UDS helper; UI keeps no CAP_NET_ADMIN
    #   # nftBin = "${pkgs.nftables}/bin/nft";  # if not defaulted by module
    #   # whitelist = [ "203.0.113.10/32" ];    # never ban operator nets
    # };
    # Lab cleanup after membership probe (host): helper remove-ban is idempotent
    # when the element is already absent:
    #   surmount-nft-ban-helper remove-ban 203.0.113.50
    # Live traffic drop remains operator residual (ban_drop=UNPROVEN in e2e-host).

    # Deploy secrets: production loud gate once material is on host.
    # Prefer requireDeployMaterial + PEM paths together with listenMode=https.
    # secrets.requireDeployMaterial = true;
    # secrets.requiredHostPaths = [
    #   { path = "/run/surmount-secrets/tls/cert.pem"; kind = "file"; }
    #   { path = "/run/surmount-secrets/tls/key.pem"; kind = "file"; }
    #   { path = "/run/surmount-secrets/arti/onion-service"; kind = "directory"; }
    # ];

    # Arti HS required product surface; enable when host HS state exists.
    # Happy path: enable + host HS dir (owned 0750 surmount-arti:surmount-arti) +
    # startDaemon. package null uses pkgs.artiOnionService from surmountOverlay
    # (onion-service-service; passthru capable claim). Do not set
    # packageIsOnionServiceCapable unless pointing package at a non-Surmount
    # binary (footgun if claimed true on stock client arti). Unit active !=
    # onion published (live Tor verify residual).
    # With managementUi listenMode=https, leave backendAddress/backendUnixSocket
    # null so the module auto-points onion rproxy at local cleartext API
    # (see managementUi.localCleartextListen notes above). Explicit plain-HTTP
    # backendAddress or unix: UDS also OK; do not point at the https primary TCP.
    # artiHiddenService.enable = true;
    # artiHiddenService.onionServiceStateDir = "/run/surmount-secrets/arti/onion-service";
    # artiHiddenService.startDaemon = true;
    # # artiHiddenService.backendAddress = "127.0.0.1:8090";  # only if overriding auto
    # # artiHiddenService.backendUnixSocket = "/run/surmount/management-ui.sock";

    # backups.enable = true;
    # backups.repository = "s3:s3.example/surmount-mail";
    # backups.passwordFile = config.sops.secrets."backups/restic_password".path;
  };

  # ---- Base system ---------------------------------------------------------
  networking.hostName = "mail-vps";
  # Sample host only. New installs track the flake host channel (26.05).
  # Do not change stateVersion on a live box without reading NixOS release notes.
  system.stateVersion = "26.05";

  boot.loader.grub = {
    # For cloud images / many VPS providers. Adjust for your hoster.
    device = lib.mkDefault "/dev/sda";
  };

  # TODO: replace with nixos-generate-config output for real disks.
  fileSystems."/" = lib.mkDefault {
    device = "/dev/disk/by-label/nixos";
    fsType = "ext4";
  };

  time.timeZone = "UTC";
  i18n.defaultLocale = "en_US.UTF-8";

  # ---- Networking (provider-specific) --------------------------------------
  # TODO: set the public address your MX/A records will point at.
  # networking.interfaces.eth0.ipv4.addresses = [{
  #   address = "203.0.113.10";
  #   prefixLength = 24;
  # }];
  # networking.defaultGateway = "203.0.113.1";
  networking.useDHCP = lib.mkDefault true;

  # ---- Users / SSH ---------------------------------------------------------
  users.users.root.openssh.authorizedKeys.keys = [
    # TODO: "ssh-ed25519 AAAA... you@workstation"
  ];

  users.users.surmount = {
    isNormalUser = true;
    extraGroups = [ "wheel" ];
    openssh.authorizedKeys.keys = [
      # TODO: "ssh-ed25519 AAAA... you@workstation"
    ];
  };

  # ---- Nix -----------------------------------------------------------------
  nix.settings = {
    experimental-features = [
      "nix-command"
      "flakes"
    ];
    trusted-users = [
      "root"
      "@wheel"
    ];
    # Prefer binary caches; avoid impure substitutes surprises in prod.
    substituters = [
      "https://cache.nixos.org"
    ];
    trusted-public-keys = [
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
    ];
  };

  nixpkgs.config.allowUnfree = false;

  # ---- Minimal conveniences ------------------------------------------------
  environment.systemPackages = with pkgs; [
    vim
    htop
    curl
    dig
    tcpdump
    git
  ];

  # journald retention so mail logs stay available for abuse review.
  services.journald.extraConfig = ''
    SystemMaxUse=1G
  '';
}
