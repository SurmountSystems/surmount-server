# Shared Surmount options: domains, accounts, paths, and feature toggles.
# Other modules read config.surmount.* so host configuration stays declarative
# and free of duplicated strings.

{
  lib,
  ...
}:
let
  inherit (lib) mkOption mkEnableOption types;
in
{
  options.surmount = {
    enable = mkEnableOption "Surmount mail + web stack modules";

    primaryDomain = mkOption {
      type = types.str;
      default = "surmount.systems";
      description = "Apex domain used for mail identity and default vhosts.";
    };

    mailHostname = mkOption {
      type = types.str;
      default = "mail.surmount.systems";
      description = "FQDN advertised in SMTP banners, TLS, and MX targets.";
    };

    servicesHostname = mkOption {
      type = types.str;
      default = "services.surmount.systems";
      description = "Hostname for the management UI (reverse-proxied).";
    };

    additionalDomains = mkOption {
      type = types.listOf types.str;
      default = [ ];
      example = [
        "legacy.example.com"
        "old-brand.net"
      ];
      description = ''
        Extra mail domains (aliases / multi-tenant legacy). Add SPF/DKIM/DMARC
        DNS for each; see docs/DNS.md.
      '';
    };

    acmeEmail = mkOption {
      type = types.str;
      default = "admin@surmount.systems";
      description = "Contact email for Let's Encrypt / ACME registration.";
    };

    # Declarative account *names* only. Passwords live in sops secrets.
    mailAccounts = mkOption {
      type = types.listOf (
        types.submodule {
          options = {
            localPart = mkOption {
              type = types.str;
              example = "admin";
              description = "Local part before @domain.";
            };
            domain = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Domain override; null means primaryDomain.";
            };
            displayName = mkOption {
              type = types.str;
              default = "";
              description = "Human-readable name for directory entries.";
            };
            # Secret path key under sops (not the password itself).
            passwordSecret = mkOption {
              type = types.str;
              default = "";
              example = "mail/accounts/admin";
              description = ''
                sops key name for this account password. Empty means the
                operator will create the account out-of-band (stalwart-cli).
              '';
            };
          };
        }
      );
      default = [ ];
      example = [
        {
          localPart = "admin";
          displayName = "Admin";
          passwordSecret = "mail/accounts/admin";
        }
      ];
      description = "Sample / production mail accounts (no plaintext secrets).";
    };

    stateDir = mkOption {
      type = types.path;
      default = "/var/lib/surmount";
      description = ''
        Root for Surmount-owned state (UI, import staging, future product DB).
        Inventory and backup expectations: docs/DATASTORES.md.
      '';
    };

    mailDataDir = mkOption {
      type = types.path;
      default = "/var/lib/stalwart-mail";
      description = ''
        Stalwart data directory (must match services.stalwart.dataDir).
        Scaffold default: RocksDB at ''${mailDataDir}/db for all four store
        roles. Design, gates, and alternatives: docs/DATASTORES.md and
        docs/open-choices.md. Not "whatever nixpkgs defaulted to."
      '';
    };

    # Future (not implemented): explicit store backend switch for Stalwart.
    # When added, options should name engine + paths per role (data/blob/fts/
    # lookup) and force DATASTORES.md + open-choices updates. Do not add a
    # silent multi-backend switch without migration runbooks.
    # mailStore = { backend = "rocksdb" | "postgresql" | ...; ... };

    # Deploy secrets on host (bucket 1). Material never in git.
    # Tool can change (Q-DEP-1); need for host paths at activation is fixed.
    secrets = {
      requireDeployMaterial = mkOption {
        type = types.bool;
        default = false;
        description = ''
          When true, activation fails loud if any requiredHostPaths entry is
          missing on the host. Default false so scaffold hosts and VM smoke
          still eval without real secrets. Production hosts should set true
          once material is installed out-of-band.
        '';
      };

      deployMaterialDir = mkOption {
        type = types.str;
        default = "/run/surmount-secrets";
        description = ''
          Ephemeral host directory for optional short-lived deploy material
          under tmpfs (wiped on reboot). Operator-placed only; never a path
          under the public git tree. Not automatically created with secret
          contents. New profiles default TLS PEMs + ACME account under
          durableMaterialDir (H-PEM); /run remains a valid override when the
          operator prefers re-issue-after-reboot.
        '';
      };

      durableMaterialDir = mkOption {
        type = types.str;
        default = "/var/lib/surmount/secrets";
        description = ''
          Durable Domain B root for activation copies that must survive reboot
          (TLS PEMs + ACME account JSON recommended default H-PEM; session
          secret, Stalwart API token, Namecheap DNS-01 env, Vaultwarden admin
          env). Mode not world-writable; root-owned parent 0755; leaf dirs
          0750 surmount-ui; leaf files mode 0600 (UI-consumed leaves owned by
          surmount-ui when installed via secrets-install-host). Laptop Domain A
          remains custody SoT. Never a path under the public git tree. See
          docs/SECRETS.md.
        '';
      };

      requiredHostPaths = mkOption {
        type = types.listOf (
          types.submodule {
            options = {
              path = mkOption {
                type = types.str;
                example = "/run/surmount-secrets/tls/cert.pem";
                description = "Absolute host path (no secret values).";
              };
              kind = mkOption {
                type = types.enum [
                  "file"
                  "directory"
                ];
                example = "file";
                description = ''
                  Host check: file (-f) or directory (-d). Required per entry
                  so PEM files and Arti state dirs can mix in one list.
                '';
              };
            };
          }
        );
        default = [ ];
        example = [
          {
            path = "/var/lib/surmount/secrets/tls/cert.pem";
            kind = "file";
          }
          {
            path = "/var/lib/surmount/secrets/tls/key.pem";
            kind = "file";
          }
          {
            # Optional ephemeral override remains allowlisted.
            path = "/run/surmount-secrets/arti/onion-service";
            kind = "directory";
          }
        ];
        description = ''
          Host paths that must exist when requireDeployMaterial is true.
          Each entry is { path; kind; } with kind file or directory (no
          global kind; no weak "any" default). Paths only (no secret values).
          Empty list with require true is a configuration error (assertion).
          Strict charset /[A-Za-z0-9._/-]+ (no metacharacters).
        '';
      };
    };

    managementUi = {
      enable = mkOption {
        type = types.bool;
        default = true;
        description = "Enable the Rust management UI service + reverse proxy.";
      };

      listenAddress = mkOption {
        type = types.str;
        default = "127.0.0.1";
        description = "Bind address for the management UI (loopback only).";
      };

      port = mkOption {
        type = types.port;
        # 8090: Stalwart 0.16 first-boot defaults bind HTTP on :8080; keep
        # the Surmount UI on a distinct loopback port.
        default = 8090;
        description = "Local HTTP port for the management UI (loopback).";
      };

      package = mkOption {
        type = types.nullOr types.package;
        default = null;
        description = "Override management-ui package; null uses pkgs.surmount-management-ui overlay.";
      };

      # Axum edge foundation (TLS paths from host deploy secrets; no secrets in git).
      listenMode = mkOption {
        type = types.enum [
          "http"
          "https"
        ];
        default = "http";
        description = ''
          Management UI listen mode. "https" terminates TLS 1.3 in-process
          (rustls) using tlsCertPath/tlsKeyPath host deploy-secret PEMs and is
          the public edge when web.enable is false (product default). "http"
          is for dev/scaffold and dual-run behind transitional nginx
          (set surmount.web.enable = true).
        '';
      };

      # Emergency only: always forces cleartext under the https label when set.
      allowCleartextHttpsEscape = mkOption {
        type = types.bool;
        default = false;
        description = ''
          DANGEROUS cleartext override. When true with listenMode=https, sets
          SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE=1 and the binary **always**
          binds cleartext under the https label. It does **not** take the
          rustls path even when PEMs load and the acceptor is ready. Default
          false. Not for production. Happy path is bare listenMode=https with
          PEMs and real TLS terminate (leave this option false).
        '';
      };

      tlsCertPath = mkOption {
        type = types.str;
        default = "";
        example = "/var/lib/surmount/secrets/tls/cert.pem";
        description = ''
          Absolute host path to TLS certificate PEM when listenMode is https.
          Prefer durable Domain B under secrets.durableMaterialDir (H-PEM).
          Ephemeral /run/surmount-secrets/tls/cert.pem remains valid.
        '';
      };

      tlsKeyPath = mkOption {
        type = types.str;
        default = "";
        example = "/var/lib/surmount/secrets/tls/key.pem";
        description = ''
          Absolute host path to TLS private key PEM when listenMode is https.
          Prefer durable Domain B (mode 0600 surmount-ui after install/issue;
          owner-only, not group-readable). Stalwart mail-plane TLS uses copies
          under secrets/mail/tls. Ephemeral
          /run/surmount-secrets/tls/key.pem remains valid.
        '';
      };

      # In-process ACME (DNS-01). Default off. Not ACME-only forever; static PEMs stay.
      # HTTP-01 on product :80 parked. external-hook = operator DNS-01 adapter;
      # live host LE + hot-reload residual (not a commercial DNS brand lock-in).
      acme = {
        enable = mkOption {
          type = types.bool;
          default = false;
          description = ''
            Enable in-process ACME issuance inside management-ui (instant-acme,
            DNS-01). Default false. When false, listenMode=https requires
            existing host PEMs at tlsCertPath/tlsKeyPath. When true, the binary
            reuses valid PEMs or attempts issuance (fail-closed on failure; no
            silent cleartext). CI never requires live Let's Encrypt. Prefer
            staging directory first. See docs/EDGE_AND_TLS.md and docs/OPS.md.
          '';
        };

        directory = mkOption {
          type = types.str;
          default = "";
          example = "https://acme-staging-v02.api.letsencrypt.org/directory";
          description = ''
            ACME directory URL. Empty default. When acme.enable, must be set
            (e.g. Let's Encrypt staging, then production). Emitted as
            SURMOUNT_ACME_DIRECTORY. Never required when enable is false.
          '';
        };

        email = mkOption {
          type = types.str;
          default = "";
          description = ''
            ACME account contact email (mailto). Empty default. Required when
            acme.enable. Emitted as SURMOUNT_ACME_EMAIL. Not a secret; still
            do not invent production addresses in the public tree.
          '';
        };

        domains = mkOption {
          type = types.listOf types.str;
          default = [ ];
          example = [
            "services.example.test"
            "mail.example.test"
          ];
          description = ''
            DNS identifiers for the ACME order. Empty default. Required
            non-empty when acme.enable. Joined into SURMOUNT_ACME_DOMAINS.
          '';
        };

        accountCredentialsPath = mkOption {
          type = types.str;
          default = "";
          example = "/var/lib/surmount/secrets/acme/account.json";
          description = ''
            Absolute host path for serialized ACME account credentials JSON
            (never in git). Empty default. Required when acme.enable. Prefer
            durable Domain B under secrets.durableMaterialDir (H-PEM).
            Ephemeral /run path remains valid. Emitted as
            SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH.
          '';
        };

        challenge = mkOption {
          type = types.enum [ "dns-01" ];
          default = "dns-01";
          description = ''
            ACME challenge type. Only dns-01 in this build (fits redirect-only
            product :80). HTTP-01 on product :80 is parked. Emitted as
            SURMOUNT_ACME_CHALLENGE.
          '';
        };

        dnsProvider = mkOption {
          type = types.enum [
            "none"
            "mock"
            "external-hook"
          ];
          default = "none";
          description = ''
            ACME DNS-01 *challenge adapter* (creates/deletes _acme-challenge TXT),
            not a commercial DNS brand and not "Surmount DNS product."
            "none" (default): reuse valid PEMs only; issuance fails closed without
            an adapter. "mock": hermetic/lab self-signed issuer (not live Let's
            Encrypt; refused against production LE directory). "external-hook":
            run an operator-owned absolute executable (dnsHookPath /
            SURMOUNT_ACME_DNS_HOOK) with argv set|clear|wait so the hook can talk
            to whatever DNS the operator already runs. No Cloudflare/Route53
            crates. Emitted as SURMOUNT_ACME_DNS_PROVIDER.
          '';
        };

        dnsHookPath = mkOption {
          type = types.str;
          default = "";
          example = "/run/surmount/acme-dns-hook";
          description = ''
            Absolute host path to the DNS-01 hook executable when dnsProvider is
            "external-hook". Empty option default; when acme.enable and
            external-hook, the management-ui module mkDefaults this to the
            packaged Namecheap helper store path (pkgs.acme-dns-hook-namecheap)
            when that package is on the overlay. That package is **code** only
            (`nix run .#acme-dns-hook-namecheap-bin` / pkgs.acme-dns-hook-namecheap);
            Namecheap API credentials
            never embed in the Nix package or git. Prefer durable Domain B
            namecheap.env at /var/lib/surmount/secrets/acme/namecheap.env
            (survives reboot; install kind namecheap-api); optional ephemeral
            /run/surmount-secrets/acme/namecheap.env remains allowlisted.
            Override dnsHookPath with any operator absolute path. Required
            (absolute) when enable and external-hook. Binary must be a regular
            file (no symlink), executable, not group/world-writable. Prefer a
            path outside ACME PEM ReadWritePaths parents (store path is ideal).
            Emitted as SURMOUNT_ACME_DNS_HOOK. Protocol: set name value; clear
            name; optional wait name value (exit 2 = unsupported).
          '';
        };

        dnsHookTimeoutSecs = mkOption {
          type = types.ints.between 1 600;
          default = 60;
          description = ''
            Timeout seconds for one external-hook invocation (range 1..600).
            Emitted as SURMOUNT_ACME_DNS_HOOK_TIMEOUT_SECS. Scaffold default 60.
          '';
        };

        renewDaysBeforeExpiry = mkOption {
          type = types.ints.unsigned;
          default = 30;
          description = ''
            Reissue when remaining leaf lifetime is under this many days
            (scaffold default 30; common LE operator practice, not locked CA law).
            Emitted as SURMOUNT_ACME_RENEW_DAYS_BEFORE_EXPIRY when acme.enable.
            Zero means only full notAfter fails the usability check.
          '';
        };
      };

      redirectHttpToHttps = mkOption {
        type = types.bool;
        default = false;
        description = ''
          When true with a non-empty httpRedirectListen, the management-ui
          binary binds a plain HTTP redirect-only listener (no cleartext API)
          and upgrades allowlisted Hosts to HTTPS. Sets
          SURMOUNT_REDIRECT_HTTP_TO_HTTPS. Requires listenMode=https (eval
          asserts). Product path: web.enable=false. Fail-closed at eval if
          web.enable is true (nginx dual-run owns public :80/:443 ACME/redirect).
          ACME HTTP-01 on the product :80 listener is not implemented (parked;
          Q-EDGE / RESIDUAL).
        '';
      };

      httpRedirectListen = mkOption {
        type = types.str;
        default = "0.0.0.0:80";
        example = "0.0.0.0:80";
        description = ''
          Bind address for the plain HTTP redirect-only listener when
          redirectHttpToHttps is true. Emitted as SURMOUNT_HTTP_REDIRECT_LISTEN.
          Must be empty or host:port (e.g. 0.0.0.0:80, [::]:80). Empty string
          skips the listen env (no bind) even if the flag is true. Default
          0.0.0.0:80. CAP_NET_BIND_SERVICE is granted only when the primary port
          or this listen port is under 1024.
        '';
      };

      # Full management API on loopback cleartext (not redirect-only :80).
      # Used when primary is https and a local reverse-proxy (Arti HS lean path)
      # cannot speak TLS to the UI TCP target.
      localCleartextListen = mkOption {
        type = types.nullOr types.str;
        default = null;
        example = "127.0.0.1:8090";
        description = ''
          Optional plain HTTP bind for the full management API (health, SSR,
          stubs), loopback only. Not the redirect-only :80 listener.
          Emitted as SURMOUNT_LOCAL_CLEARTEXT_LISTEN when set or when auto-
          derived for Arti.

          Auto path (null): when managementUi is https (escape off), Arti HS
          is enabled, and artiHiddenService.backendAddress / backendUnixSocket
          are both null, the module binds a dedicated loopback cleartext API
          and points the onion reverse-proxy at it. Auto port prefers
          127.0.0.1:8090, then 8091, then primary+1, always avoiding
          managementUi.port and the active redirect port (so 0.0.0.0:8090
          primary does not collide with 127.0.0.1:8090). Explicit host:port
          here overrides the auto address (numeric loopback only: 127.0.0.1
          or [::1]; not localhost hostnames). Must differ in port from primary
          and redirect listens. Never a public cleartext API. Redirect-only
          :80 stays redirect-only (no ACME product invent).
        '';
      };

      # MTA-STS policy skeleton (default off until public HTTPS policy host).
      mtaStsMode = mkOption {
        type = types.enum [
          "off"
          "testing"
          "enforce"
        ];
        default = "off";
        description = ''
          MTA-STS policy mode for GET /.well-known/mta-sts.txt on the product
          edge when Host is mta-sts.<primaryDomain> (RFC 8461). Default off
          (404). Use testing before enforce. Emitted as SURMOUNT_MTA_STS_MODE.
          Policy body uses mailHostname as mx. DNS TXT _mta-sts and A/AAAA for
          the policy host remain operator residual (docs/DNS.md).
        '';
      };

      mtaStsMaxAge = mkOption {
        type = types.ints.unsigned;
        default = 86400;
        description = ''
          max_age seconds in the MTA-STS policy body when mtaStsMode is not off.
          Emitted as SURMOUNT_MTA_STS_MAX_AGE. Scaffold default 86400 (1 day).
        '';
      };

      apexPublicRoot = mkOption {
        type = types.nullOr types.str;
        default = "/var/lib/surmount/public-site";
        description = ''
          Absolute directory for the public apex/www static site.
          When the flake overlay provides pkgs.surmount-public-site, the
          management-ui module mkDefaults this to that store path (locked
          github:SurmountSystems/site). A host directory such as
          /var/lib/surmount/public-site remains a valid override.
          When the directory contains index.html, those files are served on
          apex and www. Missing directory or missing index.html keeps the
          UNDER CONSTRUCTION page. Emitted as SURMOUNT_APEX_PUBLIC_ROOT.
        '';
      };

      staticVhosts = mkOption {
        type = types.attrsOf (
          types.submodule {
            options = {
              root = mkOption {
                type = types.str;
                example = "/var/lib/surmount/static-sites/cryptoquick";
                description = "Absolute document root for this HTTP Host.";
              };
            };
          }
        );
        default = { };
        example = {
          "extra.example.test" = {
            root = "/var/lib/surmount/static-sites/extra";
          };
          "www.extra.example.test" = {
            root = "/var/lib/surmount/static-sites/extra";
          };
        };
        description = ''
          Extra clearnet Host -> document root map (not apex/www SurmountSystems/site,
          not services console). Each key is a hostname (include www aliases
          pointing at the same root). Served with the same path/MIME/CSP rules
          as apex static files. Missing index.html is a closed 404, never the
          operator console. Emitted as SURMOUNT_STATIC_VHOSTS_FILE (JSON object).
          Do not overload apexPublicRoot for these names.
          The management-ui module mkDefaults proven DS3018xs static sites to
          /var/lib/surmount/static-sites/<slug>. Populate with
          just sync-static-sites-from-ds3018xs. See docs/EDGE_AND_TLS.md.
        '';
      };

      extraMailHostnames = mkOption {
        type = types.listOf types.str;
        default = [ ];
        example = [ "mail.cryptoquick.com" ];
        description = ''
          Extra mail Hosts that are not mailHostname and not staticVhosts keys
          (for example mail.cryptoquick.com on the production leaf). Emitted as
          SURMOUNT_EXTRA_MAIL_HOSTNAMES (comma list) for the management UI
          domains inventory. Do not parse /etc/surmount/mail-domains.txt as
          source of truth. The management-ui module mkDefaults
          mail.cryptoquick.com when provenStaticVhosts includes cryptoquick.com.
        '';
      };

      rateLimitMaxRequests = mkOption {
        type = types.ints.unsigned;
        default = 120;
        description = "Fixed-window max requests per client IP (0 disables).";
      };

      rateLimitWindowSecs = mkOption {
        type = types.ints.positive;
        default = 60;
        description = "Fixed-window length in seconds for edge rate limit.";
      };

      rateLimitMaxKeys = mkOption {
        type = types.ints.positive;
        default = 50000;
        description = "Cap on distinct rate-limit keys retained in memory.";
      };

      # Operator-published onion for admin UI display (never invent a live onion).
      onionUrl = mkOption {
        type = types.str;
        default = "";
        # v3 onion labels are 56 base32 chars (placeholder x's, not a live address).
        example = "http://xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx.onion";
        description = ''
          Optional operator-published onion URL for the management console
          (SURMOUNT_ONION_URL). Bare .onion hostnames are fine; the binary
          normalizes to http:// for display links. Empty = unset. Wins over
          onionHostnameFile when non-empty. Do not invent an address; set only
          after Arti has published on the host.
        '';
      };

      onionHostnameFile = mkOption {
        type = types.str;
        default = "";
        example = "/run/surmount-secrets/arti/onion-service/hostname";
        description = ''
          Optional host path to a file containing a bare .onion hostname
          (SURMOUNT_ONION_HOSTNAME_FILE). Read when onionUrl is empty.
          Empty file or unreadable path = hostname_missing status (not an
          invented address). When empty and surmount.artiHiddenService.enable
          is true, management-ui.nix derives
          onionServiceStateDir + "/hostname" and also sets
          SURMOUNT_ONION_HS_STATE_DIR so the binary can walk nested Arti
          keystore layout for a hostname file. Product residual names
          surmount.artiHiddenService; lab SURMOUNT_ONION_URL remains for tests.
        '';
      };

      # Operator-published Vaultwarden URL for console link (domain C human vault).
      # Never invent; empty = residual not configured. No admin token in UI/env.
      vaultwardenUrl = mkOption {
        type = types.str;
        default = "";
        example = "http://127.0.0.1:8222";
        description = ''
          Optional operator-published Vaultwarden URL for the management
          console (SURMOUNT_VAULTWARDEN_URL). When non-empty, system/overview
          show configured + "Open vault" external link. When empty and
          surmount.vaultwarden.enable is true, management-ui.nix may derive a
          private loopback URL from vaultwarden rocket listen (honest SSH
          tunnel / local path only; not a public invent). When
          vaultwardenProxyEnable is true and this is empty, management-ui.nix
          may derive https://{servicesHostname}/vault (public subpath shape).
          Empty with VW off and proxy off = residual not configured. Never put
          ADMIN_TOKEN or passwords here.

          Shape (eval fail-closed when set): http:// or https:// only; charset
          safe for systemd Environment= (no whitespace, newlines, quotes, $,
          backticks, or other metacharacters). Binary also rejects non-http(s)
          schemes (javascript:, data:, ...).
        '';
      };

      # Axum path reverse-proxy to loopback Vaultwarden (default off). No nginx.
      vaultwardenProxyEnable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          When true, management-ui reverse-proxies public prefix
          vaultwardenProxyPrefix (default /vault) to vaultwardenProxyUpstream
          (default loopback Rocket). SURMOUNT_VAULTWARDEN_PROXY=1. Vaultwarden
          login is SoT on that path (no Nostr gate day-one). WebSocket Upgrade
          is forwarded. Prefer with surmount.vaultwarden.enable and a matching
          vaultwarden.domain / vaultwardenUrl public base. Sample host stays
          off. Never ADMIN_TOKEN in this option.
        '';
      };

      vaultwardenProxyPrefix = mkOption {
        type = types.str;
        default = "/vault";
        description = ''
          Public path prefix for the Vaultwarden Axum proxy
          (SURMOUNT_VAULTWARDEN_PROXY_PREFIX). Normalized without trailing
          slash. Default /vault.
        '';
      };

      vaultwardenProxyUpstream = mkOption {
        type = types.str;
        default = "";
        example = "http://127.0.0.1:8222";
        description = ''
          Upstream base for the Vaultwarden path proxy
          (SURMOUNT_VAULTWARDEN_PROXY_UPSTREAM). Empty = derive
          http://{vaultwarden.rocketAddress}:{vaultwarden.rocketPort} when
          vaultwarden is enabled, else http://127.0.0.1:8222. Loopback only
          is the honest product default.
        '';
      };

      # HTTP/3 (QUIC) on the same bind as TCP HTTPS. Default on when
      # listenMode is https. SURMOUNT_HTTP3=0 turns it off.
      http3Enable = mkOption {
        type = types.bool;
        default = true;
        description = ''
          When listenMode is https, bind UDP QUIC on the same address:port as
          TCP HTTPS (product :443) and serve the same Axum router over HTTP/3
          (ALPN h3 only; TCP ALPN stays h2 + http/1.1). Same host PEMs.
          SURMOUNT_HTTP3. Default true; binary still defaults HTTP/3 on only
          for https listen. Set false to skip the QUIC listener and omit
          clearnet h3 Alt-Svc. Onion h2 Alt-Svc is separate.
        '';
      };

      # Nostr auth scaffold (default off). Q-AUTH-1 residual: key-loss, durable
      # session store, first-operator bootstrap product UX. Session secret is a
      # host deploy secret (never in git). Empty allowlist + mode=nostr = fail-closed.
      authMode = mkOption {
        type = types.enum [
          "off"
          "nostr"
        ];
        default = "off";
        description = ''
          SURMOUNT_AUTH_MODE. off = open console (local/dev default on
          loopback/private binds only; not public-safe). Public primary
          listen (non-loopback / 0.0.0.0) refuses authMode=off at Nix eval
          and process start unless lab-only allowPublicAuthOff. nostr = gate
          admin HTML + JSON APIs behind session cookie or valid NIP-98
          (rust-nostr; not JS NDK). Requires non-empty sessionSecret when
          nostr.
        '';
      };

      nostrAllowlist = mkOption {
        type = types.str;
        default = "";
        example = "npub1...,hex...";
        description = ''
          SURMOUNT_NOSTR_ALLOWLIST: comma/space separated bech32 npub or hex
          pubkeys. Scaffold bootstrap allowlist (Q-AUTH-1 first-operator UX
          still open). Empty with authMode=nostr means nobody authenticates
          (fail-closed) unless nostrAllowlistFile supplies keys. Non-empty
          env/string wins over the file path.
        '';
      };

      nostrAllowlistFile = mkOption {
        type = types.str;
        default = "";
        example = "/run/surmount-secrets/ui/nostr-allowlist";
        description = ''
          Optional host path for SURMOUNT_NOSTR_ALLOWLIST_FILE. Same token
          rules as nostrAllowlist (npub/hex, whitespace/comma separated).
          Used when nostrAllowlist is empty. Unreadable file fails closed at
          process start. Prefer host-only file for longer lists; never commit
          allowlist secrets if treated as sensitive.
        '';
      };

      sessionSecretPath = mkOption {
        type = types.str;
        default = "";
        example = "/var/lib/surmount/secrets/ui/session-secret";
        description = ''
          Host path to a systemd EnvironmentFile that sets
          SURMOUNT_SESSION_SECRET=... (KEY=value lines). Loaded when non-empty.
          Prefer durable Domain B path under durableMaterialDir; never commit.
          Empty = unset (required when authMode=nostr unless sessionSecretEnv
          is set for lab only).
        '';
      };

      sessionSecretEnv = mkOption {
        type = types.str;
        default = "";
        description = ''
          Optional raw SURMOUNT_SESSION_SECRET value for lab only. Prefer
          sessionSecretPath. Empty string ignored. Do not put production
          secrets in Nix config that lands in the public tree.
        '';
      };

      sessionTtlSecs = mkOption {
        type = types.ints.positive;
        default = 86400;
        description = "SURMOUNT_SESSION_TTL_SECS for signed session cookie Max-Age.";
      };

      publicBaseUrl = mkOption {
        type = types.str;
        default = "";
        example = "https://services.surmount.systems";
        description = ''
          SURMOUNT_PUBLIC_BASE_URL for NIP-98 u-tag matching when behind a
          reverse proxy (optional). Empty = build from request Host + scheme.
        '';
      };

      nip98MaxSkewSecs = mkOption {
        type = types.ints.positive;
        default = 300;
        description = "SURMOUNT_NIP98_MAX_SKEW_SECS: |now - created_at| window for kind 27235.";
      };

      consoleAccountsFile = mkOption {
        type = types.str;
        default = "";
        example = "/var/lib/surmount/console/accounts.json";
        description = ''
          Host path for the Surmount console account map (optional mailbox
          address, optional npub hex, role administrator or user). Empty uses
          stateDir/console/accounts.json. Emitted as SURMOUNT_CONSOLE_ACCOUNTS.
          Owner surmount-ui, mode 0600 after first write. Not the Nostr
          allowlist: User npubs stay in this map and must not be copied to
          nostrAllowlistFile. Never nsec. Never commit. Missing file is an
          empty map.
        '';
      };

      nwcStoreFile = mkOption {
        type = types.str;
        default = "";
        example = "/var/lib/surmount/secrets/ui/nwc.json";
        description = ''
          Host path for contributor Nostr Wallet Connect (NIP-47) URIs.
          Empty uses durableMaterialDir/ui/nwc.json. Emitted as
          SURMOUNT_NWC_STORE. Owner surmount-ui, mode 0600 after first write.
          Wallet connection strings only. Never nsec. Never git. Login stays
          NIP-07 / NIP-98. Missing file means no wallets connected.
        '';
      };

      # Account directory strategy (default honest empty; live Stalwart is explicit).
      directory = mkOption {
        type = types.enum [
          "unavailable"
          "mock"
          "stalwart"
        ];
        default = "unavailable";
        description = ''
          SURMOUNT_DIRECTORY. unavailable (default) = honest empty accounts API
          (no invented live mailboxes). mock = hermetic fixture only (lab/tests;
          never production default). stalwart = live management JMAP client
          (x:Account/query + get) against SURMOUNT_STALWART_URL; requires a host
          token via stalwartTokenPath (production) or lab inline token with
          allowLabInlineStalwartToken. Live directory also requires
          authMode=nostr unless allowDirectoryUnauthenticated (lab). Misconfig
          fails closed at process start (not silent skip). Live list failures
          stay empty + status-code error note (source still "stalwart"; no
          invented rows; no upstream body reflection).
        '';
      };

      stalwartTokenPath = mkOption {
        type = types.str;
        default = "";
        example = "/var/lib/surmount/secrets/ui/stalwart-api-token";
        description = ''
          Host path to a raw API token file (SURMOUNT_STALWART_TOKEN_FILE).
          First non-empty non-# line is the Bearer token for Stalwart management
          JMAP. File must be non-empty (comments alone fail closed), a regular
          file, and owner-only readable (mode not group/world, e.g. 0600;
          binary fail-closed). Readable by the UI user. Prefer durable Domain B
          under durableMaterialDir so free-443 and directory survive reboot.
          Host-only deploy secret; never commit. Empty = unset. Required when
          directory=stalwart unless lab inline token is allowed. Module adds the path to
          ReadOnlyPaths + ConditionPathExists when path-only so a missing file
          yields inactive unit (not restart thrash). Present-but-bad content
          still fails at process start under Restart=on-failure (burst capped).
        '';
      };

      stalwartTokenEnv = mkOption {
        type = types.str;
        default = "";
        description = ''
          Optional raw SURMOUNT_STALWART_TOKEN for lab only. Production must use
          stalwartTokenPath (host file). Non-empty requires
          allowLabInlineStalwartToken = true (eval fail-closed otherwise) so a
          secret is not interpolated into the Nix store via Environment=.
          Empty string ignored.
        '';
      };

      allowLabInlineStalwartToken = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Lab only: permit non-empty stalwartTokenEnv. Default false so
          production cannot silently put a Bearer token into unit Environment=
          (and thus the store). Prefer stalwartTokenPath.
        '';
      };

      allowDirectoryUnauthenticated = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Lab only: allow directory=stalwart while authMode=off (sets
          SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED=1). Default false: live
          directory requires authMode=nostr so principal inventory is not open
          on the UI bind. Binary also fail-closes the same coupling.
        '';
      };

      allowPublicAuthOff = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Lab only: allow authMode=off while the primary listen is not loopback
          (sets SURMOUNT_ALLOW_PUBLIC_AUTH_OFF=1). Default false: public primary
          bind (0.0.0.0 / non-loopback) requires authMode=nostr so the open
          console cannot sit on a public edge. Prefer loopback/private binds for
          local auth-off instead of this flag. Binary also fail-closes the same
          coupling. Never production default.
        '';
      };
    };

    # Arti onion / hidden service (REQUIRED product surface).
    # HS private keys are deploy secrets on the host only (never in git).
    # Generated arti.toml is a management-publish config (onion + rproxy).
    # One [onion_services] nickname per public HTTP Host. Mail Hosts unmapped.
    artiHiddenService = {
      enable = mkEnableOption "Arti onion/hidden service for Surmount backends";

      # Default false: enable installs config only; no multi-user daemon.
      startDaemon = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Start the arti systemd service under multi-user. Default false so
          enable=true only installs management-publish config + status oneshot.
          When true, uses the complete management-publish arti.toml (no need
          for acceptIncompleteOnionConfig). Requires: onionServiceStateDir on
          the host (ConditionPathIsDirectory), a non-null arti package, and a
          service-capable package gate: Surmount pkgs.artiOnionService
          (passthru.surmountOnionServiceCapable when package is null) or an
          explicit packageIsOnionServiceCapable = true claim for a non-Surmount
          binary. Unit active does not by itself prove an onion is published
          on the Tor network.
        '';
      };

      # No effect in this module version (kept for experimental future modes).
      acceptIncompleteOnionConfig = mkOption {
        type = types.bool;
        default = false;
        description = ''
          No effect in this module version. Kept only as a reserved flag for a
          possible future experimental incomplete-config path. The lean
          management-publish config is complete: startDaemon does not read this
          flag for assertions or generated toml. Leave false.
        '';
      };

      package = mkOption {
        type = types.nullOr types.package;
        default = null;
        description = ''
          Arti package. Null prefers pkgs.artiOnionService from the Surmount
          overlay (cargo feature onion-service-service), then falls back to
          stock pkgs.arti when the overlay package is absent. Stock nixpkgs
          arti is client-default; the Surmount package is a distinct attribute
          and does not replace pkgs.arti. Explicit package = pkgs.arti keeps
          the client binary and requires packageIsOnionServiceCapable if you
          still start the daemon (documented footgun). Live Tor publish also
          needs operator host HS keys (never in git).
        '';
      };

      # Fail-closed claim: stock channel arti is NOT service-capable.
      packageIsOnionServiceCapable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Operator claim that `package` was built with onion-service-service
          (and keymgr as needed via arti-client) so it can publish a hidden
          service. Default false. When package is null and pkgs.artiOnionService
          is present, the module treats passthru.surmountOnionServiceCapable as
          sufficient (happy path without setting this bool). Required true for
          startDaemon when using a non-Surmount binary. unit active != onion
          published without a capable binary and live Tor verify. Do not set
          true on stock client arti unless you intentionally override the gate.
        '';
      };

      nickname = mkOption {
        type = types.str;
        default = "surmount-management";
        description = ''
          Onion service nickname (Arti local name embedded in config; not the
          .onion address). Must match [A-Za-z0-9][A-Za-z0-9_-]*.
        '';
      };

      # Host path for non-secret process cache (and related stateDir layout).
      stateDir = mkOption {
        type = types.str;
        default = "/var/lib/surmount/arti";
        description = ''
          Host directory for Arti process cache (storage.cache_dir). Must not
          be in the public git tree. Owned by surmount-arti after tmpfiles
          (0750 surmount-arti:surmount-arti).
        '';
      };

      onionServiceStateDir = mkOption {
        type = types.str;
        default = "/run/surmount-secrets/arti/onion-service";
        description = ''
          Host path for onion service identity and HS instance state
          (arti storage.state_dir). One Arti keystore root for every public
          HTTP Host nickname. Deploy secrets on host only; never example
          private key material in repo. Daemon unit requires this directory
          to exist (ConditionPathIsDirectory) and to be writable by the
          surmount-arti service user (e.g. chown surmount-arti:surmount-arti,
          mode 0750 or tighter). Root-owned 0700 will pass the path condition
          then fail at runtime when opening the keystore. Module does not
          auto-create this directory.
        '';
      };

      # Backend for the default lean surface: management HTTP only (cleartext).
      backendAddress = mkOption {
        type = types.nullOr types.str;
        default = null;
        example = "127.0.0.1:8090";
        description = ''
          Local TCP host:port for onion reverse-proxy when backendUnixSocket
          is null. Null (default): when UI is https (escape off) and no
          backendUnixSocket, the module auto-points at the local cleartext
          API (managementUi.localCleartextListen or auto-derived loopback);
          otherwise derives managementUi.listenAddress:port. Explicit
          override must match host:port or [ipv6]:port. Lean path expects
          plain HTTP on this target (not TLS). Do not invent TLS-on-onion.
        '';
      };

      backendUnixSocket = mkOption {
        type = types.nullOr types.str;
        default = null;
        example = "/run/surmount/management-ui.sock";
        description = ''
          When set, onion reverse-proxy target is unix:<path> instead of
          backendAddress / derived UI TCP. Prefer TCP loopback for production
          until upstream UDS forward is fully proven (tor-hsrproxy residual).
        '';
      };

      # Lean defaults: do not onion-publish Stalwart admin/JMAP without explicit yes.
      publishStalwartAdmin = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Intent to onion-publish Stalwart admin HTTP. Default false (Q-ARTI-3
          lean). Reserved: does not yet add onion stanzas (no admin backend
          address option). Management-publish config stays lean either way.
        '';
      };

      publishStalwartJmap = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Intent to onion-publish Stalwart JMAP. Default false (Q-ARTI-3 lean).
          Reserved: does not yet add onion stanzas (no JMAP backend address
          option). Management-publish config stays lean either way.
        '';
      };
    };

    web = {
      enable = mkOption {
        type = types.bool;
        # Product default off: management-ui rustls owns public HTTPS.
        # Escape: set true for dual-run nginx + ACME while migrating.
        default = false;
        description = ''
          Enable transitional nginx + ACME virtual hosts.
          TRANSITIONAL-TO-DELETE: product edge is Axum-first (management-ui
          listenMode=https with host PEMs). Default false. Set true only as a
          dual-run escape while migrating off nginx
          (`surmount.web.enable = true`).
        '';
      };

      extraVhosts = mkOption {
        type = types.attrsOf types.attrs;
        default = { };
        description = ''
          Extra nginx virtualHost attrsets merged into services.nginx.virtualHosts.
          Legacy / migrated sites are static files only; use root
          locations, not app-server upstreams, for old Synology content.
        '';
      };

      # TODO(static-sites): structured staticSites option (hostname -> root path)
      # once content is staged; keep extraVhosts for escape hatches.
    };

    backups = {
      enable = mkEnableOption "restic backups of mail store and site data";

      repository = mkOption {
        type = types.str;
        default = "";
        example = "s3:s3.amazonaws.com/surmount-backups";
        description = "restic repository URL. Empty disables timer until set.";
      };

      passwordFile = mkOption {
        type = types.str;
        default = "";
        description = "Path to restic password file (typically a sops secret path).";
      };

      paths = mkOption {
        type = types.listOf types.path;
        default = [ ];
        description = "Extra paths to include beyond mail store defaults.";
      };
    };

    hardening = {
      enable = mkOption {
        type = types.bool;
        default = true;
        description = "Apply SSH hardening, fail2ban sketch, and sensible defaults.";
      };

      allowPasswordAuth = mkOption {
        type = types.bool;
        default = false;
        description = "Allow SSH password authentication (prefer keys only).";
      };

      qemuGuestAgent = {
        enable = mkOption {
          type = types.bool;
          default = false;
          description = ''
            Start the QEMU guest agent (services.qemuGuest). SHC ticket 261
            staff could not inject a command when SSH was stuck because
            this agent was missing. Ticket 263 staff (2026-08-25) said
            guest-agent talk is pending the next reboot acknowledgement
            and is not a silver bullet. Default off (non-QEMU hosts).
            Enable from private host-local on the mail VPS. Agents never
            reboot.
          '';
        };
      };

      eternalTerminal = {
        enable = mkOption {
          type = types.bool;
          default = true;
          description = ''
            Eternal Terminal server (etserver) for operator SSH that
            reconnects after sleep and network change. Default on with
            hardening. Listens TCP 2022 and opens that port. Uses stock
            nixpkgs services.eternal-terminal. Not niced (same class as
            sshd). Mullvad or other laptop VPNs stay operator-owned; this
            module does not replace them. Pair long commands with tmux
            on the guest. See docs/OPS.md.
          '';
        };

        port = mkOption {
          type = types.port;
          default = 2022;
          description = "etserver TCP port (stock Eternal Terminal default).";
        };
      };
    };

    # Operator paper trail: persistent journald with a size cap, sshd VERBOSE,
    # journal group for nixbuilder. Completeness and OOM/cgroup clues are
    # the priority; size vacuum is secondary so the disk cannot fill.
    # Default on with surmount.enable. Never log secrets. Scaffold sizes
    # are not published guest disk.
    logging = {
      enable = mkOption {
        type = types.bool;
        default = true;
        description = ''
          Persistent journald paper trail for host services (mail, management-ui,
          sshd, Arti, nft/fail2ban, nix-daemon, optional Vaultwarden, optional
          Lake). Default true when surmount.enable. Completeness and start-of-OOM
          clues beat aggressive rate-limit vacuum. Size-capped so logs cannot
          fill the disk (SHC ticket 261 class). Operator-only; no public log dump.
        '';
      };

      systemMaxUse = mkOption {
        type = types.str;
        default = "1G";
        description = ''
          journald SystemMaxUse (persistent /var/log/journal). Scaffold default
          1G, not a published guest disk size. Host-local may raise it without
          committing a SKU.
        '';
      };

      runtimeMaxUse = mkOption {
        type = types.str;
        default = "256M";
        description = ''
          journald RuntimeMaxUse (volatile /run/log/journal). Scaffold default
          256M, not a published guest RAM or disk size.
        '';
      };

      maxRetentionSec = mkOption {
        type = types.str;
        default = "30day";
        description = ''
          journald MaxRetentionSec. Vacuum by age in addition to size. Scaffold
          30day.
        '';
      };

      rateLimitIntervalSec = mkOption {
        type = types.str;
        default = "30s";
        description = ''
          journald RateLimitIntervalSec. Explicit so a flood cannot silently
          use an unpinned default. Combined with rateLimitBurst. Size vacuum
          (systemMaxUse) still caps disk. Do not set 0 (that disables the
          rate limit and can fill the cap with noise).
        '';
      };

      rateLimitBurst = mkOption {
        type = types.str;
        default = "50000";
        description = ''
          journald RateLimitBurst per interval. Scaffold 50000 is above the
          systemd 10000 default so mail, Axum, nix-daemon, and Lake lines
          (including the start of an OOM or cgroup kill) are less likely to
          be the first dropped during a torture-test burst. Must stay at
          least 20000; 0 disables the rate limit. Size vacuum (systemMaxUse)
          still caps disk. Not a guest SKU.
        '';
      };

      sshdLogLevel = mkOption {
        type = types.str;
        default = "VERBOSE";
        description = ''
          OpenSSH LogLevel when logging.enable. VERBOSE records auth failures
          and disconnects without passwords. Do not set DEBUG3 on a public
          host (noise and possible sensitive material).
        '';
      };

      journalReaders = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = ''
          Extra local users added to group systemd-journal so they can read
          the system journal without sudo. The remote-builder user is added
          automatically when remoteBuilder.enable (do not add nixbuilder to
          wheel). Users listed here must already exist.
        '';
      };
    };

    # ssh-ng remote builder. Default off (host-local enable). Hard MemoryMax
    # is required: niceness is not a memory cap. Optional Lake lives in
    # surmount.lake (also default off).
    remoteBuilder = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Install the nixbuilder user, niced wrappers, and hard memory
          caps for ssh-ng. Trusted ssh-ng forwards rustc to system
          nix-daemon, so MemoryMax and Nice land on nix-daemon.service
          as well as the nixbuilder user slice.
          Default false. Enable from private host-local.
          Does not set Nice= or MemoryMax on mail or other critical units.
        '';
      };

      user = mkOption {
        type = types.str;
        default = "nixbuilder";
        description = "Unprivileged ssh-ng builder user. Do not add to wheel.";
      };

      uid = mkOption {
        type = types.int;
        default = 1987;
        description = ''
          Stable uid (>= 1000, normal user) so user-<uid>.slice can carry
          MemoryMax. Not a published host SKU. Override from host-local if
          this uid is already taken.
        '';
      };

      group = mkOption {
        type = types.str;
        default = "nixbuilder";
        description = "Primary group for the builder user.";
      };

      memoryMax = mkOption {
        type = types.str;
        default = "4G";
        example = "1500M";
        description = ''
          systemd MemoryMax for nix-daemon.service (the cgroup that
          actually runs rustc), the builder slice, and ssh-ng stdio.
          Scaffold default 4G is a conservative budget, not a published guest
          RAM size. This is the builder's budget, not 95 percent of the
          whole guest: mail and OS must keep RAM. SHC 261 95 percent
          guards are a total ceiling (CPU, RAM, storage), not "give the
          builder almost all RAM." Host-local sets the real budget
          without committing that number to the public tree.
        '';
      };

      maxJobs = mkOption {
        type = types.ints.positive;
        default = 8;
        description = ''
          Parallel store jobs this builder will accept (host
          nix.settings.max-jobs). The operator laptop is local;
          surmount-1 is the remote builder. Laptop ssh-ng machines
          max-jobs must match this live guest number, not laptop inxi
          / nproc. Measure the guest with ssh surmount-1 inxi or lscpu
          before changing it. Scaffold 8 is a memory-safe default, not
          laptop cores and not a published guest SKU. Do not set this
          to a fake high advert.
        '';
      };

      buildCores = mkOption {
        type = types.nullOr types.ints.unsigned;
        default = null;
        description = ''
          If set, nix.settings.cores (threads per job). null leaves the Nix
          default (0 = auto) so one derivation can use online CPUs.
          Pair a modest maxJobs with auto cores instead of one job slot
          per online CPU.
        '';
      };

      cpuQuota = mkOption {
        type = types.str;
        default = "auto";
        description = ''
          systemd CPUQuota on the builder path. "auto" means do not apply
          a one-CPU 95 percent cap (that leaves cores idle and starves the
          ssh-ng stdio proxy). When onlineCpus is also set, the module
          applies 95 percent times that count on nix-daemon.service and
          the slices. niced-builder then uses 95 percent times nproc when
          systemd-run is available. Host-local may set an explicit
          multi-CPU quota without putting a SKU in git.
        '';
      };

      onlineCpus = mkOption {
        type = types.nullOr types.ints.positive;
        default = null;
        description = ''
          Guest online CPU count for cpuQuota=auto (95 percent times this
          number, not 95 percent of one CPU). null leaves CPUQuota unset on
          systemd units so we never pin one CPU. Measure the mail host with
          ssh surmount-1 inxi / lscpu; set this from host-local. Do not
          copy laptop nproc. Do not put the live guest count in git.
        '';
      };

      diskGuardPercent = mkOption {
        type = types.ints.between 1 100;
        default = 95;
        description = ''
          Refuse ssh-ng stdio when df used-percent on diskGuardPath
          is at least this value (SHC storage guard). Default 95.
        '';
      };

      diskGuardPath = mkOption {
        type = types.str;
        default = "/";
        description = ''
          Filesystem path measured by the niced-builder disk guard. Default
          root.
        '';
      };
    };

    # Optional Lean/Lake compute. Default off. Same class of cgroup guards
    # as the ssh-ng builder. Do not enable on the live mail host from an
    # agent. Uncapped Lake previously OOM-killed systemd.
    lake = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Install a niced, MemoryMax-capped surmount-lake unit. Default
          false. Enable only from private host-local after the generation
          that carries this unit is live. Does not set Nice= on mail,
          management-ui, sshd, Arti, or networking.
        '';
      };

      package = mkOption {
        type = types.nullOr types.package;
        default = null;
        description = ''
          Lake (Lean) package whose bin/lake is ExecStart. Required when
          enable is true. null while default-off so eval does not pull Lean.
        '';
      };

      extraArgs = mkOption {
        type = types.listOf types.str;
        default = [ "build" ];
        description = ''
          Arguments after lake -j<jobs>. Default is one build. Not a shell
          string.
        '';
      };

      workDir = mkOption {
        type = types.str;
        default = "/var/lib/surmount-lake";
        description = ''
          Working directory for the Lake unit (StateDirectory). Must be
          absolute. Not a published guest path SKU.
        '';
      };

      memoryMax = mkOption {
        type = types.str;
        default = "4G";
        example = "1500M";
        description = ''
          systemd MemoryMax for surmount-lake.service and its slice.
          Scaffold 4G is a Lake budget, not a published guest RAM size
          and not 95 percent of the whole guest. Required when enable is
          true.
        '';
      };

      jobs = mkOption {
        type = types.ints.positive;
        default = 4;
        description = ''
          Parallel Lake jobs (-j). Scaffold 4 is memory-safe, not guest
          nproc and not a published core count. Must stay under what
          memoryMax can hold. Do not set this to raw nproc.
        '';
      };

      cpuQuota = mkOption {
        type = types.str;
        default = "auto";
        description = ''
          systemd CPUQuota on the Lake unit. auto means do not apply a
          one-CPU 95 percent cap. When onlineCpus is set, apply 95 percent
          times that count.
        '';
      };

      onlineCpus = mkOption {
        type = types.nullOr types.ints.positive;
        default = null;
        description = ''
          Guest online CPU count for cpuQuota=auto. Same rule as
          remoteBuilder.onlineCpus. Measure the mail host; set from
          host-local; do not copy laptop nproc into git.
        '';
      };
    };

    # Optional Grok OSS TUI on the mail host. Default off. Enable from
    # host-local. User grok, home for ~/.grok, MemoryMax on the user
    # slice. Binary on PATH. Do not auto-start the TUI on boot.
    grokOss = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Install user grok, put grok-oss and tmux on PATH, and cap that
          user's systemd slice with MemoryMax. Default false. Enable
          only from private host-local. Does not start the TUI on boot.
          Does not set Nice=19 (sshd class, not a niced builder). tmux
          is installed here so attach does not depend only on Eternal
          Terminal.
        '';
      };

      package = mkOption {
        type = types.nullOr types.package;
        default = null;
        description = ''
          Grok OSS package whose bin/grok-oss is on PATH. Required when
          enable is true. null while default-off so eval does not pull
          the grok-oss flake package. Fail-closed without this package.
        '';
      };

      user = mkOption {
        type = types.str;
        default = "grok";
        description = "Unprivileged grok-oss owner. Home holds ~/.grok. Not root.";
      };

      uid = mkOption {
        type = types.int;
        default = 1988;
        description = ''
          Stable uid (>= 1000, normal user) so user-<uid>.slice can carry
          MemoryMax. Not a published host SKU. Override from host-local if
          this uid is already taken. Distinct from remoteBuilder.uid.
        '';
      };

      home = mkOption {
        type = types.str;
        default = "/home/grok";
        description = ''
          Home directory for user grok. Product config lives at
          $home/.grok. Must be absolute.
        '';
      };

      memoryMax = mkOption {
        type = types.str;
        default = "4G";
        example = "1500M";
        description = ''
          systemd MemoryMax for the grok user slice (tmux / grok-oss
          login). Scaffold 4G is a grok-oss budget, not a published
          guest RAM size. Host-local sets the real budget without
          committing that number to the public tree. Required when
          enable is true.
        '';
      };
    };

    # Merciless ban / whitelist first product path (Rust edge + optional nft sets).
    # Defaults lean private: off. Q-ACL-1..6 remain open (see docs/open-choices.md).
    accessControl = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Install Surmount nft set placeholders and wire management-ui ban env when
          managementUi is enabled. Default false (lean private). Does not claim
          live host bans without operator nft load + optional helper. See
          docs/research/access-control-fail2ban.md and docs/EDGE_AND_TLS.md.
        '';
      };

      # off | dry-run | enforce — matches SURMOUNT_BAN_ENFORCEMENT.
      enforcement = mkOption {
        type = types.enum [
          "off"
          "dry-run"
          "enforce"
        ];
        default = "off";
        description = ''
          Ban enforcement mode for management-ui. Default off.
          dry-run: record bans in app state without 403; does **not** mutate
          nft and does not require helper/nftBin existence.
          enforce: 403 banned clients at the Axum edge; when backend=nft and
          nftHelper (preferred) or nftExec is on, privileged apply runs
          (fail-closed if helper sock/bin or nftBin missing). UI never gets
          CAP_NET_ADMIN; host drop uses socket-activated helper unit.
        '';
      };

      backend = mkOption {
        type = types.enum [
          "memory"
          "nft"
        ];
        default = "memory";
        description = ''
          Ban backend kind. memory = app-level only (never installs or calls
          the privileged helper). nft = same app store plus optional host nft
          add-element sync when nftHelper (preferred) or nftExec is true.
          Preferred host-drop path: backend=nft + nftHelper + absolute nftBin
          + enforcement=enforce + socket-activated oneshot
          (SURMOUNT_BAN_NFT_HELPER_SOCK). nftHelper/nftExec require backend=nft
          (Nix assertion + binary fail-closed); otherwise the helper sock would
          be a silent no-op. nftExec is unsupported on the UI unit
          (NoNewPrivileges; no CAP_NET_ADMIN). Mutually exclusive: nftHelper vs
          nftExec. DryRun/Off skip capable-bin existence checks.
        '';
      };

      whitelist = mkOption {
        type = types.listOf types.str;
        default = [ ];
        example = [
          "203.0.113.10/32"
          "2001:db8::/64"
        ];
        description = ''
          Whitelist CIDRs (comma-joined into SURMOUNT_BAN_WHITELIST). Never
          banned; last-used touched on allowed requests from a match.
        '';
      };

      statePath = mkOption {
        type = types.str;
        default = "";
        description = ''
          Absolute host path for ban/whitelist-last-used JSON (SURMOUNT_BAN_STATE_PATH).
          Empty uses in-memory only. Typical: ''${config.surmount.stateDir}/ui/ban-state.json
          when management-ui owns the file. Never put secrets here.
        '';
      };

      nftSets = mkOption {
        type = types.bool;
        default = true;
        description = ''
          When accessControl.enable, define inet surmount_guard table with sets
          surmount-ban4/6 and surmount-whitelist4/6 (names match Rust constants).
          networking.nftables.enable is turned on. Sets start empty; operator
          must load ruleset and still owns live ban drop policy (Q-ACL open).
        '';
      };

      nftExec = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Enable direct in-process `nft` exec from management-ui
          (SURMOUNT_BAN_NFT_EXEC). Default false. **Unsupported for live host
          drop:** UI has NoNewPrivileges and no CAP_NET_ADMIN (and on
          privileged ports CapabilityBoundingSet is CAP_NET_BIND_SERVICE only).
          Mutually exclusive with nftHelper. Prefer nftHelper. Module warns.
        '';
      };

      nftBin = mkOption {
        type = types.str;
        default = "";
        description = ''
          Absolute path to nft binary. Required when nftHelper or nftExec is
          true under Enforce (helper oneshot env and/or direct exec). DryRun/Off
          do not require the bin at config parse.
        '';
      };

      # Product elevation: UDS -> socket-activated oneshot (not UI child setcap).
      nftHelper = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Install socket-activated surmount-nft-ban-helper@.service with
          CAP_NET_ADMIN/CAP_NET_RAW on the **helper unit only**, and wire UI
          SURMOUNT_BAN_NFT_HELPER_SOCK=/run/surmount/nft-ban-helper.sock.
          **Requires backend = "nft"** (asserted; Memory never applies). For
          live host drop also set enforcement = "enforce" and absolute nftBin.
          management-ui keeps NoNewPrivileges and never receives CAP_NET_ADMIN.
          Child setcap spawn is intentionally not used (NNP blocks elevation).
          Default false (lean). Mutually exclusive with nftExec. DryRun does
          not call the helper. Live host nft smoke still operator residual.
        '';
      };

      nftHelperBin = mkOption {
        type = types.str;
        default = "";
        description = ''
          Absolute path to surmount-nft-ban-helper binary used as ExecStart for
          the socket-activated oneshot when nftHelper is true. Empty uses
          ''${managementUi.package}/bin/surmount-nft-ban-helper. Non-empty must
          be absolute. UI env points at the UDS path, not this binary.
        '';
      };
    };

    # Domain C: human / org password vault (Vaultwarden). Not deploy-secret
    # activation feed; not Bitwarden Secrets Manager API. Sample host stays off.
    vaultwarden = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Enable Surmount-wrapped Vaultwarden (stock services.vaultwarden with
          private defaults). Default false so sample hosts and CI stay off.
          When true: SQLite, loopback Rocket, signups disabled, admin token
          from host EnvironmentFile path only (never inline secret in tree).
          Does not feed nixos-rebuild for other secrets. See docs/SECRETS.md
          section 5 and modules/vaultwarden.nix.
        '';
      };

      rocketAddress = mkOption {
        type = types.str;
        default = "127.0.0.1";
        description = ''
          Rocket bind address (ROCKET_ADDRESS). Default and forced after
          extraConfig: loopback only (127.0.0.1 or ::1) unless
          allowNonLoopbackListen = true. Private; SSH tunnel or later
          onion/edge exposure is operator work. World bind without the
          escape fails eval closed.
        '';
      };

      rocketPort = mkOption {
        type = types.port;
        default = 8222;
        description = "Rocket listen port (ROCKET_PORT). Default 8222.";
      };

      allowNonLoopbackListen = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Escape hatch: allow rocketAddress outside 127.0.0.1 / ::1 (e.g.
          0.0.0.0). Default false so accidental world bind fails at eval.
          Signups stay forced off; public exposure is still operator residual
          (tunnel / onion / deliberate design). Prefer leave false.
        '';
      };

      domain = mkOption {
        type = types.str;
        default = "";
        example = "https://services.example.test/vault";
        description = ''
          Optional Vaultwarden DOMAIN config (public or private base URL the
          clients see). Empty = unset (fine for pure loopback / tunnel).
          Not the same as managementUi.vaultwardenUrl (console link); set both
          when you publish a stable client URL. When managementUi path proxy
          is on and this is empty, vaultwarden.nix may derive
          https://{servicesHostname}/vault so Rocket DOMAIN matches the
          public subpath.
        '';
      };

      adminTokenEnvFile = mkOption {
        type = types.str;
        default = "";
        example = "/var/lib/surmount/secrets/vaultwarden/admin.env";
        description = ''
          Host path to a systemd EnvironmentFile that sets ADMIN_TOKEN=...
          (KEY=value lines; mode 0600; never in git). Required non-empty when
          enable is true (eval assertion). Unit uses ConditionPathExists so a
          missing file yields inactive (dead), not restart thrash. Example
          install via `nix run .#secrets-install-host` (domain B). Secret keys
          must not go in extraConfig (store-bound env file).
        '';
      };

      dbBackend = mkOption {
        type = types.enum [
          "sqlite"
          "mysql"
          "postgresql"
        ];
        default = "sqlite";
        description = ''
          Vaultwarden database backend. Default sqlite (recommended single-VPS;
          see docs/DATASTORES.md). Postgres only if already operating PG.
        '';
      };

      # Optional extra config keys (ROCKET_LOG, etc.). Never put secrets here.
      extraConfig = mkOption {
        type = types.attrsOf (
          types.nullOr (
            types.oneOf [
              types.bool
              types.int
              types.str
            ]
          )
        );
        default = { };
        description = ''
          Extra services.vaultwarden.config attrs (non-secret only). Eval
          refuses secret-bearing keys (ADMIN_TOKEN, SMTP_PASSWORD, DATABASE_URL,
          YUBICO_SECRET_KEY, HIBP_API_KEY, PUSH_INSTALLATION_KEY,
          SSO_CLIENT_SECRET) because stock nixpkgs writes config to a
          world-readable store EnvironmentFile. Secrets belong only in host
          environmentFile paths. SIGNUPS_ALLOWED is forced false after this
          merge; ROCKET_ADDRESS / ROCKET_PORT are forced from rocketAddress /
          rocketPort after this merge (extraConfig cannot rebind listen).
        '';
      };
    };

    # Splora (Esplora-compatible indexer) Unix-socket reverse proxy on the
    # Axum edge. Default off. Host-local instance map. Not nginx.
    sploraProxy = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          When true, management-ui reverse-proxies paths on the portal Host
          to Unix sockets /run/splora/<instance>.http.sock (HTTP/1.1).
          NIP-98 stays in splora. No edge API keys. SURMOUNT_SPLORA_PROXY=1.
          Sample host stays off. Do not enable modules/web.nix for this.
          One public Host. Not per-network esplora Hosts.
        '';
      };

      socketDir = mkOption {
        type = types.str;
        default = "/run/splora";
        description = ''
          Directory for indexer HTTP sockets (SURMOUNT_SPLORA_SOCKET_DIR).
          Default /run/splora. Instance sockets default to
          <socketDir>/<instance>.http.sock. Not the Electrum newline socket.
        '';
      };

      queueSocket = mkOption {
        type = types.str;
        default = "/run/splora/queue.sock";
        description = ''
          Queue unit Unix socket (SURMOUNT_SPLORA_QUEUE_SOCKET). POST
          {npub,email} only, on queuePath. Not an indexer unit.
        '';
      };

      queuePath = mkOption {
        type = types.str;
        default = "/splora/queue";
        description = ''
          Public path for the queue POST (SURMOUNT_SPLORA_QUEUE_PATH).
          Default /splora/queue.
        '';
      };

      instances = mkOption {
        type = types.attrsOf (
          types.submodule {
            options = {
              hosts = mkOption {
                type = types.listOf types.str;
                default = [ ];
                example = [ ];
                description = ''
                  Optional extra public Host names (lowercased at runtime).
                  Leave empty. Network paths are on portalHost, not a
                  per-network Host.
                '';
              };
              socket = mkOption {
                type = types.str;
                default = "";
                description = ''
                  Absolute HTTP Unix socket. Empty = socketDir + /<name>.http.sock.
                  Must not be an Electrum newline socket.
                '';
              };
            };
          }
        );
        default = { };
        example = {
          testnet3.socket = "/run/splora/testnet3.http.sock";
          testnet4.socket = "/run/splora/testnet4.http.sock";
          mutinynet.socket = "/run/splora/mutinynet.http.sock";
          liquid.socket = "/run/splora/liquid.http.sock";
        };
        description = ''
          Instance name -> HTTP Unix socket. Allowed names: mainnet,
          testnet3, testnet4, mutinynet, liquid. Empty hosts is normal:
          paths on portalHost select the socket. Omit mainnet while that
          indexer is off. Host-local. Emitted as SURMOUNT_SPLORA_INSTANCES
          JSON when enable.
        '';
      };

      portalHost = mkOption {
        type = types.str;
        default = "";
        example = "splora.surmount.systems";
        description = ''
          One public Host (one DNS name). Not an indexer. Empty =
          splora.<primaryDomain>. Emitted as SURMOUNT_SPLORA_PORTAL_HOST
          when sploraProxy.enable. GET / lists same-host paths
          (/api/, /testnet/, /testnet4/, /signet/, /mutinynet/, /liquid/).
          Not an explorer. The Rust explorer UI is Splora's, not this repo.
        '';
      };

      # The imported services.splora module does not set MemoryMax. Host-local
      # may cap indexer/queue units from this tree without editing splora.
      unitsMemoryMax = mkOption {
        type = types.nullOr types.str;
        default = null;
        example = "2G";
        description = ''
          When services.splora.enable, set systemd MemoryMax on splora
          indexer, queue, and popular-scripts units. Null leaves the
          imported module as-is. Host-local. Not a published guest RAM size.
        '';
      };
    };

    # One Splora indexer against remote Bitcoin JSON-RPC. Host-local single
    # knob over imported services.splora. Sample host stays off.
    sploraIndexer = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          When true, turn on imported services.splora with one indexer
          instance. Host-local JSON-RPC address plus cookie file path.
          A local bitcoind datadir is not required. Cookie bytes never
          belong here. Public sample stays false. Do not start five
          indexers. Queue-only enable is not REST.
        '';
      };

      instanceName = mkOption {
        type = types.str;
        default = "mainnet";
        description = ''
          services.splora.instances key for the one indexer process.
          Allowed: mainnet, testnet3, testnet4, mutinynet, liquid.
        '';
      };

      network = mkOption {
        type = types.enum [
          "mainnet"
          "testnet3"
          "testnet4"
          "mutinynet"
          "liquid"
        ];
        default = "mainnet";
        description = "Chain that instance indexes. Mutinynet is signet with mutinynet magic.";
      };

      jsonrpcImport = mkOption {
        type = types.bool;
        default = true;
        description = ''
          Pass --jsonrpc-import so the indexer uses JSON-RPC instead of
          local blk*.dat. Default true for this wrap (remote or
          already-running bitcoind).
        '';
      };

      daemonRpcAddr = mkOption {
        type = types.str;
        default = "";
        example = "127.0.0.1:8332";
        description = ''
          Bitcoin Core JSON-RPC host:port passed as --daemon-rpc-addr.
          Required when enable. Host-local. Not a published guest address.
        '';
      };

      cookieFile = mkOption {
        type = types.str;
        default = "";
        example = "/run/surmount-secrets/splora/rpc.cookie";
        description = ''
          Host path to the bitcoind cookie file (--cookie-file). Path
          only. Never cookie bytes. Required when enable. Strict absolute
          path charset (same as other host secrets).
        '';
      };

      daemonDir = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = ''
          Optional local bitcoind or elementsd datadir (--daemon-dir).
          Null means remote JSON-RPC: do not require /var/lib/bitcoind,
          and systemd ReadOnlyPaths must not list that missing path.
        '';
      };

      dbBlockCacheMb = mkOption {
        type = types.ints.positive;
        default = 24;
        description = ''
          RocksDB block cache MiB for the instance this wrap creates
          (--db-block-cache-mb). Matches the imported module and CLI
          default of 24.
        '';
      };

      publicHealth = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Pass --public-health so GET /blocks/tip/height can skip
          NIP-98. Empty allowlist still 401s address/tx/mempool.
        '';
      };

      extraArgs = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Extra indexer argv after --jsonrpc-import and --public-health.";
      };
    };

    swapFile = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Create a swap file. Path is host-local (disk). Default off.
          Enable without path is an eval refuse. Scram still keys off
          MemAvailable, not swap fill. vm.swappiness is 1 in scram.nix.
        '';
      };
      path = mkOption {
        type = types.str;
        default = "";
        description = "Absolute swap file path. Required when enable. Not a public SKU path.";
      };
      sizeGiB = mkOption {
        type = types.ints.positive;
        default = 256;
        description = "Swap file size in GiB. Default 256. Host-local may lower it.";
      };
    };
  };
}
