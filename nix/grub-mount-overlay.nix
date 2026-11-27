# grub 2.14 util/grub-mount.c includes <fuse.h>.
# nixpkgs passes --enable-grub-mount and buildInputs fuse3.
# fuse3 installs include/fuse3/fuse.h, not include/fuse.h.
# configure.ac AC_ARG_ENABLE([grub-mount]) treats
# --disable-grub-mount as explicitly disabled.
# AM_CONDITIONAL COND_GRUB_MOUNT then skips util/grub-mount.c.
# The mail host does not need grub-mount. Do not add a header.
# configure.ac AC_ARG_ENABLE([werror]) accepts --disable-werror.
# That switch does not downgrade the C99 -Wint-conversion default error.
old:
let
  tried = builtins.tryEval (old.configureFlags or [ ]);
  flags = if tried.success && builtins.isList tried.value then tried.value else [ ];
  kept = builtins.filter (flag: flag != "--enable-grub-mount") flags;
in
{
  configureFlags = kept ++ [
    "--disable-grub-mount"
    "--disable-werror"
  ];
}
