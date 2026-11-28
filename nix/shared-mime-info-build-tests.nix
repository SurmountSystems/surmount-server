# shared-mime-info 2.5.1 option build-tests defaults to true.
# That option is the gate around the xdgmime subproject.
# The test programs are not installed. doCheck is off.
# The fetched xdgmime tree registers its override for the host machine.
# Muon does not satisfy a native lookup from that host override.
# nixpkgs mesonFlags set update-mimedb and build-spec.
# They do not set build-tests. This flag is the meson_options.txt option.
old:
(old.mesonFlags or [ ]) ++ [ "-Dbuild-tests=false" ]
