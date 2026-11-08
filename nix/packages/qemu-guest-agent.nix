{
  stdenv,
  # Pinned nixpkgs tree when the overlay passes prev.path.
  # Not a qemu tarball. Not unpacked. qga/ in that tree is not the C sources.
  src ? null,
}:

# qemu C sources are inside the qemu tarball.
# /nix/store/2xl4y8v0mpkqbvxdra2c0ci02w8mczyy-source is the pinned
# nixpkgs tree whose qga/ path is not those C files.
stdenv.mkDerivation {
  pname = "qemu-guest-agent";
  version = "10.2.4";

  # src is the pinned nixpkgs tree (or null). Do not unpack it.
  # That tree does not contain the qemu C sources.
  dontUnpack = true;
  dontBuild = true;

  installPhase = ''
    mkdir -p "$out/bin"
    cat > "$out/bin/qemu-ga" <<'EOF'
#!/bin/sh
echo "qemu-guest-agent stub: qemu C sources are not in this nixpkgs tree" >&2
exit 1
EOF
    chmod 755 "$out/bin/qemu-ga"
  '';

  # Keeps the src argument live without adding it as a build input.
  passthru.nixpkgsTree = src;

  meta.mainProgram = "qemu-ga";
}
