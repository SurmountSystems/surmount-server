# Mail-host Cargo builds: -C target-cpu=x86-64-v3 on every rustc Cargo
# runs, including dependency crates and proc-macros. Gated on
# surmount.mailHostCpu.enable (no second option). enable false installs
# nothing, so CI and mkPkgsBare stay generic.
#
# Crane buildDepsOnly is a separate derivation (cargoArtifacts). A flag
# on the top-level package does not reach that derivation. Stamp both.
#
# Duplicate -C target-cpu is a rustc hard error. The wrapper skips when
# argv or RUSTFLAGS already contains target-cpu= (the other module may
# already set CARGO_BUILD_RUSTFLAGS on the same package).
#
# Does not rebuild rustc. RUSTC_WRAPPER calls the rustc already on the
# derivation (nixpkgs-rust, or Splora's rust-bin).

{
  config,
  lib,
  ...
}:
let
  cfg = config.surmount.mailHostCpu;
  # C and C++. MARCH for those stays in mail-host-cpu.nix.
  cppNames = [
    "bitcoind"
    "elementsd"
  ];
  # splora-http is craneLib.buildPackage in the Splora flake. This
  # repo's package overlay does not always expose the attr.
  rustNames = lib.unique (lib.subtractLists cppNames cfg.tunedPackages ++ [ "splora-http" ]);

  hookText = ''
    real=$1
    if [ -z "$real" ]; then
      echo "mail-host rustc wrapper: missing rustc path" >&2
      exit 1
    fi
    shift
    need=1
    for arg in "$@"; do
      case "$arg" in
        *target-cpu=*) need=0 ;;
      esac
    done
    case "''${RUSTFLAGS-}" in
      *target-cpu=*) need=0 ;;
    esac
    case "''${CARGO_ENCODED_RUSTFLAGS-}" in
      *target-cpu=*) need=0 ;;
    esac
    if [ "$need" -eq 1 ]; then
      set -- -C "target-cpu=x86-64-v3" "$@"
    fi
    if [ -n "''${SURMOUNT_RUSTC_WRAPPER_NEXT-}" ]; then
      exec "''${SURMOUNT_RUSTC_WRAPPER_NEXT}" "$real" "$@"
    fi
    exec "$real" "$@"
  '';

  tuneWith =
    wrapper: pkg:
    if !(lib.isDerivation pkg) then
      pkg
    else
      pkg.overrideAttrs (
        old:
        let
          prevEnv = old.env or { };
          prevWrap = if prevEnv ? RUSTC_WRAPPER then prevEnv.RUSTC_WRAPPER else null;
          nextDeps =
            if old ? cargoArtifacts && lib.isDerivation old.cargoArtifacts then
              tuneWith wrapper old.cargoArtifacts
            else
              null;
        in
        {
          env =
            prevEnv
            // {
              RUSTC_WRAPPER = wrapper;
            }
            // lib.optionalAttrs (prevWrap != null) {
              SURMOUNT_RUSTC_WRAPPER_NEXT = prevWrap;
            };
          passthru = (old.passthru or { }) // {
            mailHostRustCpuHook = hookText;
            mailHostRustCpuUses = "RUSTC_WRAPPER";
          } // lib.optionalAttrs (nextDeps != null) {
            mailHostRustCpuCargoArtifacts = nextDeps;
          };
        }
        // lib.optionalAttrs (nextDeps != null) {
          cargoArtifacts = nextDeps;
        }
      );

  rustCpuOverlay =
    _final: prev:
    let
      wrapper = prev.writeShellScript "mail-host-rustc-wrapper" hookText;
      tune = tuneWith wrapper;
    in
    lib.foldl' (
      acc: name:
      acc
      // lib.optionalAttrs (prev ? ${name}) {
        ${name} = tune prev.${name};
      }
    ) { } rustNames;
in
{
  config = lib.mkIf cfg.enable {
    nixpkgs.overlays = lib.mkAfter [ rustCpuOverlay ];
  };
}
