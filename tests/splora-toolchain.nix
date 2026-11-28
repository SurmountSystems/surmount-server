# Small eval of the Splora toolchain wiring. Does not import the mail
# host, does not read the locked Splora tree, and does not build.
#
#   nix eval --file tests/splora-toolchain.nix --raw
let
  rustc = {
    version = "from-the-set";
    name = "rustc-from-set";
  };
  cargo = {
    version = "from-the-set";
    name = "cargo-from-set";
  };
  clippy = {
    name = "clippy-from-set";
  };
  rustfmt = {
    name = "rustfmt-from-set";
  };

  pkgs = {
    inherit
      rustc
      cargo
      clippy
      rustfmt
      ;
    symlinkJoin = args: args;
  };

  hasInfix =
    needle: hay:
    let
      n = builtins.stringLength needle;
      h = builtins.stringLength hay;
      go =
        i:
        if i + n > h then
          false
        else if builtins.substring i n hay == needle then
          true
        else
          go (i + 1);
    in
    go 0;

  splitString =
    sep: text:
    let
      n = builtins.stringLength sep;
      h = builtins.stringLength text;
      find =
        i:
        if i + n > h then
          null
        else if builtins.substring i n text == sep then
          i
        else
          find (i + 1);
      go =
        start:
        let
          i = find start;
        in
        if i == null then
          [ (builtins.substring start (h - start) text) ]
        else
          [ (builtins.substring start (i - start) text) ] ++ go (i + n);
    in
    go 0;

  lib = {
    inherit splitString hasInfix;
  };

  toolchain = import ../nix/rust-toolchain.nix { inherit pkgs; };

  other = import ../nix/rust-toolchain.nix {
    pkgs = pkgs // {
      rustc = rustc // {
        version = "not-a-pinned-string";
      };
    };
  };

  wired = import ../nix/splora-toolchain.nix {
    inherit lib;
    sploraInput = null;
    system = "x86_64-linux";
    pkgsRust = pkgs;
  };

  sample = ''
    rustToolchain = pkgs.rust-bin.stable.pinned.default.override {
      someFlag = false;
    };
    craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
  '';

  patched = wired.retarget sample;

  expected = ''
    rustToolchain = pkgs.surmountRust;
    craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
  '';

  toolchainText = builtins.readFile ../nix/rust-toolchain.nix;
in
if toolchain.name != "surmount-rust" then
  throw "toolchain name is ${toolchain.name}"
else if other.name != "surmount-rust" then
  throw "a different rustc version changed the derivation name to ${other.name}"
else if toolchain.passthru.rustc != rustc then
  throw "toolchain rustc is not the compiler from the package set"
else if toolchain.passthru.cargo != cargo then
  throw "toolchain cargo is not the compiler from the package set"
else if toolchain ? override then
  throw "toolchain still has override"
else if builtins.isFunction toolchain then
  throw "toolchain is a function"
else if hasInfix "makeOverridable" toolchainText then
  throw "nix/rust-toolchain.nix still uses makeOverridable"
else if patched != expected then
  throw ("retarget produced:\n" + patched)
else if hasInfix "pkgs.rust-bin" patched then
  throw "retarget left pkgs.rust-bin"
else if wired.toolchain.name != "surmount-rust" then
  throw "splora wiring did not use surmount-rust"
else
  "surmount-rust rustc=from-the-set cargo=from-the-set no-override retargeted"
