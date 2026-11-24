# Mail-host glibc. nixos-26.05 packages glibc 2.42. That library does
# not provide GLIBC_2.43 or GLIBC_2.44, so the upstream Nix 2.36.0
# binary cannot load its libcurl or libsqlite.
#
# GNU C Library 2.44 is a stable release (2026-07-25).
# https://lists.gnu.org/archive/html/info-gnu/2026-07/msg00005.html
# (accessed: 2026-11-24). The tarball is glibc-2.44.tar.xz.
#
# Splora's locked nixpkgs is nixos-unstable
# 2786a2a72088ecf32aea5b3ec52099e6a7cff78f (flake.lock nixpkgs_5).
# pkgs/development/libraries/glibc/common.nix in that tree sets
# version = "2.44" and fetches mirror://gnu/glibc/glibc-2.44.tar.xz.
# The patch suffix on that expression is the 2.44 stable branch,
# not an unreleased development snapshot.
# https://github.com/NixOS/nixpkgs/blob/2786a2a72088ecf32aea5b3ec52099e6a7cff78f/pkgs/development/libraries/glibc/common.nix
# (accessed: 2026-11-24).
#
# This overlay selects that glibc. It does not move the mail-host
# channel off nixos-26.05. It does not add a Python package. The
# selected expression still lists python3Minimal for
# scripts/gen-as-const.py. That is the existing generator exception.
#
# The derivation is built in nix/glibc-2.44-stdenv-deps.nix so its
# libidn2, libunistring, and linux-headers are this package set's
# copies. The Splora nixpkgs finished glibc is not used. That
# derivation referenced the other nixpkgs' linux-headers 7.3,
# libidn2, and libunistring, which stdenv-linux allowedRequisites
# rejects.
nixpkgs244: final: prev:
import ./glibc-2.44-stdenv-deps.nix nixpkgs244 final prev
