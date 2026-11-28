# gcc promotes discarded const qualifiers to errors in these packages.
# The cc-wrapper appends NIX_CFLAGS_COMPILE after the package flags,
# so -Wno-error=discarded-qualifiers overrides -Werror=discarded-qualifiers.
# Attributes: libfaketime, libtpms, krb5, elfutils, and bpftools.
# bpftools' pname is bpftools. It does not set __structuredAttrs, so the
# flag is the derivation argument NIX_CFLAGS_COMPILE, not env.
# nixpkgs grub2 does not set NIX_CFLAGS_COMPILE. grub2 does not set
# __structuredAttrs. overlays.nix sets that derivation argument once.
# Do not set grub2 here too. The same name in env and as a derivation
# argument is the overlap that fails eval.
# libfaketime and krb5 set the flag in env because __structuredAttrs
# is true. libtpms and bpftools do not. elfutils does, so its flag is
# in env.
final: prev:
let
  extra = " -Wno-error=discarded-qualifiers";
  addFlag =
    old:
    let
      prevFlag = old.env.NIX_CFLAGS_COMPILE or old.NIX_CFLAGS_COMPILE or "";
      flag = prevFlag + extra;
    in
    if old.__structuredAttrs or false then
      {
        env = (old.env or { }) // {
          NIX_CFLAGS_COMPILE = flag;
        };
      }
    else
      {
        NIX_CFLAGS_COMPILE = flag;
      };
in
{
  libfaketime = prev.libfaketime.overrideAttrs addFlag;
  libtpms = prev.libtpms.overrideAttrs addFlag;
  krb5 = prev.krb5.overrideAttrs addFlag;
  elfutils = prev.elfutils.overrideAttrs addFlag;
  bpftools = prev.bpftools.overrideAttrs addFlag;
}
