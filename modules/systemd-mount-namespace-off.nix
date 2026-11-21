# systemd is built with -Dlibmount=enabled (nix/overlays.nix). Mount
# units are supported in that build, so mail, udev, and Stalwart keep
# their own sandbox keys. This module used to force those keys off.
# It no longer does.
{ }
