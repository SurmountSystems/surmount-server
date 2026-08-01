# NixOS VM test sketch: Stalwart + management UI come up; health endpoint works.
# This is intentionally lighter than upstream's full SMTP/IMAP test so flake
# check stays practical. Expand once TLS/ACME secrets are test-friendly.
#
# Invoke via: nix build .#checks.x86_64-linux.mail-vm-test
# Or flake check (may take a while on first run).

{
  pkgs,
  lib,
  nixosTest,
  surmountModules,
  managementUi,
}:
let
  # Prefer testers.runNixOSTest on newer nixpkgs; fall back to nixosTest.
  runTest = if pkgs.testers ? runNixOSTest then pkgs.testers.runNixOSTest else nixosTest;
in
runTest {
  name = "surmount-mail-smoke";

  nodes.server =
    {
      config,
      pkgs,
      ...
    }:
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
        };
        web.enable = false; # skip ACME in VM
        hardening.enable = false;
        backups.enable = false;
      };

      # Speed: no bloated boot
      services.openssh.enable = true;

      environment.systemPackages = [
        pkgs.curl
      ];

      # Required by some modules even if unused
      system.stateVersion = "25.05";
    };

  testScript = ''
    server.start()
    server.wait_for_unit("stalwart-mail.service")
    server.wait_for_unit("surmount-management-ui.service")
    server.wait_for_open_port(25)
    # Surmount management UI (default 8090)
    server.wait_for_open_port(8090)
    # Stalwart 0.16 first-boot default HTTP management
    server.wait_for_open_port(8080)

    out = server.succeed("curl -fsS http://127.0.0.1:8090/health")
    assert "ok" in out, out
    out = server.succeed("curl -fsS http://127.0.0.1:8090/api/v1/domains")
    assert "example.test" in out, out
  '';
}
