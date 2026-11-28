# Mutinynet-only Bitcoin Core. Same upstream version as pkgs.bitcoind.
# Signet proof-of-work target spacing is 30 seconds in this package only.
# SigNetParams is shared by every custom signet inside one binary, so this
# package must not replace the system bitcoind that testnet3 and testnet4 run.
# Stock signet spacing stays 10 * 60 in pkgs.bitcoind.
{
  bitcoind,
  lib,
}:
assert lib.assertMsg (
  bitcoind.version == "32.1"
) "bitcoind-mutinynet stays on Bitcoin Core 32.1 (got ${bitcoind.version})";
bitcoind.overrideAttrs (old: {
  pname = "bitcoind-mutinynet";
  patches = (old.patches or [ ]) ++ [
    ./bitcoind-mutinynet-signet-spacing.patch
  ];
  # nixpkgs bitcoind sets doCheck. Those tests lock stock signet spacing
  # and take hours. This package is the spacing change. Skip that suite.
  doCheck = false;
  meta = (old.meta or { }) // {
    description = "Bitcoin Core 32.1 with mutinynet signet target spacing of 30 seconds";
  };
})
