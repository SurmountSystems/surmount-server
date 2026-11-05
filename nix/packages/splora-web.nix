# Trunk CSR bundle of the pinned splora input, directory frontend/web.
# Trunk.toml sets target = index.html and dist = dist.
# Not wasm-pack. Not cargo-leptos.
# The mail-host management UI unit sets SURMOUNT_SPLORA_WEB_ROOT to this
# package output (flake packages.<system>.splora-web). The Axum edge
# serves $out. Do not copy dist by hand.
# wasm-bindgen in frontend/web/Cargo.lock is 0.2.128. The CLI must match.
{
  pkgs,
  craneLib,
  sploraInput,
}:

let
  wasmBindgenCli =
    if pkgs ? wasm-bindgen-cli_0_2_128 then
      pkgs.wasm-bindgen-cli_0_2_128
    else
      pkgs.wasm-bindgen-cli;
in
craneLib.buildTrunkPackage {
  pname = "splora-web";
  version = "0.1.0";
  src = sploraInput;
  cargoExtraArgs = "--locked --manifest-path frontend/web/Cargo.toml";
  cargoVendorDir = craneLib.vendorCargoDeps {
    cargoLock = "${sploraInput}/frontend/web/Cargo.lock";
  };
  trunkIndexPath = "frontend/web/index.html";
  doCheck = false;
  wasm-bindgen-cli = wasmBindgenCli;
  meta.description = "Splora Trunk client bundle from frontend/web. Not wasm-pack and not cargo-leptos.";
}
