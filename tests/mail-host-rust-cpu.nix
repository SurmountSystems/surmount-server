# Eval-only contract for the mail-host Rust CPU wrapper.
# Does not build rustc, Splora, bitcoind, or any crate.
# nix eval --impure --json --file tests/mail-host-rust-cpu.nix
let
  flake = builtins.getFlake (toString ./..);
  system = builtins.currentSystem;
  inherit (flake.inputs.nixpkgs) lib;

  mkProbe =
    prev: pname: cargoArtifacts:
    prev.stdenv.mkDerivation (
      {
        inherit pname;
        version = "0";
        dontUnpack = true;
        installPhase = "mkdir -p $out";
      }
      // lib.optionalAttrs (cargoArtifacts != null) {
        inherit cargoArtifacts;
      }
    );

  evalRust =
    enable:
    lib.nixosSystem {
      inherit system;
      modules = [
        ../modules/mail-host-cpu.nix
        ../modules/mail-host-rust-cpu.nix
        (
          { ... }:
          {
            system.stateVersion = "26.05";
            networking.hostName = "mail-host-rust-cpu-eval";
            fileSystems."/" = {
              device = "nodev";
              fsType = "ext4";
            };
            boot.loader.grub.enable = false;
            surmount.mailHostCpu.enable = enable;
            nixpkgs.overlays = [
              (
                _final: prev:
                let
                  deps = mkProbe prev "surmount-management-ui-deps" null;
                  sploraDeps = mkProbe prev "splora-deps" null;
                in
                {
                  surmount-management-ui = mkProbe prev "surmount-management-ui" deps;
                  splora = mkProbe prev "splora" sploraDeps;
                  splora-http = mkProbe prev "splora-http" null;
                  bitcoind = mkProbe prev "bitcoind" null;
                  elementsd = mkProbe prev "elementsd" null;
                }
              )
            ];
          }
        )
      ];
    };

  hookOf = pkg: pkg.mailHostRustCpuHook or "";
  usesOf = pkg: pkg.mailHostRustCpuUses or "";
  depsHookOf = pkg: if pkg ? mailHostRustCpuCargoArtifacts then hookOf pkg.mailHostRustCpuCargoArtifacts else "";

  goodHook = text: lib.hasInfix "target-cpu=x86-64-v3" text && lib.hasInfix "RUSTFLAGS" text && lib.hasInfix "CARGO_ENCODED_RUSTFLAGS" text;

  on = evalRust true;
  off = evalRust false;
  onUi = on.pkgs.surmount-management-ui;
  onSplora = on.pkgs.splora;
  onHttp = on.pkgs.splora-http;
  offUi = off.pkgs.surmount-management-ui;

  t-enable-true-top-and-deps =
    assert goodHook (hookOf onUi);
    assert usesOf onUi == "RUSTC_WRAPPER";
    assert goodHook (depsHookOf onUi);
    assert depsHookOf onUi == hookOf onUi;
    assert goodHook (hookOf onSplora);
    assert goodHook (depsHookOf onSplora);
    assert goodHook (hookOf onHttp);
    assert !(on.pkgs.bitcoind ? mailHostRustCpuHook);
    assert !(on.pkgs.elementsd ? mailHostRustCpuHook);
    "t-enable-true-rust-hook-x86-64-v3";

  t-enable-false-hook-absent =
    assert hookOf offUi == "";
    assert !(offUi ? mailHostRustCpuHook);
    assert !(offUi ? mailHostRustCpuCargoArtifacts);
    assert !(off.pkgs.splora ? mailHostRustCpuHook);
    assert !(off.pkgs.splora-http ? mailHostRustCpuHook);
    assert !(off.pkgs.bitcoind ? mailHostRustCpuHook);
    "t-enable-false-rust-hook-absent";
in
{
  t-enable-true-top-and-deps = t-enable-true-top-and-deps;
  t-enable-false-hook-absent = t-enable-false-hook-absent;
}
