# wasm-bindgen-cli 0.2.128. frontend/web Cargo.lock pins wasm-bindgen
# 0.2.128. This nixpkgs pin ships 0.2.121 and 0.2.126, not 0.2.128.
# Same shape as nixpkgs pkgs/by-name/wa/wasm-bindgen-cli_0_2_126/package.nix.
# src hash and cargoHash are the fixed-output build got: lines.
{
  lib,
  buildWasmBindgenCli,
  fetchCrate,
  rustPlatform,
}:

let
  # vendor-staging got: sha256-R1Tas33Ursy8kqsxguAkG0ZhNed2n5uFTAhw1l2qlLY=
  cargoHash = "sha256-R1Tas33Ursy8kqsxguAkG0ZhNed2n5uFTAhw1l2qlLY=";
in
buildWasmBindgenCli rec {
  src = fetchCrate {
    pname = "wasm-bindgen-cli";
    version = "0.2.128";
    hash = "sha256-a7lcXJnnZkYReja+iUO7NqqrWyv3toxnUgQb8s4IS5s=";
  };

  cargoDeps = rustPlatform.fetchCargoVendor {
    inherit src;
    inherit (src) pname version;
    hash = cargoHash;
  };
}
