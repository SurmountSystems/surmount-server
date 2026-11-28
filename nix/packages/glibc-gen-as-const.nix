# Rust replacement for glibc 2.44 scripts/gen-as-const.py.
#
# Toolchain pin, quoted from flake.lock nixpkgs-rust (do not invent
# another compiler):
#   rev 981724d1e07192596b94a2dcd4bd3c30139cc508
#   narHash sha256-gjAmGk/n8OS7bAvGqUfTC7YTN8HgkfQGzgpjAR77k84=
# nix/rust-toolchain.nix asserts that this package set's rustc.version
# is 1.100.0. Mail-host pkgs.rustc is 1.95.0. This file imports the
# pinned set itself so glibc's native build does not depend on the
# glibc derivation that is being built.
#
# The default checkPhase is cargoCheckHook (`cargo test`). doCheck
# stays true. fake-cc is a test double and is removed from $out.
#
# makerulesEdit is the postPatch fragment both glibc overlays append.
# It does not contain a Python program or a C program.
{ system }:
let
  nixpkgsRust = fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/981724d1e07192596b94a2dcd4bd3c30139cc508.tar.gz";
    sha256 = "sha256-gjAmGk/n8OS7bAvGqUfTC7YTN8HgkfQGzgpjAR77k84=";
  };
  pkgsRust = import nixpkgsRust {
    inherit system;
    config = { };
    overlays = [ ];
  };
  # Forces the rustc 1.100.0 assert. Unused bindings are lazy.
  asserted = import ../rust-toolchain.nix { pkgs = pkgsRust; };
  lib = pkgsRust.lib;
  crate = ../../crates/surmount-glibc-gen-as-const;
  pkg = pkgsRust.rustPlatform.buildRustPackage {
    pname = "surmount-glibc-gen-as-const";
    version = "0.1.0";
    src = lib.cleanSource crate;
    cargoLock.lockFile = crate + "/Cargo.lock";
    strictDeps = true;
    doCheck = true;
    # On PATH during the build so the 1.100.0 assert is an input.
    # rustPlatform already runs cargo and rustc from this same set.
    nativeBuildInputs = [ asserted ];
    postInstall = ''
      rm -f "$out/bin/fake-cc"
    '';
    meta = {
      description = "glibc 2.44 gen-as-const replacement (rustc ${asserted.name})";
      mainProgram = "surmount-glibc-gen-as-const";
      license = lib.licenses.unlicense;
      platforms = lib.platforms.linux;
    };
  };
  bin = "${pkg}/bin/surmount-glibc-gen-as-const";
in
pkg
// {
  # Appended to glibc postPatch. The three Makerules recipes are the
  # only $(PYTHON) calls on scripts/gen-as-const.py in glibc 2.44.
  makerulesEdit = ''
    substituteInPlace Makerules \
      --replace-fail '$(PYTHON) $(py-const-script)' '${bin}' \
      --replace-fail '$(PYTHON) $< --cc=' '${bin} --cc=' \
      --replace-fail '$(PYTHON) $< --test' '${bin} --test'
    if grep -F '$(PYTHON) $(py-const-script)' Makerules >/dev/null; then
      echo "Makerules still runs python on py-const-script" >&2
      exit 1
    fi
    if grep -F '$(PYTHON) $< --cc=' Makerules >/dev/null; then
      echo "Makerules still runs python on the gen-as-const header recipe" >&2
      exit 1
    fi
    if grep -F '$(PYTHON) $< --test' Makerules >/dev/null; then
      echo "Makerules still runs python on the gen-as-const test recipe" >&2
      exit 1
    fi
  '';
}
