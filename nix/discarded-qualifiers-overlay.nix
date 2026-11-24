# gcc promotes discarded const qualifiers to errors in these packages.
# The cc-wrapper appends NIX_CFLAGS_COMPILE after the package flags,
# so -Wno-error=discarded-qualifiers overrides -Werror=discarded-qualifiers.
# Attributes: libfaketime, libtpms, krb5, grub2, elfutils, and bpftools.
# grub2's pname is grub. bpftools' pname is bpftools.
# The mail-host grub and bpftools overrides keep this flag.
# libfaketime and krb5 set the flag in env because __structuredAttrs
# is true. libtpms, grub2, elfutils, and bpftools do not.
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
  grub2 = prev.grub2.overrideAttrs addFlag;
  elfutils = prev.elfutils.overrideAttrs addFlag;
  bpftools = prev.bpftools.overrideAttrs addFlag;
}
