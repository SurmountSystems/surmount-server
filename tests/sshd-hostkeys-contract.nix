# Targeted sshd host-key contract (not the 14-minute module-eval suite).
# Impure flake pin so `just test-sshd-hostkeys-eval` can nix-eval this file.
# Named contract: when hardening enables sshd, hostKeys is exactly one
# ed25519 key at /etc/ssh/ssh_host_ed25519_key. No rsa entry.
# Never embeds host SKUs, addresses, RAM, or guest disk sizes.
let
  flake = builtins.getFlake (toString ./..);
  system = builtins.currentSystem;
  inherit (flake.inputs.nixpkgs) lib;
  evalHardening =
    extra:
    lib.nixosSystem {
      inherit system;
      modules = [
        ../modules/options.nix
        ../modules/hardening.nix
        {
          system.stateVersion = "26.05";
          networking.hostName = "sshd-hostkeys-eval";
          fileSystems."/" = {
            device = "nodev";
            fsType = "ext4";
          };
          boot.loader.grub.enable = false;
          surmount = lib.recursiveUpdate {
            enable = extra.enable or true;
            hardening.enable = extra.hardening.enable or true;
          } extra;
        }
      ];
    };
  failedAssertions = e: builtins.filter (a: !a.assertion) e.config.assertions;

  t43-sshd-hostkeys-ed25519-only-when-hardening-on =
    let
      e = evalHardening { };
      keys = e.config.services.openssh.hostKeys;
      first = builtins.head keys;
      types = map (k: k.type or "") keys;
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.hardening.enable == true;
    assert e.config.services.openssh.enable == true;
    assert e.config.services.openssh.settings.PasswordAuthentication == false;
    assert e.config.services.openssh.settings.KbdInteractiveAuthentication == false;
    assert builtins.length keys == 1;
    assert first.type == "ed25519";
    assert first.path == "/etc/ssh/ssh_host_ed25519_key";
    assert !(builtins.elem "rsa" types);
    "t43-sshd-hostkeys-ed25519-only-when-hardening-on-ok";

  # SHC 261: staff could not inject a command because the guest agent was
  # missing. Default off (non-QEMU hosts). Host-local enables it.
  t43b-qemu-guest-agent-off-by-default =
    let
      e = evalHardening { };
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.hardening.qemuGuestAgent.enable == false;
    assert e.config.services.qemuGuest.enable == false;
    "t43b-qemu-guest-agent-off-by-default-ok";

  t43c-qemu-guest-agent-on-when-enabled =
    let
      e = evalHardening { hardening.qemuGuestAgent.enable = true; };
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.hardening.qemuGuestAgent.enable == true;
    assert e.config.services.qemuGuest.enable == true;
    assert builtins.elem "multi-user.target" (
      e.config.systemd.services.qemu-guest-agent.wantedBy or [ ]
    );
    "t43c-qemu-guest-agent-on-when-enabled-ok";
in
{
  t43-sshd-hostkeys-ed25519-only-when-hardening-on = t43-sshd-hostkeys-ed25519-only-when-hardening-on;
  t43b-qemu-guest-agent-off-by-default = t43b-qemu-guest-agent-off-by-default;
  t43c-qemu-guest-agent-on-when-enabled = t43c-qemu-guest-agent-on-when-enabled;
}
