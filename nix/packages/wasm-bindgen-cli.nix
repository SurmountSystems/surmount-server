# wasm-bindgen-cli 0.2.130. The Splora web build emits schema 0.2.130.
# This nixpkgs pin ships 0.2.121 and 0.2.126, not 0.2.130.
# Same shape as nixpkgs pkgs/by-name/wa/wasm-bindgen-cli_0_2_126/package.nix.
# src hash is the unpacked fetchCrate fixed-output hash (verified against
# the previous 0.2.128 line). cargoHash is the vendor-staging got: line.
{
  lib,
  buildWasmBindgenCli,
  fetchCrate,
  rustPlatform,
}:

let
  # vendor-staging got: sha256-j81sUf1m5yXU4FJfGj7GV+hdqVCV8mRmNb0QIsMCjqw=
  cargoHash = "sha256-j81sUf1m5yXU4FJfGj7GV+hdqVCV8mRmNb0QIsMCjqw=";
in
buildWasmBindgenCli rec {
  src = fetchCrate {
    pname = "wasm-bindgen-cli";
    version = "0.2.130";
    hash = "sha256-9Og1O/6/VC3sc5hBjoIWBErmLW2UHGMsTQO+6+m/dng=";
  };

  cargoDeps = rustPlatform.fetchCargoVendor {
    inherit src;
    inherit (src) pname version;
    hash = cargoHash;
  };
}
