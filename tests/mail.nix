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
# The testScript below stays Python because that runner is upstream NixOS.
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
    def smtp_rcpt(text):
        for line in text.splitlines():
            if line.startswith("RCPT "):
                return int(line.split()[1])
        raise AssertionError(text)

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

    accepted = server.succeed(
        "${stalwartOps}/bin/vm-lab-smtp-accept recvbox@example.test"
    )
    assert smtp_rcpt(accepted) == 250, accepted
    assert any(line == "DATA 250" for line in accepted.splitlines()), accepted

    refused = server.succeed(
        "${stalwartOps}/bin/vm-lab-smtp-accept notcreated@example.test"
    )
    refused_code = smtp_rcpt(refused)
    assert refused_code >= 500, refused
    assert not any(line.startswith("DATA ") for line in refused.splitlines()), refused
  '';
}
