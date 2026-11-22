# Surmount-controlled Stalwart Mail and Collaboration Server package.
#
# Version 1.1.0 is the upstream source tarball, not the release binary.
# A local patch restores numeric_value_merge for column families n, y, and u.
# The release binary cannot take that Rust patch.
#
# RocksDB: crate-bundled. Cargo.lock replaces librocksdb-sys with
# git+https://github.com/stalwartlabs/rust-rocksdb?branch=stalwart/0.25
# rev f6169ca36d74c15637f8c6396c34e82aa73d0d47 (crate version
# 0.19.0+11.8.1). That crate compiles the stalwartlabs/rocksdb submodule
# d2ea7067099d57c5c85bd076f2cc1ca8fb4c5d04. Cargo vendor drops git
# submodules, so this expression unpacks those two upstream tarballs into
# the vendored crate. It does not link pkgs.rocksdb. nixpkgs rocksdb at
# this pin is 10.10.1, which is not that 11.8.1 tree. The 1.1.0 package
# has no switch onto pkgs.rocksdb. Do not add a C program to invent one.
#
# Bump procedure:
#   1. Set version = "X.Y.Z" to match GitHub tag vX.Y.Z on stalwartlabs/stalwart.
#   2. Prefetch the source tarball hash and cargoHash.
#   3. Re-read the rocksdb main.rs layout before reusing the merge patch.
#   4. Bump webui / spam-filter FODs if upstream release notes require it.

{
  lib,
  # Flake passes mkArtiRustPlatform plus the same nixpkgs-rust rustc
  # and cargo. Locked rev 981724d1 rustc.version is 1.100.0.
  # makeRustPlatform does not expose .rustc on the scope.
  # Do not leave these formals to callPackage: that would inject
  # mail-host rustc 1.95.0. mail-parser 1.0.1 requires 1.98.
  rustPlatform,
  rustc,
  cargo,
  fetchurl,
  pkg-config,
  cmake,
  perl,
  openssl,
  zstd,
  callPackage,
}:

let
  # Checked 2026-11-16: GitHub releases/latest is v1.1.0
  # (published 2026-11-09T13:23:08Z, not a prerelease).
  version = "1.1.0";

  # stalwartlabs/rust-rocksdb librocksdb-sys submodule gitlinks at
  # f6169ca36d74c15637f8c6396c34e82aa73d0d47. Not pkgs.rocksdb.
  rocksdbSrc = fetchurl {
    url = "https://github.com/stalwartlabs/rocksdb/archive/d2ea7067099d57c5c85bd076f2cc1ca8fb4c5d04.tar.gz";
    hash = "sha256-1CFXitFp9t0hgsACNFWn5IRyoUZpBuYT0MF0jpcjt3c=";
  };
  snappySrc = fetchurl {
    url = "https://github.com/google/snappy/archive/23b3286820105438c5dbb9bc22f1bb85c5812c8a.tar.gz";
    hash = "sha256-rsi/LEdo2OpVL/eQwbjVmrE333UV9u9Q94UTpLlFJyw=";
  };
in
assert lib.assertMsg (lib.versionAtLeast rustc.version "1.98")
  "stalwart-mail: rustc ${rustc.version} is below mail-parser 1.0.1 (needs 1.98). Pass nixpkgs-rust rustPlatform, not mail-host rustc 1.95.0.";
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "stalwart-mail";
  inherit version;

  src = fetchurl {
    url = "https://github.com/stalwartlabs/stalwart/archive/refs/tags/v${finalAttrs.version}.tar.gz";
    hash = "sha256-09jg+Ssl+/m/kaJVzbrw5RXEOPgJxDGtjmHgtl0XtwA=";
  };
  sourceRoot = "stalwart-${finalAttrs.version}";

  patches = [
    ../patches/stalwart-1.1.0-numeric-value-merge.patch
  ];

  # Vendor hash is independent of the Rust patch (Cargo.lock is unchanged).
  # surmount-1 vendor fixed-output got this hash for the 1.1.0 lock.
  cargoHash = "sha256-pbEwTTQm3NpsF8QKz6Q7TBOLC2Pl94HUyioYdTHYBL8=";

  buildAndTestSubdir = "crates/main";

  # Upstream integration tests need live databases. This derivation is the server binary.
  doCheck = false;

  nativeBuildInputs = [
    rustc
    cargo
    pkg-config
    cmake
    perl
    rustPlatform.bindgenHook
  ];

  buildInputs = [
    openssl
    zstd
  ];

  env = {
    CARGO_BUILD_JOBS = "32";
    NIX_BUILD_CORES = "32";
    MAKEFLAGS = "-j32";
    OPENSSL_NO_VENDOR = "1";
    ZSTD_SYS_USE_PKG_CONFIG = "1";
  };

  # cargoSetupPostUnpackHook copies the vendor dir and sets cargoDepsCopy
  # before this postPatch. cargoSetupPostPatchHook clears it afterwards.
  postPatch = ''
    if [ -z "''${cargoDepsCopy:-}" ]; then
      echo "stalwart-mail: cargoDepsCopy is unset; bundled RocksDB sources were not placed" >&2
      exit 1
    fi
    rocksdb_unpacked=$(mktemp -d)
    snappy_unpacked=$(mktemp -d)
    tar -xzf ${rocksdbSrc} -C "$rocksdb_unpacked"
    tar -xzf ${snappySrc} -C "$snappy_unpacked"
    rocksdb_top=$(echo "$rocksdb_unpacked"/*)
    snappy_top=$(echo "$snappy_unpacked"/*)
    found=0
    # fetchCargoVendor puts git crates in source-git-N/<name>-<version>
    # with .cargo-checksum.json. Not a crates.io dir named librocksdb-sys.
    for d in \
      "$cargoDepsCopy"/librocksdb-sys \
      "$cargoDepsCopy"/librocksdb-sys-* \
      "$cargoDepsCopy"/source-git-*/librocksdb-sys \
      "$cargoDepsCopy"/source-git-*/librocksdb-sys-*
    do
      if [ ! -d "$d" ] || [ ! -f "$d/.cargo-checksum.json" ]; then
        continue
      fi
      if [ ! -f "$d/Cargo.toml" ]; then
        continue
      fi
      if ! grep -E -q '^[[:space:]]*name[[:space:]]*=[[:space:]]*"librocksdb-sys"[[:space:]]*$' "$d/Cargo.toml"; then
        continue
      fi
      found=$((found + 1))
      chmod -R u+w "$d"
      rm -rf "$d/rocksdb" "$d/snappy"
      mkdir -p "$d/rocksdb" "$d/snappy"
      cp -a "$rocksdb_top"/. "$d/rocksdb/"
      cp -a "$snappy_top"/. "$d/snappy/"
      test -f "$d/rocksdb/AUTHORS"
      test -f "$d/snappy/snappy.cc"
    done
    if [ "$found" -lt 1 ]; then
      echo "stalwart-mail: librocksdb-sys not found under $cargoDepsCopy" >&2
      ls -1 "$cargoDepsCopy" >&2
      for g in "$cargoDepsCopy"/source-git-*; do
        if [ -d "$g" ]; then
          echo "stalwart-mail: $g" >&2
          ls -1 "$g" >&2
        fi
      done
      exit 1
    fi
  '';

  postInstall = ''
    ln -s stalwart $out/bin/stalwart-mail
    mkdir -p $out/etc/stalwart
  '';

  passthru = {
    webui = callPackage ./stalwart-webui.nix { };
    spam-filter = callPackage ./stalwart-spam-filter.nix { };
    webadmin = callPackage ./stalwart-webui.nix { };
    packagingMode = "source-tarball";
    # nixpkgs-rust rustc.version. Not mail-host pkgs.rustc.
    rustcVersion = rustc.version;
    # Not pkgs.rocksdb. The crate compiles stalwartlabs/rocksdb d2ea706 (11.8.1).
    rocksdb = "crate-bundled-librocksdb-sys-11.8.1";
    updateScript = null;
  };

  meta = {
    description = "Secure, modern, all-in-one mail and collaboration server (Surmount pin, 1.1.0 source)";
    homepage = "https://github.com/stalwartlabs/stalwart";
    changelog = "https://github.com/stalwartlabs/stalwart/blob/v${finalAttrs.version}/CHANGELOG.md";
    license = lib.licenses.agpl3Only;
    mainProgram = "stalwart";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    sourceProvenance = [ lib.sourceTypes.fromSource ];
  };
})
