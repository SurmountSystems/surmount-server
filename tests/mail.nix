# NixOS VM smoke: Stalwart and the management UI come up, health and the
# config domain list still answer, then one mailbox is created the way the
# services console does (POST /api/v1/accounts). SMTP on port 25 must accept
# that recipient (RCPT 250 and DATA 250) and refuse a local-part that was
# not created (RCPT 5xx, no DATA).
#
# This is SMTP acceptance for that recipient. It is not an IMAP read, and
# it is not proof a later queue runner kept the message. The guest generates
# the recovery pin and the API token at boot. Those values are not in this
# file. Domain and permanent admin CLI setup is not the mailbox create.
#
# Guest directory setup and the one SMTP dialogue are Rust programs in
# surmount-stalwart-ops (vm-lab-directory-setup and vm-lab-smtp-accept).
# testScript is the upstream NixOS Python runner. The RCPT code check is
# /bin/sh in that script, not a Python function.
#
# Invoke later via: nix build .#checks.x86_64-linux.mail-vm-test
# This file does not run that build.

{
  pkgs,
  # Passed by flake.nix. Unused in this expression.
  lib,
  nixosTest,
  surmountModules,
  managementUi,
}:
let
  # Prefer testers.runNixOSTest on newer nixpkgs; fall back to nixosTest.
  runTest = if pkgs.testers ? runNixOSTest then pkgs.testers.runNixOSTest else nixosTest;
  createBody = pkgs.writeText "mail-vm-create-account.json" ''
    {"name":"recvbox","description":"vm receive proof","role":"user"}
  '';
  # Curl -w token. Defined outside the test script on purpose.
  curlStatus = "%{http_code}";
  stalwartOps = pkgs.surmount-stalwart-ops;
  tokenPath = "/var/lib/surmount/secrets/ui/stalwart-api-token";
  recoveryPath = "/var/lib/surmount/secrets/stalwart/recovery.env";
in
runTest {
  name = "surmount-mail-ui-smtp-accept";

  nodes.server =
    { pkgs, ... }:
    {
      imports = surmountModules;

      # VM: no real ACME; disable public TLS dance.
      surmount = {
        enable = true;
        primaryDomain = "example.test";
        mailHostname = "mail.example.test";
        servicesHostname = "services.example.test";
        acmeEmail = "admin@example.test";
        managementUi = {
          enable = true;
          package = managementUi;
          listenAddress = "127.0.0.1";
          # Distinct from Stalwart 0.16 default HTTP :8080.
          port = 8090;
          # Lab VM only. The module already has this switch. authMode stays
          # off on loopback. No session secret and no token string in Nix.
          directory = "stalwart";
          allowDirectoryUnauthenticated = true;
          stalwartTokenPath = tokenPath;
        };
        web.enable = false; # skip ACME in VM
        hardening.enable = false;
        backups.enable = false;
      };

      # Speed: no bloated boot
      services.openssh.enable = true;

      environment.systemPackages = [
        pkgs.curl
        pkgs.stalwart-cli
        stalwartOps
      ];

      # Pin recovery admin before the first Stalwart start. The password is
      # generated in the guest. It is not written into the Nix store.
      # Do not set STALWART_RECOVERY_MODE: that disables the MTA.
      systemd.services.surmount-vm-lab-recovery-pin = {
        description = "Generate lab VM Stalwart recovery pin";
        requiredBy = [ "stalwart-mail.service" ];
        before = [ "stalwart-mail.service" ];
        after = [ "local-fs.target" ];
        path = [ pkgs.coreutils ];
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
        };
        script = ''
          set -eu
          d=/var/lib/surmount/secrets/stalwart
          mkdir -p "$d"
          f=${recoveryPath}
          if [ -s "$f" ]; then
            exit 0
          fi
          hex=$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n')
          len=$(printf %s "$hex" | wc -c)
          if [ "$len" -lt 32 ]; then
            echo "recovery pin generation failed" >&2
            exit 1
          fi
          umask 077
          printf 'STALWART_RECOVERY_ADMIN=admin:%s\n' "$hex" > "$f"
          chmod 0600 "$f"
        '';
      };

      # Started only when first-boot HTTP is on ::1 and not on 127.0.0.1.
      # The UI module always calls http://127.0.0.1:8080.
      systemd.services.surmount-vm-lab-http-forward = {
        description = "Forward Stalwart management HTTP onto 127.0.0.1:8080";
        after = [ "stalwart-mail.service" ];
        serviceConfig = {
          Type = "simple";
          ExecStart = "${pkgs.socat}/bin/socat TCP-LISTEN:8080,bind=127.0.0.1,reuseaddr,fork TCP6:[::1]:8080";
        };
      };

      # Required by some modules even if unused
      system.stateVersion = "26.05";
    };

  testScript = ''
    server.start()
    server.wait_for_unit("stalwart-mail.service")
    server.wait_for_unit("surmount-vm-lab-recovery-pin.service")
    server.wait_for_open_port(25)
    # Stalwart 0.16 first-boot default HTTP management.
    server.wait_for_open_port(8080)
    server.succeed("test -s ${recoveryPath}")
    server.succeed("test $(stat -c %a ${recoveryPath}) = 600")

    prepared = server.succeed(
        "PATH=/run/current-system/sw/bin:$PATH ${stalwartOps}/bin/vm-lab-directory-setup"
    )
    assert "domain-ready" in prepared, prepared
    assert "token-ready" in prepared, prepared
    server.succeed("test -s ${tokenPath}")
    server.succeed("test $(stat -c %a ${tokenPath}) = 600")

    server.succeed("systemctl reset-failed surmount-management-ui.service || true")
    server.succeed("systemctl start surmount-management-ui.service")
    server.wait_for_unit("surmount-management-ui.service")
    # Surmount management UI (default 8090)
    server.wait_for_open_port(8090)

    out = server.succeed("curl -fsS http://127.0.0.1:8090/health")
    assert "ok" in out, out
    out = server.succeed("curl -fsS http://127.0.0.1:8090/api/v1/domains")
    assert "example.test" in out, out

    # Same route as the services console. No password field: a literal
    # password would be a secret in git, and inbound SMTP does not use it.
    request = server.succeed("cat ${createBody}")
    assert '"name":"recvbox"' in request, request
    assert "password" not in request.lower(), request
    created = server.succeed(
        "curl -sS -H content-type:application/json --data-binary @${createBody} "
        + "-w '\\nHTTP_CODE:${curlStatus}\\n' http://127.0.0.1:8090/api/v1/accounts"
    )
    assert "HTTP_CODE:200" in created, created
    assert "recvbox@example.test" in created, created
    assert '"ok":true' in created or '"ok": true' in created, created
    assert '"created":true' in created or '"created": true' in created, created
    assert '"source":"stalwart"' in created or '"source": "stalwart"' in created, created

    # Same acceptance and refusal as the previous RCPT asserts.
    # accept: first RCPT code is 250, and one line is exactly DATA 250.
    # refuse: first RCPT code is 500 or higher, and no line starts with DATA.
    server.succeed(
        r"""
        set -eu
        transcript=/tmp/surmount-vm-lab-smtp-transcript
        cr=$(printf '\r')

        smtp_fail() {
          printf '%s\n' "$1"
          if [ -f "$transcript" ]; then
            cat "$transcript"
          fi
          rm -f "$transcript"
          exit 1
        }

        rcpt_status_check() {
          mode=$1
          recipient=$2
          if [ "$mode" != accept ] && [ "$mode" != refuse ]; then
            smtp_fail "smtp check: unknown mode"
          fi
          if ! ${stalwartOps}/bin/vm-lab-smtp-accept "$recipient" >"$transcript"; then
            smtp_fail "smtp $mode $recipient: vm-lab-smtp-accept failed"
          fi
          rcpt_code=
          data_exact=0
          data_prefix=0
          while IFS= read -r line || [ -n "$line" ]; do
            line=''${line%"$cr"}
            case "$line" in
              "DATA 250")
                data_exact=1
                ;;
            esac
            case "$line" in
              "DATA "*)
                data_prefix=1
                ;;
            esac
            case "$line" in
              "RCPT "*)
                if [ -z "$rcpt_code" ]; then
                  set -f
                  set -- $line
                  field_count=$#
                  if [ "$field_count" -ge 2 ]; then
                    rcpt_code=$2
                  else
                    rcpt_code=x
                  fi
                  set +f
                fi
                ;;
            esac
          done <"$transcript"
          case "$rcpt_code" in
            ""|*[!0-9]*)
              smtp_fail "smtp $mode $recipient: no numeric RCPT code"
              ;;
          esac
          if [ "$mode" = accept ]; then
            if [ "$rcpt_code" -ne 250 ] || [ "$data_exact" -ne 1 ]; then
              smtp_fail "smtp accept $recipient: wanted RCPT 250 and a line DATA 250, got RCPT $rcpt_code"
            fi
          else
            if [ "$rcpt_code" -lt 500 ] || [ "$data_prefix" -ne 0 ]; then
              smtp_fail "smtp refuse $recipient: wanted RCPT 500 or higher and no DATA line, got RCPT $rcpt_code"
            fi
          fi
          rm -f "$transcript"
          printf 'smtp %s %s RCPT %s\n' "$mode" "$recipient" "$rcpt_code"
        }

        rcpt_status_check accept "recvbox@example.test"
        rcpt_status_check refuse "notcreated@example.test"
        """
    )
  '';
}
