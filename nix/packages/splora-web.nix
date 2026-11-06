# Trunk CSR bundle of the pinned splora input, directory frontend/web.
# Trunk.toml sets target = index.html and dist = dist.
# Not wasm-pack. Not cargo-leptos.
# The mail-host management UI unit sets SURMOUNT_SPLORA_WEB_ROOT to this
# package output (flake packages.<system>.splora-web). The Axum edge
# serves $out. Do not copy dist by hand.
# wasm-bindgen in frontend/web/Cargo.lock is 0.2.128. The CLI must match.
#
# The Splora git tree still has .cargo/config.toml replace-with
# menhera-cooldown for laptop cargo. That file is omitted from this Nix
# src. If it stayed, Cargo would prefer it over a vendored directory and
# query index.crates.menhera.org during the build phase.
# Registry crates come from rustPlatform.fetchCargoVendor. cargoHash is
# the vendor-staging output hash printed by the horizon prefetch.
# The build phase passes --offline and does not run cargo update.
{
  lib,
  pkgs,
  craneLib,
  rustPlatform,
  sploraInput,
}:

let
  wasmBindgenCli =
    if pkgs ? wasm-bindgen-cli_0_2_128 then
      pkgs.wasm-bindgen-cli_0_2_128
    else
      pkgs.wasm-bindgen-cli;

  src = lib.cleanSourceWith {
    src = sploraInput;
    name = "splora-web-src";
    filter =
      path: type:
      let
        base = baseNameOf path;
        parent = baseNameOf (dirOf path);
        isMenheraCargoConfig = parent == ".cargo" && (base == "config.toml" || base == "config");
      in
      !isMenheraCargoConfig;
  };

  # Horizon prefetch of rustPlatform.fetchCargoVendor (vendor-staging).
  # got: sha256-9OEeFci65XLav7XQkoOHOYyNwy/6lccuVhXq8Zc/WQM=
  cargoHash = "sha256-9OEeFci65XLav7XQkoOHOYyNwy/6lccuVhXq8Zc/WQM=";

  cargoDeps = rustPlatform.fetchCargoVendor {
    pname = "splora-web";
    version = "0.1.0";
    inherit src;
    hash = cargoHash;
    cargoRoot = "frontend/web";
  };
in
craneLib.buildTrunkPackage {
  pname = "splora-web";
  version = "0.1.0";
  inherit src;
  # Null skips crane's own vendor. That vendor writes CARGO_HOME and
  # loses to a project replace-with. cargoDeps is the fixed-output vendor.
  cargoVendorDir = null;
  cargoExtraArgs = "--offline --locked --manifest-path frontend/web/Cargo.toml";
  trunkExtraArgs = "--offline";
  trunkExtraBuildArgs = "--locked --frozen";
  env = {
    CARGO_NET_OFFLINE = "true";
    TRUNK_OFFLINE = "true";
  };
  preConfigure = ''
    mkdir -p .cargo
    substitute ${cargoDeps}/.cargo/config.toml .cargo/config.toml \
      --subst-var-by vendor ${cargoDeps}
    cat >> .cargo/config.toml <<'EOF'

[net]
offline = true
EOF
  '';
  trunkIndexPath = "frontend/web/index.html";
  doCheck = false;
  wasm-bindgen-cli = wasmBindgenCli;
  passthru = {
    inherit cargoDeps cargoHash;
  };
  meta.description = "Splora Trunk client bundle from frontend/web. Not wasm-pack and not cargo-leptos.";
}
