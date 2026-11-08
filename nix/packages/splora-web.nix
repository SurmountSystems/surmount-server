# Trunk CSR bundle of the pinned splora input, directory frontend/web.
# Trunk.toml sets target = index.html and dist = dist.
# Not wasm-pack. Not cargo-leptos.
# The mail-host management UI unit sets SURMOUNT_SPLORA_WEB_ROOT to this
# package output (flake packages.<system>.splora-web). The Axum edge
# serves $out. Do not copy dist by hand.
# wasm-bindgen in frontend/web/Cargo.lock is 0.2.128. The CLI must match.
# That lock is in the Splora checkout. Crane's deps dummy keeps only the
# repo-root Cargo.lock, so the web lock is copied back into the dummy.
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
  # nixpkgs lld (same store path as llvmPackages.lld). wasm32 rustc
  # execs the binary name `lld`. Not Python, GHC, or ShellCheck.
  lld,
}:

let
  # wasm-bindgen-cli 0.2.128 from ./wasm-bindgen-cli.nix only.
  wasmBindgenCli = pkgs.callPackage ./wasm-bindgen-cli.nix { };

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
  # Trunk 0.21+ (this pin is nixpkgs trunk 0.21.14) gives --offline,
  # --release, --locked, and --frozen an optional value. A bare --offline
  # consumes the next token, so crane's
  # `trunk --offline build --release=true ...` parses `build` as that value
  # and rejects --release before any subcommand. --offline=true leaves
  # `build` as the subcommand. Crane then appends --release=true after
  # `build` when trunk.version is at least 0.21. --locked=true and
  # --frozen=true keep those flags from consuming the index path.
  trunkExtraArgs = "--offline=true";
  trunkExtraBuildArgs = "--locked=true --frozen=true";
  # frontend/web is its own workspace. Copy its lock into the deps dummy.
  extraDummyScript = ''
    mkdir -p "$out/frontend/web"
    cp ${src}/frontend/web/Cargo.lock "$out/frontend/web/Cargo.lock"
  '';
  env = {
    CARGO_NET_OFFLINE = "true";
    TRUNK_OFFLINE = "true";
  };
  # Trunk 0.21.14 config::load(None) calls Source::find on the unpack
  # root (crane's cwd). Find prefers Trunk.toml, then Cargo.toml
  # (trunk src/config/models/source/mod.rs). The root Cargo.toml is
  # package splora and has no [workspace]. Keep that file. Do not
  # rename it. Write Trunk.toml at the unpack root so find loads a
  # file and does not select package splora. dist is frontend/web/dist,
  # which is the directory the trunk install copies. frontend/web
  # already has [workspace], so this cargo check does not walk up to
  # package splora. If that table is missing, write an empty one.
  # Drop the dev-dependency `splora = { path = "../.." }`. Leave
  # splora-api and splora-frontend-shared. Do not hand-edit
  # Cargo.lock. After the manifest no longer names the indexer,
  # regenerate the unpacked web lock with cargo offline and without
  # --locked, using the vendor dir from cargoDeps. Do not delete
  # tempfile. The indexer packages keep the upstream root Cargo.toml.
  preConfigure = ''
    grep -vxF \
      -e 'splora = { path = "../.." }' \
      -e '# Indexer library name is `electrs`. Package name is `splora`.' \
      frontend/web/Cargo.toml > frontend/web/Cargo.toml.no-indexer-dev
    mv frontend/web/Cargo.toml.no-indexer-dev frontend/web/Cargo.toml
    if grep -F 'splora = { path = "../.." }' frontend/web/Cargo.toml; then
      echo "frontend/web still depends on the indexer package splora" >&2
      exit 1
    fi
    if ! grep -qxF '[workspace]' frontend/web/Cargo.toml; then
      {
        printf '%s\n' '[workspace]'
        echo
        cat frontend/web/Cargo.toml
      } > frontend/web/Cargo.toml.workspace-root
      mv frontend/web/Cargo.toml.workspace-root frontend/web/Cargo.toml
    fi
    cat > Trunk.toml <<'EOF'
[build]
target = "frontend/web/index.html"
dist = "frontend/web/dist"
EOF
    mkdir -p .cargo
    substitute ${cargoDeps}/.cargo/config.toml .cargo/config.toml \
      --subst-var-by vendor ${cargoDeps}
    cat >> .cargo/config.toml <<'EOF'

[net]
offline = true
EOF
    cargo generate-lockfile --offline --manifest-path frontend/web/Cargo.toml
  '';
  trunkIndexPath = "frontend/web/index.html";
  doCheck = false;
  # buildTrunkPackage forwards this attrset to buildDepsOnly. That
  # derivation is splora-web-deps (the wasm-streams link). The trunk
  # derivation appends binaryen, trunk, and wasm-bindgen-cli onto this
  # list; it does not replace it.
  nativeBuildInputs = [
    lld
  ];
  wasm-bindgen-cli = wasmBindgenCli;
  passthru = {
    inherit cargoDeps cargoHash;
  };
  meta.description = "Splora Trunk client bundle from frontend/web. Not wasm-pack and not cargo-leptos.";
}
