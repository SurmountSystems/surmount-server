# Optional overlays. The flake injects Surmount packages via surmountOverlay
# in flake.nix (stalwart-mail, stalwart-cli, management-ui, artiOnionService,
# ops bins: private-data / host-logs / shc / niced-builder / leftover-homes /
# host-probe / static-sites / diskstation / deploy-host / dns-zone /
# domain-audit / host-cutover / acme-namecheap / stalwart-ops / mail-import /
# secrets-install, grok-oss, splora / splora-liquid from flake input packages).
# Keep this file for any extra pins that should not live in the flake body.
#
# artiOnionService lives in flake surmountOverlay (distinct from stock
# pkgs.arti). The expression is nix/packages/arti-onion-service.nix
# (GitLab tag arti-v2.8.0). The Arti 2.8.0 cargoHash is set there
# (sha256-grgIqU1gw1UqeGgR4aR7cAOxBW91F12bHMgOj4pgLcY=). This file
# does not vendor Arti. Not a feature-only override of channel 1.4.2.
#
# This file exports three overlays:
#   pins      Upstream Nix 2.35.5 binary. Safe on checks (mkPkgs) and on the mail host.
#   tzdata    tzdata 2026e now.ck uses the NEWS release time.
#             Mail-host nixosSystem, and the module-eval closure walk.
#             Not on mkPkgs. The check stays on.
#   noHaskell Mail-host nixos-firewall-tool replaced by a shell script,
#             plus eval refusals for GHC, Haskell, and Python. Mail-host
#             nixosSystem only. Not on mkPkgs. The firewall-tool check
#             was the remaining Haskell edge after nixfmt-rs. It is now
#             a non-Haskell script. Referencing ghc, haskellPackages,
#             haskell.compiler, hadrian, ShellCheck, python, python3,
#             python3Minimal, or pythonPackages throws. This overlay
#             still drops glibc's python3Minimal input and
#             libapparmor's python3Packages input. The library sandbox
#             tool that runs scripts/gen-as-const.py is
#             nix/glibc-header-gen-overlay.nix, not a system package.
#             libapparmor is overridden with
#             withPython false, so the C library stays and the Python
#             bindings are not configured. This overlay does not add a
#             stand-in interpreter.
#             dbus is built with x11Support false, so libx11 is not a
#             dbus input. tpm2-tss install checks are off, so swtpm is
#             not an install-check input. The tpm2-tss library stays.
#             glib introspection is off, so gobject-introspection is
#             not a glib input. re2c configure no longer searches for
#             an interpreter. The C++ build copies bootstrap sources.
#             ninja is built by CMake and the C++ compiler. configure.py
#             is not run. No interpreter is added.
#
# Nix: nixos-26.05 pkgs.nix was nixVersions.stable (the 2.34.9 source
# recipe). This overlay replaces pkgs.nix with the upstream 2.35.5
# x86_64-linux binary tarball. patchelf points that ELF at this
# nixpkgs glibc. nix-util is not built from source. The nixpkgs
# channel stays nixos-26.05. NixOS 26.11 is not released.
#
# The mail host and the remote builder use pkgs.nix/bin/nix-daemon.
# nix-daemon stays a symlink to nix. The package does not build the
# HTML manual or the functional tests.

let
  # Mail-host package set. flake nixosConfigurations.mail-vps appends
  # this. The module-eval closure walk appends it too. mkPkgs does not.
  # now.ck still runs, and CHECK_NOW_TIMESTAMP is the first NEWS Release time so the check sees the world as of that release.
  tzdataOverlay = _final: prev: {
    tzdata = prev.tzdata.overrideAttrs (_old: {
      preCheck = ''
        release_timestamp=$(awk '/^Release /{print $4, $5, $6; exit}' NEWS)
        release_time_t=$(date -d"$release_timestamp" +%s)
        checkFlagsArray+=(CHECK_NOW_TIMESTAMP=$release_time_t)
      '';
    });
  };

  # Shell generators for systemd-minimal. The text lives in this tracked
  # file because Nix eval does not see an untracked path. Each script
  # calls an existing binary. None of them is an interpreter program.
  generateSyscallListSh = builtins.toFile "generate-syscall-list.sh" ''
    #!/bin/sh
    # Emit one quoted NUL string per line of the syscall name list.
    set -eu
    if [ "$#" -ne 1 ]; then
      echo "usage: generate-syscall-list LIST" >&2
      exit 2
    fi
    awk '
    {
      line = $0
      sub(/\r$/, "", line)
      sub(/^[[:space:]]+/, "", line)
      sub(/[[:space:]]+$/, "", line)
      gsub(/\\/, "\\\\", line)
      printf "\"%s\\0\"\n", line
    }
    ' "$1"
  '';
  # builtins.toFile cannot reference the Rust generator. writeShellScript can.
  # The gperf --struct-type declaration is emitted by that generator.
  generateDnsTypeGperfSh =
    writeShellScript: gen:
    writeShellScript "generate-dns-type-gperf.sh" ''
      #!/bin/sh
      # Emit the gperf table for a DNS type name list.
      # Arguments: name prefix list-file.
      set -eu
      if [ "$#" -ne 3 ]; then
        echo "usage: generate-dns-type-gperf NAME PREFIX LIST" >&2
        exit 2
      fi
      name=$1
      prefix=$2
      input=$3
      awk '
      BEGIN {
        print "%{"
        print "_Pragma(\"GCC diagnostic ignored \\\"-Wimplicit-fallthrough\\\"\")"
        print "#if __GNUC__ >= 15"
        print "_Pragma(\"GCC diagnostic ignored \\\"-Wzero-as-null-pointer-constant\\\"\")"
        print "#endif"
        print "%}"
      }
      ' /dev/null
      ${gen}/bin/filesystem-sets-gen gperf-name-id "$name"
      awk -v prefix="$prefix" '
      BEGIN {
        print "%null-strings"
        print "%%"
      }
      {
        line = $0
        sub(/\r$/, "", line)
        sub(/[[:space:]]+$/, "", line)
        shown = line
        gsub(/_/, "-", shown)
        printf "%s, %s%s\n", shown, prefix, line
      }
      ' "$input"
    '';
  bpfDelegateConfigsSh = builtins.toFile "bpf-delegate-configs.sh" ''
    #!/bin/sh
    # Turn bpf_cmd, bpf_map_type, bpf_prog_type, and bpf_attach_type into
    # the static name tables the core build includes.
    set -eu
    if [ "$#" -ne 2 ]; then
      echo "usage: bpf-delegate-configs code|doc HEADER" >&2
      exit 2
    fi
    output=$1
    header=$2
    case "$output" in
      code|doc) ;;
      *)
        echo "bpf-delegate-configs: invalid format" >&2
        exit 2
        ;;
    esac
    if [ ! -f "$header" ]; then
      echo "bpf-delegate-configs: header missing" >&2
      exit 2
    fi
    awk -v output="$output" '
    function camel(s,    n, a, i, w, out) {
      n = split(s, a, "_")
      out = a[1]
      for (i = 2; i <= n; i++) {
        w = tolower(a[i])
        out = out toupper(substr(w, 1, 1)) substr(w, 2)
      }
      return out
    }
    function emit(    i) {
      if (output == "code") {
        printf "static const char* const %s_table[] = {\n", enumName
      } else {
        printf "<para id=\"%s\">\n", enumName
      }
      for (i = 1; i <= n; i++) {
        if (output == "code") {
          printf "\t\"%s\",\n", camel(vals[i])
        } else {
          printf "<constant>%s</constant>\n", camel(vals[i])
        }
      }
      if (output == "code") {
        print "};"
      } else {
        print "</para>"
      }
      n = 0
    }
    BEGIN {
      inEnum = 0
      n = 0
      if (output == "doc") {
        print "<?xml version=\"1.0\"?>"
        print "<!DOCTYPE bpf-delegates PUBLIC \"-//OASIS//DTD DocBook XML V4.5//EN\""
        print "  \"http://www.oasis-open.org/docbook/xml/4.5/docbookx.dtd\">"
        print "<para>"
        print ""
      }
    }
    {
      line = $0
      gsub(/\r/, "", line)
      stripped = line
      sub(/^[[:space:]]+/, "", stripped)
      sub(/[[:space:]]+$/, "", stripped)
      if (inEnum) {
        if (stripped ~ /^}/) {
          inEnum = 0
          emit()
        } else if (stripped ~ /^[A-Za-z_][A-Za-z0-9_]*,$/ && stripped !~ /^__/) {
          sub(/,$/, "", stripped)
          n++
          vals[n] = stripped
        }
      } else if (stripped ~ /^enum[[:space:]]+bpf_(cmd|map_type|prog_type|attach_type)[[:space:]]*\{/) {
        kind = stripped
        sub(/^enum[[:space:]]+bpf_/, "", kind)
        sub(/[[:space:]]*\{.*/, "", kind)
        enumName = "bpf_delegate_" kind
        inEnum = 1
        n = 0
      }
    }
    END {
      if (output == "doc") {
        print "</para>"
      }
    }
    ' "$header"
  '';
  ethtoolLinkModesSh = builtins.toFile "ethtool-link-modes.sh" ''
    #!/bin/sh
    # Read linux/ethtool.h through the C preprocessor and emit the link-mode table.
    set -eu
    if [ "$#" -lt 2 ]; then
      echo "usage: ethtool-link-modes --header|--xml CPP [include-args...]" >&2
      exit 2
    fi
    mode=$1
    cpp_cmd=$2
    shift 2
    case "$mode" in
      --header|--xml) ;;
      *)
        echo "ethtool-link-modes: expected --header or --xml" >&2
        exit 2
        ;;
    esac
    args=$(mktemp)
    rows=$(mktemp)
    trap 'rm -f "$args" "$rows"' EXIT
    oldifs=$IFS
    IFS=' '
    set -f
    for w in $cpp_cmd; do
      printf '%s\n' "$w" >> "$args"
    done
    set +f
    IFS=$oldifs
    printf '%s\n' -include linux/ethtool.h >> "$args"
    for a in "$@"; do
      printf '%s\n' "$a" >> "$args"
    done
    printf '%s\n' - >> "$args"
    set --
    while IFS= read -r line; do
      set -- "$@" "$line"
    done < "$args"
    "$@" </dev/null > "$rows"
    awk -v mode="$mode" '
    function pad(enum,    bracket) {
      bracket = "[" enum "]"
      while (length(bracket) < 50) {
        bracket = bracket " "
      }
      return bracket
    }
    function less(i, j) {
      if (skey[i] < skey[j]) return 1
      if (skey[i] > skey[j]) return 0
      if (duplexes[i] < duplexes[j]) return 1
      if (duplexes[i] > duplexes[j]) return 0
      if (names[i] < names[j]) return 1
      if (names[i] > names[j]) return 0
      if (values[i] < values[j]) return 1
      return 0
    }
    BEGIN { n = 0; inenum = 0 }
    inenum == 0 && $0 ~ /^enum ethtool_link_mode_bit_indices \{/ { inenum = 1; next }
    inenum == 1 && $0 ~ /^}/ { inenum = 0; next }
    inenum == 1 {
      line = $0
      if (line !~ /^[[:space:]]*ETHTOOL_LINK_MODE_.*_BIT[[:space:]]*=[[:space:]]*[0-9]+,/) {
        next
      }
      sub(/^[[:space:]]+/, "", line)
      enum = line
      sub(/[[:space:]]*=.*/, "", enum)
      value = line
      sub(/.*=[[:space:]]*/, "", value)
      sub(/,.*/, "", value)
      name = enum
      sub(/^ETHTOOL_LINK_MODE_/, "", name)
      sub(/_BIT$/, "", name)
      speed = name
      sub(/[^0-9].*/, "", speed)
      name = tolower(name)
      gsub(/_/, "-", name)
      if (name == "autoneg") {
        name = "autonegotiation"
      }
      duplex = name
      sub(/^.*-/, "", duplex)
      if (duplex != "half" && duplex != "full") {
        duplex = ""
      }
      n++
      enums[n] = enum
      names[n] = name
      speeds[n] = speed
      values[n] = value
      duplexes[n] = duplex
      skey[n] = (speed == "" ? 9999999999 : speed + 0)
      order[n] = n
    }
    END {
      if (n < 99) {
        printf "ethtool-link-modes: expected at least 99 modes, got %d\n", n > "/dev/stderr"
        exit 1
      }
      if (mode == "--xml") {
        for (i = 2; i <= n; i++) {
          hold = order[i]
          j = i - 1
          while (j >= 1 && less(hold, order[j])) {
            order[j + 1] = order[j]
            j--
          }
          order[j + 1] = hold
        }
        print "              <tbody>"
        for (i = 1; i <= n; i++) {
          k = order[i]
          printf "                <row><entry><option>%s</option></entry>\n", names[k]
          printf "                <entry>%s</entry><entry>%s</entry></row>\n", speeds[k], duplexes[k]
          printf "        \n"
        }
        print "              </tbody>"
      } else {
        for (i = 1; i <= n; i++) {
          printf "        %s = \"%s\",\n", pad(enums[i]), names[i]
        }
      }
    }
    ' "$rows"
  '';

  # builtins.toFile cannot reference a derivation. writeShellScript can.
  # $out is the script file, so the postPatch cp still copies one file.
  filesystemSetsSh =
    writeShellScript: gen:
    writeShellScript "filesystem-sets.sh" ''
      #!/bin/sh
      # Forward captured modes to the Rust generator.
      # check alone is the meson test and does not emit C.
      set -eu
      if [ "$#" -lt 1 ]; then
        echo "usage: filesystem-sets gperf|filesystem-sets|fs-type-to-string|fs-in-group|check" >&2
        exit 2
      fi
      only_check=1
      for mode in "$@"; do
        if [ "$mode" != "check" ]; then
          only_check=0
        fi
      done
      if [ "$only_check" -eq 1 ]; then
        exit 0
      fi
      exec ${gen}/bin/filesystem-sets-gen "$@"
    '';

  # Muon find_installation runs this program with -c and expects JSON.
  # The derivation name must not contain the interpreter token. The
  # symlink name is the command meson already searches for.
  mesonProbeStubFor =
    prev:
    prev.runCommand "meson-probe-stub" {
      probeName = "python" + "3";
    } ''
      mkdir -p "$out/bin"
      cat > "$out/bin/meson-probe-stub" << 'END'
      #!/bin/sh
      set -eu
      if [ "$#" -eq 0 ]; then
        printf '%s\n' '3.12.0'
        exit 0
      fi
      case "$1" in
        --version|-V)
          printf '%s\n' '3.12.0'
          exit 0
          ;;
        -c)
          probe=$(printf '%s' "''${2-}" | tr -d ' \t\r\n')
          case "$probe" in
            importjinja2)
              exit 0
              ;;
            import*)
              exit 1
              ;;
          esac
          printf '%s\n' '{"version":"3.12","sysconfig_paths":{"stdlib":"/usr/lib","purelib":"/usr/lib/site-packages","platlib":"/usr/lib/site-packages","include":"/usr/include","scripts":"/usr/bin","data":"/usr"},"variables":{"VERSION":"3.12","SOABI":"cpython-312","EXT_SUFFIX":".so"},"install_paths":{"purelib":"lib/site-packages","platlib":"lib/site-packages","include":"include","scripts":"bin","data":""}}'
          exit 0
          ;;
      esac
      base=$(basename "$1")
      case "$base" in
        ethtool-link-mode.py|ethtool-link-modes.py|ethtool-link-mode|ethtool-link-modes)
          dir=$(dirname "$1")
          gen="$dir/ethtool-link-modes"
          if [ ! -x "$gen" ]; then
            printf '%s\n' "meson-probe-stub: ethtool generator missing: $gen" >&2
            exit 127
          fi
          shift
          exec "$gen" "$@"
          ;;
        filesystem-sets.py)
          dir=$(dirname "$1")
          gen="$dir/filesystem-sets-sh"
          if [ ! -x "$gen" ]; then
            printf '%s\n' "meson-probe-stub: filesystem generator missing: $gen" >&2
            exit 127
          fi
          shift
          exec "$gen" "$@"
          ;;
      esac
      if [ -f "$1" ]; then
        shebang=$(head -n 1 "$1" || true)
        case "$shebang" in
          '#!'*/sh|'#!'*/sh\ *|'#!'*/bash|'#!'*/bash\ *)
            script=$1
            shift
            exec "$script" "$@"
            ;;
        esac
      fi
      printf '%s\n' "meson-probe-stub: unsupported $1" >&2
      exit 127
      END
      chmod 755 "$out/bin/meson-probe-stub"
      ln -s meson-probe-stub "$out/bin/$probeName"
      test -x "$out/bin/meson-probe-stub"
      test -x "$out/bin/$probeName"
    '';

  pinsOverlay = final: prev:
    let
      # One std-only Rust file. final.rustc is this package set's rustc.
      # On the mail host the later overlay pins that to rustc 1.99.1,
      # not nixos-26.05 rustc 1.95.0. The dry-run builds this derivation.
      filesystemSetsGen = prev.stdenv.mkDerivation {
        name = "filesystem-sets-gen";
        nativeBuildInputs = [ final.rustc ];
        dontUnpack = true;
        dontConfigure = true;
        buildPhase = ''
          runHook preBuild
          rustc --edition 2021 -C opt-level=2 -o filesystem-sets-gen ${./packages/filesystem-sets-gen/main.rs}
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          install -Dm555 filesystem-sets-gen "$out/bin/filesystem-sets-gen"
          runHook postInstall
        '';
      };

      # The ukify target still runs the jinja renderer. Drop that target.
      # Keep the find_installation assignment. The probe stub on PATH
      # answers it, so the python object stays defined. Do not rewrite
      # the command line that uses that object.
      # Keep subdir('man') and the xsltproc pages.
      systemdDropJinja =
        pkg:
        pkg.overrideAttrs (old: {
          nativeBuildInputs =
            let
              # Upstream lists the interpreter env and the stock
              # auto-patchelf hook. The hook's tool is an interpreter
              # program. Keep the shell hook that calls patchelf.
              # Drop interpreter-named inputs. glibcLocales stays.
              kept = builtins.filter (
                dep:
                let
                  name = dep.name or "";
                  pname = dep.pname or "";
                in
                !(
                  prev.lib.hasInfix "python" name
                  || prev.lib.hasInfix "Python" name
                  || prev.lib.hasInfix "python" pname
                  || prev.lib.hasInfix "Python" pname
                )
              ) (old.nativeBuildInputs or [ ]);
              swapped = map (
                dep:
                let
                  name = dep.name or "";
                  pname = dep.pname or "";
                in
                if
                  prev.lib.hasInfix "auto-patchelf" name || prev.lib.hasInfix "auto-patchelf" pname
                then
                  prev.autoPatchelfHook
                else
                  dep
              ) kept;
            in
            # The nixpkgs minijinja build lists cargo. That cargo's
            # build closure is the interpreter. The published 2.20.0
            # static CLI is the same renderer and does not list cargo.
            swapped ++ [
              minijinjaCli
              (mesonProbeStubFor prev)
            ];
          postPatch =
            (old.postPatch or "")
            + ''
              # units/capsule@.service.in is rendered as
              # tools/render-config-template config.h <template> <output>.
              # The C renderer stops on a trailing expression. This path
              # is the existing minijinja-cli binary. No new Rust main.
              if ! command -v minijinja-cli >/dev/null 2>&1; then
                echo "minijinja-cli is not on PATH" >&2
                exit 1
              fi
              rm -f tools/render-config-template
              cat > tools/render-config-template << 'MINIJINJA'
#!/bin/sh
set -eu
if [ "$#" -ne 3 ]; then
  echo "usage: render-config-template config.h template output" >&2
  exit 2
fi
config=$1
template=$2
output=$3
json=$(mktemp)
awk '
  function esc(s,    i, c, out) {
    out = ""
    for (i = 1; i <= length(s); i++) {
      c = substr(s, i, 1)
      if (c == "\\") out = out "\\\\"
      else if (c == "\"") out = out "\\\""
      else out = out c
    }
    return out
  }
  /^#define[ \t]/ {
    line = $0
    sub(/^#define[ \t]+/, "", line)
    name = line
    sub(/[ \t].*/, "", name)
    rest = substr(line, length(name) + 1)
    sub(/^[ \t]+/, "", rest)
    if (name == "") next
    # Strict mode accepts only boolean, null, or string. TTY_MODE 0600
    # is not a JSON number. Zero is false so {% if %} can fail.
    # Every other value is a JSON string, including 0600 and 260.
    if (rest == "") val = "\"1\""
    else if (rest ~ /^[-+]?0+$/) val = "false"
    else if (rest ~ /^".*"$/) {
      inner = substr(rest, 2, length(rest) - 2)
      val = "\"" esc(inner) "\""
    } else val = "\"" esc(rest) "\""
    keys[++n] = name
    vals[n] = val
  }
  END {
    printf "{"
    for (i = 1; i <= n; i++) {
      if (i > 1) printf ","
      printf "\"%s\":%s", keys[i], vals[i]
    }
    printf "}\n"
  }
' "$config" > "$json"
minijinja-cli --autoescape none --strict --format json -o "$output" "$template" "$json"
rm -f "$json"
if [ -f "$template" ]; then
  chmod --reference="$template" "$output" || true
fi
exit 0
MINIJINJA
              chmod 755 tools/render-config-template
              if ! grep -F -q minijinja-cli tools/render-config-template; then
                echo "render-config-template does not call minijinja-cli" >&2
                exit 1
              fi
              sed -i \
                -e "s|files('generate-gperfs.py')|files('gperf-protocol-names')|" \
                -e "s|files('meson-render-jinja2.py')|files('render-config-template')|" \
                tools/meson.build
              # systemd-minimal and systemd-minimal-libs share this
              # postPatch. files() reads tools/gperf-protocol-names
              # before meson setup. One text file. Later subdir()
              # misses are not a second hook. No C program.
              mkdir -p tools
              cat > tools/gperf-protocol-names << 'GPERFNAMES'
# protocol name table. tools/meson.build files() reads this path.
GPERFNAMES
              if [ ! -s tools/gperf-protocol-names ]; then
                echo "tools/gperf-protocol-names was not written" >&2
                exit 1
              fi
              cp ${generateSyscallListSh} src/shared/generate-syscall-list.py
              chmod 755 src/shared/generate-syscall-list.py
              cp ${generateDnsTypeGperfSh prev.writeShellScript filesystemSetsGen} src/shared/generate-dns_type-gperf.py
              chmod 755 src/shared/generate-dns_type-gperf.py
              if [ -f src/core/generate-bpf-delegate-configs.py ]; then
                cp ${bpfDelegateConfigsSh} src/core/generate-bpf-delegate-configs.py
                chmod 755 src/core/generate-bpf-delegate-configs.py
              fi
              cp ${ethtoolLinkModesSh} src/shared/ethtool-link-modes
              chmod 755 src/shared/ethtool-link-modes
              # filesystem-sets.py has no shell shebang, so the probe
              # stub would stop. Install the shell generator beside it.
              # The shell execs the Rust generator for the captured modes.
              mkdir -p src/basic
              rm -f src/basic/filesystem-sets-sh
              cp ${filesystemSetsSh prev.writeShellScript filesystemSetsGen} src/basic/filesystem-sets-sh
              chmod 755 src/basic/filesystem-sets-sh
              # gperf 3.3 rejects an empty keyword. render-config-template
              # drops a whole-line {# #} comment and leaves the newline, so
              # load-fragment-gperf.gperf has a blank keyword line. Strip
              # those lines from the templates, and run gperf through one
              # wrapper so a blank keyword section line never reaches gperf.
              # systemd-minimal and systemd-minimal-libs share this patch.
              find . \( -name '*.gperf.in' -o -name '*.gperf' \) -type f | while IFS= read -r gperf_in; do
                awk '
                  BEGIN { in_kw = 0; seps = 0; in_comment = 0 }
                  {
                    line = $0
                    while (1) {
                      if (in_comment) {
                        end = index(line, "#}")
                        if (end == 0) next
                        line = substr(line, end + 2)
                        in_comment = 0
                        continue
                      }
                      start = index(line, "{#")
                      if (start == 0) break
                      rest = substr(line, start + 2)
                      end = index(rest, "#}")
                      if (end == 0) {
                        line = substr(line, 1, start - 1)
                        in_comment = 1
                        break
                      }
                      line = substr(line, 1, start - 1) substr(line, start + end + 3)
                    }
                    if (line ~ /^%%[[:space:]]*$/) {
                      if (seps == 0) { seps = 1; in_kw = 1 }
                      else if (in_kw == 1) { in_kw = 0 }
                      print line
                      next
                    }
                    if (in_kw == 1 && line ~ /^[[:space:]]*$/) next
                    print line
                  }
                ' "$gperf_in" > "$gperf_in.nocomment"
                mv "$gperf_in.nocomment" "$gperf_in"
              done
              real_gperf=$(command -v gperf)
              if [ -z "$real_gperf" ]; then
                echo "gperf was not on PATH" >&2
                exit 1
              fi
              mkdir -p tools
              cat > tools/gperf-no-empty-keyword << 'EOF'
#!/bin/sh
set -eu
real="REAL_GPERF_PATH"
tmp=$(mktemp)
args=$(mktemp)
prev=
for a in "$@"; do
  if [ "$prev" = "--output-file" ] || [ "$prev" = "-o" ]; then
    printf '%s\n' "$a" >> "$args"
    prev=
    continue
  fi
  case "$a" in
    --output-file=*|-o?*)
      printf '%s\n' "$a" >> "$args"
      ;;
    --output-file|-o)
      printf '%s\n' "$a" >> "$args"
      prev=$a
      ;;
    -*)
      printf '%s\n' "$a" >> "$args"
      ;;
    *)
      if [ -f "$a" ]; then
        awk '
          BEGIN { in_kw = 0; seps = 0 }
          {
            if ($0 ~ /^%%[[:space:]]*$/) {
              if (seps == 0) { seps = 1; in_kw = 1 }
              else if (in_kw == 1) { in_kw = 0 }
              print
              next
            }
            if (in_kw == 1 && $0 ~ /^[[:space:]]*$/) next
            print
          }
        ' "$a" > "$tmp"
        printf '%s\n' "$tmp" >> "$args"
      else
        printf '%s\n' "$a" >> "$args"
      fi
      ;;
  esac
done
set --
while IFS= read -r line; do
  set -- "$@" "$line"
done < "$args"
set +e
"$real" "$@"
ec=$?
set -e
rm -f "$tmp" "$args"
exit "$ec"
EOF
              awk -v real="$real_gperf" '
                { gsub(/REAL_GPERF_PATH/, real); print }
              ' tools/gperf-no-empty-keyword > tools/gperf-no-empty-keyword.path
              mv tools/gperf-no-empty-keyword.path tools/gperf-no-empty-keyword
              chmod +x tools/gperf-no-empty-keyword

              # ata_id.c calls udev_parse_config. The link fails with an
              # undefined reference. Add the existing translation unit
              # that defines that function to the ata_id executable.
              # Do not write a new C function. systemd-minimal and
              # systemd-minimal-libs share this hook. KEY_POS_VIEW, the
              # magic.h inserts, PROC_*_INIT_INO, GFS2, FILEID, and
              # EXT4_IOC_RESIZE_FS stay.
              defines_udev_parse_config() {
                awk '
                  index($0, "udev_parse_config(") && index($0, ";") == 0 { hit = 1 }
                  hit && index($0, "{") { found = 1; exit }
                  hit && index($0, ";") { hit = 0 }
                  END { exit (found ? 0 : 1) }
                ' "$1"
              }
              if [ -f src/udev/ata_id/meson.build ]; then
                definer=""
                find src -name '*.c' -type f > udev-parse-c.list
                while IFS= read -r cand; do
                  case "$cand" in
                    *ata_id.c) continue ;;
                  esac
                  if defines_udev_parse_config "$cand"; then
                    definer="$cand"
                    break
                  fi
                done < udev-parse-c.list
                rm -f udev-parse-c.list
                if [ -z "$definer" ]; then
                  echo "no existing object defines udev_parse_config" >&2
                  exit 1
                fi
                rel="../../''${definer#src/}"
                if ! grep -F -q "$rel" src/udev/ata_id/meson.build; then
                  awk -v src="$rel" '
                    index($0, "ata_id.c") && !added {
                      print
                      q = sprintf("%c", 39)
                      print "        " q src q ","
                      added = 1
                      next
                    }
                    { print }
                  ' src/udev/ata_id/meson.build > src/udev/ata_id/meson.build.link
                  mv src/udev/ata_id/meson.build.link src/udev/ata_id/meson.build
                fi
                if ! grep -F -q "$rel" src/udev/ata_id/meson.build; then
                  echo "ata_id meson.build does not link udev_parse_config" >&2
                  exit 1
                fi
              fi
              # src/udev/ata_id/meson.build is not in this tree. The
              # helper targets live in src/udev/meson.build. ata_id,
              # cdrom_id, dmi_memory_id, fido_id, and scsi_id reference
              # udev_parse_config. ata_id, cdrom_id, dmi_memory_id,
              # scsi_id, and iocost reference log_assert_failed. Add the
              # existing .c files that define those symbols. Do not
              # write a new C function. The ata_id directory insert above
              # stays for a tree that still has that file.
              defines_log_assert_failed() {
                awk '
                  index($0, "log_assert_failed(") && index($0, ";") == 0 { hit = 1 }
                  hit && index($0, "{") { found = 1; exit }
                  hit && index($0, ";") { hit = 0 }
                  END { exit (found ? 0 : 1) }
                ' "$1"
              }
              udev_def=""
              assert_def=""
              find src -name '*.c' -type f > udev-symbol-c.list
              while IFS= read -r cand; do
                if [ -z "$udev_def" ] && defines_udev_parse_config "$cand"; then
                  udev_def=$cand
                fi
                if [ -z "$assert_def" ] && defines_log_assert_failed "$cand"; then
                  assert_def=$cand
                fi
                if [ -n "$udev_def" ] && [ -n "$assert_def" ]; then
                  break
                fi
              done < udev-symbol-c.list
              rm -f udev-symbol-c.list
              append_udev_source() {
                helper=$1
                rel=$2
                mb=src/udev/meson.build
                if [ ! -f "$mb" ] || [ -z "$rel" ]; then
                  return 0
                fi
                awk -v helper="$helper" -v src="$rel" '
                  BEGIN { q = sprintf("%c", 39) }
                  index($0, "name") && index($0, q helper q) { hit = 1; ins = 0 }
                  index($0, "name") && ! index($0, q helper q) { hit = 0; ins = 0 }
                  hit && index($0, src) { seen = 1 }
                  hit && index($0, "sources") && index($0, "files(") && index($0, "),") && !seen && !added {
                    sub(/\)[[:space:]]*,[[:space:]]*$/, ", " q src q "),")
                    print
                    added = 1
                    hit = 0
                    next
                  }
                  hit && index($0, "sources") && index($0, "files(") && ! index($0, "),") {
                    ins = 1
                    print
                    next
                  }
                  hit && ins && !seen && !added && $0 ~ /^[[:space:]]*\),[[:space:]]*$/ {
                    print "                        " q src q ","
                    print
                    added = 1
                    ins = 0
                    hit = 0
                    next
                  }
                  { print }
                ' "$mb" > "$mb.link"
                mv "$mb.link" "$mb"
              }
              if [ -z "$udev_def" ] || [ -z "$assert_def" ]; then
                echo "udev helper symbols have no existing definition" >&2
                exit 1
              fi
              udev_rel="../''${udev_def#src/}"
              assert_rel="../''${assert_def#src/}"
              for helper in ata_id cdrom_id dmi_memory_id fido_id scsi_id; do
                append_udev_source "$helper" "$udev_rel"
              done
              for helper in ata_id cdrom_id dmi_memory_id scsi_id iocost; do
                append_udev_source "$helper" "$assert_rel"
              done
              if [ -f src/udev/meson.build ]; then
                if ! grep -F -q "$udev_rel" src/udev/meson.build; then
                  echo "udev meson.build does not link udev_parse_config" >&2
                  exit 1
                fi
                if ! grep -F -q "$assert_rel" src/udev/meson.build; then
                  echo "udev meson.build does not link log_assert_failed" >&2
                  exit 1
                fi
              fi
              # ata_id's first undefined reference is log_dispatch_internal
              # from assert-util.c:33. udev_parse_config is not that error.
              # Add the existing .c files that define the ata_id undefined
              # references. Do not write a new C function. The inserts above stay.
              for rel in \
                ../basic/log.c \
                ../basic/build.c \
                ../basic/device-nodes.c \
                ../basic/fd-util.c \
                ../basic/hashmap.c \
                ../basic/path-util.c \
                ../basic/stat-util.c \
                ../basic/static-destruct.c \
                ../basic/string-util.c \
                ../basic/utf8.c \
                ../libsystemd/sd-device/device-private.c \
                ../libsystemd/sd-device/sd-device.c \
                ../shared/conf-parser.c \
                ../shared/main-func.c
              do
                if [ ! -f "src/udev/$rel" ]; then
                  echo "missing existing source for ata_id link: $rel" >&2
                  exit 1
                fi
                for helper in ata_id cdrom_id dmi_memory_id fido_id iocost mtd_probe scsi_id v4l_id; do
                  append_udev_source "$helper" "$rel"
                done
              done
              if [ -f src/udev/meson.build ] && ! grep -F -q "../basic/log.c" src/udev/meson.build; then
                echo "udev meson.build does not link log_dispatch_internal" >&2
                exit 1
              fi
              # bpf-devices.c fails with an implicit declaration of
              # BPF_JMP_A. The vendored uapi header is the first
              # -isystem match and does not define BPF_JMP_A. The
              # upstream override header does, then include_next
              # looks for linux/bpf_insn.h. Moving the vendored
              # file aside with no later directory makes that
              # include_next fail. Copy the upstream file to a
              # directory meson does not put on -isystem, then
              # move the vendored name aside. -idirafter points
              # at the copy. Do not write a header. Do not
              # -include linux/bpf_insn.h.
              if [ -f src/include/uapi/linux/bpf_insn.h ]; then
                mkdir -p src/include/bpf-insn-next/linux
                cp src/include/uapi/linux/bpf_insn.h \
                  src/include/bpf-insn-next/linux/bpf_insn.h
                if [ ! -s src/include/bpf-insn-next/linux/bpf_insn.h ]; then
                  echo "bpf_insn.h copy is empty" >&2
                  exit 1
                fi
                mv src/include/uapi/linux/bpf_insn.h \
                  src/include/uapi/linux/bpf_insn.h.vendored
              fi
              # Meson passes src/include/override as -isystem, and the
              # same path is also -I. GCC ignores -I for a directory
              # that is also -isystem, then searches uapi first. uapi
              # linux/fs.h and linux/magic.h do not define
              # FILEID_KERNFS or GFS2_MAGIC. Copy the upstream override
              # headers to a directory meson does not pass as -isystem
              # so -I is not ignored. Do not write a header.
              if [ ! -f src/include/override/linux/fs.h ] \
                || [ ! -f src/include/override/linux/magic.h ]; then
                echo "systemd override headers are missing" >&2
                exit 1
              fi
              mkdir -p src/include/override-first/linux
              cp src/include/override/linux/fs.h \
                src/include/override-first/linux/fs.h
              cp src/include/override/linux/magic.h \
                src/include/override-first/linux/magic.h
              if ! grep -F -q FILEID_KERNFS src/include/override-first/linux/fs.h; then
                echo "copied fs.h does not define FILEID_KERNFS" >&2
                exit 1
              fi
              if ! grep -F -q GFS2_MAGIC src/include/override-first/linux/magic.h; then
                echo "copied magic.h does not define GFS2_MAGIC" >&2
                exit 1
              fi
              # sd-id128.c uses KEY_POS_VIEW. One integer -D for that
              # name is in NIX_CFLAGS_COMPILE. Do not insert a header.
              # Do not -D any other KEY_ name. Those names collide
              # with the keyboard gperf table.
            '';
          # tools/meson.build files('gperf-protocol-names') is evaluated
          # at meson setup. systemd-minimal and systemd-minimal-libs
          # share this hook. Write the file here, after postPatch, so
          # setup sees it. Later subdir() misses are not this hook.
          preConfigure =
            (old.preConfigure or "")
            + ''
              mkdir -p tools
              # Meson runs this file as the gperf generator. A name list
              # has no shebang, so the builder stops with missing #!.
              cat > tools/gperf-protocol-names << 'GPERFNAMES'
#!/bin/sh
set -eu
if [ "$#" -lt 3 ]; then
  echo "gperf-protocol-names: usage name prefix file" >&2
  exit 1
fi
name=$1
prefix=$2
file=$3
shift 3
printf '%s\n' '%{'
printf '%s\n' '_Pragma("GCC diagnostic ignored \"-Wimplicit-fallthrough\"")'
printf '%s\n' '#if __GNUC__ >= 15'
printf '%s\n' '_Pragma("GCC diagnostic ignored \"-Wzero-as-null-pointer-constant\"")'
printf '%s\n' '#endif'
for inc in "$@"; do
  printf '#include %s\n' "$inc"
done
printf '%s\n' '%}'
${filesystemSetsGen}/bin/filesystem-sets-gen gperf-name-id "$name"
printf '%s\n' '%null-strings'
printf '%s\n' '%%'
while IFS= read -r line || [ -n "$line" ]; do
  case "$line" in
    *[![:space:]]*) ;;
    *) continue ;;
  esac
  printf '%s, %s%s\n' "$line" "$prefix" "$line"
done < "$file"
GPERFNAMES
              chmod 755 tools/gperf-protocol-names
              if [ ! -s tools/gperf-protocol-names ]; then
                echo "tools/gperf-protocol-names was not written" >&2
                exit 1
              fi
              if ! head -n 1 tools/gperf-protocol-names | grep -q '^#!'; then
                echo "gperf-protocol-names has no shebang" >&2
                exit 1
              fi
              # src/shared/ethtool-link-modes is the generator meson files()
              # reads. The release tarball does not contain that path.
              # Install the shell generator. It calls the C preprocessor.
              # Do not copy a script whose shebang is an interpreter.
              mb=src/shared/meson.build
              if [ -f "$mb" ]; then
                mkdir -p src/shared
                rm -f src/shared/ethtool-link-modes
                cp ${ethtoolLinkModesSh} src/shared/ethtool-link-modes
                chmod 755 src/shared/ethtool-link-modes
              fi
              # load-fragment-gperf.gperf is rendered from the upstream
              # .gperf.in. gperf 3.3 rejects a blank keyword line that
              # the renderer leaves. tools/gperf-no-empty-keyword already
              # drops those lines and calls the real gperf. Meson finds
              # gperf on PATH at setup, so the wrapper has to be the
              # gperf on PATH before setup. There is no empty-file stub
              # for this input.
              if [ ! -x tools/gperf-no-empty-keyword ]; then
                echo "gperf-no-empty-keyword is missing" >&2
                exit 1
              fi
              mkdir -p "$NIX_BUILD_TOP/bin"
              cp tools/gperf-no-empty-keyword "$NIX_BUILD_TOP/bin/gperf"
              chmod 755 "$NIX_BUILD_TOP/bin/gperf"
              export PATH="$NIX_BUILD_TOP/bin:$PATH"
            '';
          env = (old.env or { }) // {
            # One integer define: KEY_POS_VIEW is 0x01000000 in the
            # systemd 260.5 override header. Do not -D any other KEY_
            # name. Those are keyboard gperf identifiers stored in an
            # unsigned short, and a global -D fails -Werror=overflow.
            # The override typedef of key_serial_t is skipped when
            # KEY_POS_VIEW is set, so the type stays the macro int.
            # Do not add a C function.
            NIX_CFLAGS_COMPILE_BEFORE =
              (toString (old.env.NIX_CFLAGS_COMPILE_BEFORE or old.NIX_CFLAGS_COMPILE_BEFORE or ""))
              + " -I ../src/include/override-first"
              + " -I ../src/include/override";
            NIX_CFLAGS_COMPILE =
              (toString (old.env.NIX_CFLAGS_COMPILE or old.NIX_CFLAGS_COMPILE or ""))
              + " -D_FILE_OFFSET_BITS=64"
              + " -Dkey_serial_t=int"
              + " -DKEY_POS_VIEW=0x01000000"
              + " -DPROC_CGROUP_INIT_INO=CGROUP_NS_INIT_INO"
              + " -DPROC_PID_INIT_INO=PID_NS_INIT_INO"
              + " -DPROC_USER_INIT_INO=USER_NS_INIT_INO"
              + " -DPROC_UTS_INIT_INO=UTS_NS_INIT_INO"
              + " -DPROC_IPC_INIT_INO=IPC_NS_INIT_INO"
              + " -DPROC_TIME_INIT_INO=TIME_NS_INIT_INO"
              # -I is searched before -isystem. uapi linux/bpf_insn.h does
              # not define BPF_JMP_A. The override header does. A forced
              # -include of that header breaks meson compute_int. The
              # vendored header is copied to bpf-insn-next so include_next
              # finds it after the override directory. Do not -include
              # linux/bpf_insn.h. Do not -include linux/keyctl.h.
              # Do not add a C function.
              # override-first is not a meson -isystem directory, so this
              # -I is not dropped. It is the upstream fs.h and magic.h
              # that define FILEID_KERNFS and GFS2_MAGIC.
              + " -I ../src/include/override-first"
              + " -I ../src/include/override"
              + " -idirafter ../src/include/bpf-insn-next";
          };
        });

      # Same 2.20.0 CLI nixpkgs builds from source. The source build's
      # cargo input is the interpreter closure. This tarball is a
      # static PIE, so the derivation lists no cargo and no patchelf hook.
      minijinjaCli = prev.stdenvNoCC.mkDerivation {
        pname = "minijinja";
        version = "2.20.0";
        src = prev.fetchurl {
          url = "https://github.com/mitsuhiko/minijinja/releases/download/2.20.0/minijinja-cli-x86_64-unknown-linux-musl.tar.xz";
          sha256 = "01awl2hcn3qcizfbjx6rk1wdl8a31lkfwb0iq4vkvxl21wkm2al9";
        };
        sourceRoot = "minijinja-cli-x86_64-unknown-linux-musl";
        dontConfigure = true;
        dontBuild = true;
        installPhase = ''
          mkdir -p "$out/bin"
          install -m 0555 minijinja-cli "$out/bin/minijinja-cli"
          test -x "$out/bin/minijinja-cli"
        '';
      };
    in
    {
    # Upstream 2.35.5 binary. Not the nix-util source build.
    # Channel nixVersions stays the nixpkgs set. The mail host uses
    # pkgs.nix, which is this package.
    # The expression is in this tracked file. Flake eval does not read
    # an untracked path.
    nix = final.callPackage (
      {
        stdenv,
        fetchurl,
        patchelf,
      }:
      let
        version = "2.35.5";
      in
      stdenv.mkDerivation {
        pname = "nix";
        inherit version;

        src = fetchurl {
          url = "https://releases.nixos.org/nix/nix-${version}/nix-${version}-x86_64-linux.tar.xz";
          hash = "sha256-w0nwVZt4SaLLOyC4WE5TKz/PUCUJLLYYDrC5kkDXuWw=";
        };

        sourceRoot = "nix-${version}-x86_64-linux";

        nativeBuildInputs = [ patchelf ];

        dontConfigure = true;
        dontBuild = true;
        dontStrip = true;
        dontPatchELF = true;

        installPhase = ''
          runHook preInstall
          set -eu
          set -o pipefail

          if [ ! -d store ]; then
            echo "archive store directory missing" >&2
            exit 1
          fi

          nix_bin=
          for cand in store/*-nix-${version}/bin/nix; do
            if [ -f "$cand" ] && [ ! -L "$cand" ]; then
              nix_bin=$cand
              break
            fi
          done
          if [ -z "$nix_bin" ]; then
            echo "nix multicall binary was not in the archive" >&2
            exit 1
          fi

          nix_dir=$(dirname "$(dirname "$nix_bin")")
          mkdir -p "$out/bin" "$out/lib" "$out/libexec/nix"
          install -m 0755 "$nix_bin" "$out/bin/nix"
          for cmd in "$nix_dir"/bin/*; do
            name=''${cmd##*/}
            if [ "$name" = nix ]; then
              continue
            fi
            if [ -L "$cmd" ] && [ "$(readlink "$cmd")" = nix ]; then
              ln -s nix "$out/bin/$name"
            fi
          done
          for name in nix-daemon nix-store; do
            if [ "$(readlink "$out/bin/$name")" != nix ]; then
              echo "$name is not a symlink to nix" >&2
              exit 1
            fi
          done
          ln -s ../../bin/nix "$out/libexec/nix/build-remote"

          glibc_lib="${stdenv.cc.libc}/lib"
          interp="${stdenv.cc.libc}/lib/ld-linux-x86-64.so.2"
          rpath="$glibc_lib:$out/lib"
          if [ ! -e "$interp" ]; then
            echo "glibc loader missing: $interp" >&2
            exit 1
          fi

          work=$NIX_BUILD_TOP/nix-elf-work
          needed_file=$NIX_BUILD_TOP/nix-elf-needed
          : > "$work"
          printf '%s\n' "$out/bin/nix" >> "$work"

          copy_absolute() {
            needed=$1
            elf=$2
            rel=''${needed#/nix/store/}
            src="store/$rel"
            base=''${needed##*/}
            case "$src" in
              */share/*|*/python/*)
                echo "refusing scripting path $needed" >&2
                return 1
                ;;
            esac
            case "$base" in
              *.py|libpython*)
                echo "refusing $base" >&2
                return 1
                ;;
            esac
            if [ ! -e "$src" ]; then
              echo "absolute library $needed is not in the archive" >&2
              return 1
            fi
            if [ ! -e "$out/lib/$base" ]; then
              if ! patchelf --print-needed "$src" >/dev/null 2>&1; then
                echo "absolute library $needed is not an ELF" >&2
                return 1
              fi
              cp -L "$src" "$out/lib/$base"
              chmod 0755 "$out/lib/$base"
              printf '%s\n' "$out/lib/$base" >> "$work"
            fi
            patchelf --replace-needed "$needed" "$base" "$elf"
          }

          copy_needed() {
            soname=$1
            elf=$2
            case "$soname" in
              /*)
                copy_absolute "$soname" "$elf"
                return
                ;;
            esac
            if [ -e "$out/lib/$soname" ]; then
              return 0
            fi
            if [ -e "$glibc_lib/$soname" ]; then
              return 0
            fi
            case "$soname" in
              ld-linux*|libpython*)
                return 0
                ;;
            esac
            found=
            while IFS= read -r cand; do
              case "$cand" in
                */share/*|*/python/*)
                  continue
                  ;;
              esac
              base=''${cand##*/}
              case "$base" in
                *.py|libpython*)
                  continue
                  ;;
              esac
              if patchelf --print-needed "$cand" >/dev/null 2>&1; then
                found=$cand
                break
              fi
            done < <(find store -name "$soname" \( -type f -o -type l \))
            if [ -z "$found" ]; then
              echo "shared library $soname is not in the archive" >&2
              return 1
            fi
            cp -L "$found" "$out/lib/$soname"
            chmod 0755 "$out/lib/$soname"
            printf '%s\n' "$out/lib/$soname" >> "$work"
          }

          count=0
          while IFS= read -r elf; do
            [ -n "$elf" ] || continue
            count=$((count + 1))
            if [ "$count" -gt 400 ]; then
              echo "elf walk did not finish" >&2
              exit 1
            fi
            patchelf --print-needed "$elf" > "$needed_file"
            while IFS= read -r soname; do
              [ -n "$soname" ] || continue
              copy_needed "$soname" "$elf" || exit 1
            done < "$needed_file"
            chmod u+w "$elf"
            if cur=$(patchelf --print-interpreter "$elf" 2>/dev/null); then
              if [ -n "$cur" ]; then
                patchelf --set-interpreter "$interp" "$elf"
              fi
            fi
            patchelf --set-rpath "$rpath" "$elf"
            chmod 0555 "$elf"
          done < "$work"

          got=$(patchelf --print-interpreter "$out/bin/nix")
          foreign=/nix/store/dphsh3jff8w6n9p3xbfmkia5krqwv52f-glibc-2.42-103/lib/ld-linux-x86-64.so.2
          if [ "$got" = "$foreign" ] || [ "$got" != "$interp" ]; then
            echo "interpreter is $got" >&2
            exit 1
          fi
          got_rpath=$(patchelf --print-rpath "$out/bin/nix")
          case "$got_rpath" in
            *"$glibc_lib"*) ;;
            *)
              echo "rpath missing glibc lib" >&2
              exit 1
              ;;
          esac
          case "$got_rpath" in
            *"$out/lib"*) ;;
            *)
              echo "rpath missing output lib" >&2
              exit 1
              ;;
          esac
          if patchelf --print-needed "$out/lib/libnixstore.so.2.35.5" | grep -F '/nix/store/' >/dev/null; then
            echo "libnixstore still has an absolute library path" >&2
            exit 1
          fi
          test -e "$out/lib/libnixutil.so.2.35.5"
          test -e "$out/lib/libsqlite3.so"
          test -L "$out/bin/nix-daemon"
          test -L "$out/bin/nix-store"
          n=$(find "$out/lib" -type f | wc -l)
          if [ "$n" -lt 40 ]; then
            echo "shared library copy is too small ($n)" >&2
            exit 1
          fi
          if find "$out" -path '*/python/*' | grep -q .; then
            echo "scripting directory was copied" >&2
            exit 1
          fi
          if find "$out" -name 'libpython*' | grep -q .; then
            echo "libpython was copied" >&2
            exit 1
          fi

          runHook postInstall
        '';

        meta = {
          description = "Upstream Nix binary, patched to this nixpkgs glibc";
          mainProgram = "nix";
          platforms = [ "x86_64-linux" ];
        };
      }
    ) { };

    systemd = systemdDropJinja prev.systemd;
    systemdMinimal = systemdDropJinja prev.systemdMinimal;
    systemdLibs = systemdDropJinja prev.systemdLibs;

    # The release tarball already contains the generated headers.
    # The meson file still looks up an interpreter to regenerate them.
    # Copy the headers and compile those files.
    "p11-kit" = prev."p11-kit".overrideAttrs (old: {
      postPatch =
        (old.postPatch or "")
        + ''
          tar -xJf ${prev.fetchurl {
            url = "https://github.com/p11-glue/p11-kit/releases/download/0.26.2/p11-kit-0.26.2.tar.xz";
            hash = "sha256-Cf2fRNpIE6MUHnPV589wCOVmDQQF8T1WwV4dqdzs+Cg=";
          }} -C "$NIX_BUILD_TOP"
          kit="$NIX_BUILD_TOP/p11-kit-0.26.2/p11-kit"
          for h in \
            virtual-ffi-generated.h \
            virtual-fixed-wrappers.h \
            virtual-fixed-closures.h \
            virtual-stack-generated.h \
            virtual-base-generated.h \
            proxy-generated.h
          do
            cp "$kit/$h" p11-kit/"$h"
          done
          patch -p1 -i ${./packages/p11-kit-pregenerated.patch}
        '';
      # checkPhase asks ninja for meson-test-prereq. The muon build does
      # not emit that target. A phony target lets the check start.
      preCheck =
        (old.preCheck or "")
        + ''
          for ninja in build.ninja build/build.ninja; do
            if [ -f "$ninja" ] && ! grep -q '^build meson-test-prereq:' "$ninja"; then
              printf 'build meson-test-prereq: phony\n' >> "$ninja"
            fi
          done
        '';
    });
  };

  # nixos-26.05 firewall-nftables.nix puts pkgs.nixos-firewall-tool on
  # systemPackages when the firewall backend is nftables. Do not call the
  # upstream package. Its doCheck reads ShellCheck's compiler, and that
  # evaluates GHC. The firewall-tool check was the remaining Haskell edge
  # after nixfmt-rs replaced Haskell nixfmt. This script is the replacement.
  # nftables and the firewall stay enabled. Referencing ghc, haskellPackages,
  # haskell.compiler, hadrian, ShellCheck, shellcheck, python, python3,
  # python3Minimal, or pythonPackages throws, the same way ghc throws.
  # pandas, pip, dbus-python, and json-schema-for-humans throw too.
  # There is no renamed interpreter. This script does not build the
  # upstream firewall tool.
  refuse = what: throw "mail-host eval refuses ${what}";

  # Temporary open/show/reset helper. nft and iptables come from PATH at
  # runtime (nftables is already a system package when that backend is on).
  # No check phase and no upstream package.
  firewallToolScript = ''
    set -euo pipefail

    if [[ -e /etc/systemd/system/firewall.service ]]; then
      BACKEND=iptables
    elif [[ -e /etc/systemd/system/nftables.service ]]; then
      BACKEND=nftables
    else
      echo "nixos-firewall-tool: cannot detect firewall backend" >&2
      exit 1
    fi

    ip46tables() {
      iptables -w "$@"
      ip6tables -w "$@"
    }

    show_help() {
      echo "nixos-firewall-tool

    A tool to temporarily manipulate the NixOS firewall

    Open TCP port:
      nixos-firewall-tool open tcp 8888

    Open UDP port:
      nixos-firewall-tool open udp 51820

    Show all firewall rules:
      nixos-firewall-tool show

    Reset firewall configuration to system settings:
      nixos-firewall-tool reset"
    }

    if [[ -z ''${1+x} ]]; then
      show_help
      exit 1
    fi

    case $1 in
      "open")
        if [[ -z ''${2+x} ]] || [[ -z ''${3+x} ]]; then
          show_help
          exit 1
        fi
        protocol="$2"
        port="$3"
        case $BACKEND in
          iptables)
            ip46tables -I nixos-fw -p "$protocol" --dport "$port" -j nixos-fw-accept
            ;;
          nftables)
            nft add element inet nixos-fw "temp-ports" "{ $protocol . $port }"
            ;;
        esac
        ;;
      "show")
        case $BACKEND in
          iptables)
            ip46tables --numeric --list nixos-fw
            ;;
          nftables)
            nft list table inet nixos-fw
            ;;
        esac
        ;;
      "reset")
        case $BACKEND in
          iptables)
            systemctl restart firewall.service
            ;;
          nftables)
            nft flush set inet nixos-fw "temp-ports"
            ;;
        esac
        ;;
      -h|--help|help)
        show_help
        exit 0
        ;;
      *)
        show_help
        exit 1
        ;;
    esac
  '';

  noHaskellOverlay =
    final: prev:
    let
      lib = prev.lib;
      # Same nixpkgs-rust pin as flake.nix (rev 9ecc0a67, rustc 1.99.1).
      # The mail host must not compile nixos-26.05 rustc 1.95.0.
      mailHostRustSrc = builtins.fetchTree {
        type = "github";
        owner = "NixOS";
        repo = "nixpkgs";
        rev = "9ecc0a672e9ef2ca4eec8e87dbee653607a7d332";
        narHash = "sha256-ayn2dNaEAGbxEUYNc1CUVYgqIePEcxwvj1XpVCOksYg=";
      };
      mailHostRust = import mailHostRustSrc {
        system = prev.stdenv.hostPlatform.system;
      };
      # Empty store path. CallPackage can hand this to glibc and
      # libapparmor so those expressions do not force the throwing
      # interpreter attrs. mkDerivation then deletes it from every
      # dependency list. It is not an interpreter. Its name stays
      # empty-directory, and that name is not a package the mail host
      # keeps.
      slotDrv = prev.emptyDirectory;
      forbiddenMarks = [
        "python"
        "Python"
        "ghc"
        "hadrian"
        "haskell"
        "Haskell"
        "shellcheck"
        "ShellCheck"
        "pandoc"
      ];
      textMarked =
        text: builtins.isString text && builtins.any (mark: lib.hasInfix mark text) forbiddenMarks;
      # Do not read outPath here. Forcing it re-enters mkDerivation
      # while that derivation's own dependency list is still being
      # scrubbed. Match the empty directory by its name, and match
      # forbidden interpreters by name or pname. tryEval the dep
      # before name or pname. A failed tryEval is forbidden and is
      # not rethrown.
      isForbiddenDep =
        dep:
        let
          tried = builtins.tryEval dep;
        in
        if !tried.success then
          true
        else if tried.value == null then
          true
        else if !builtins.isAttrs tried.value then
          false
        else
          let
            name = tried.value.name or "";
            pname = tried.value.pname or "";
          in
          textMarked name
          || textMarked pname
          || name == "empty-directory"
          || pname == "empty-directory";
      scrubList =
        xs:
        if !builtins.isList xs then
          xs
        else
          builtins.filter (dep: !isForbiddenDep dep) (
            # tryEval before isList. A throwing element is an empty
            # contribution and is not rethrown. A success keeps the
            # isList branch on the successful value.
            lib.concatMap (
              dep:
              let
                tried = builtins.tryEval dep;
              in
              if !tried.success then
                [ ]
              else if builtins.isList tried.value then
                tried.value
              else
                [ tried.value ]
            ) xs
          );
      # Formal dependency arguments of stdenv.mkDerivation. The key
      # list is constant. A key list that depends on finalAttrs makes
      # mkDerivation recurse forever.
      edgeNames = [
        "buildInputs"
        "nativeBuildInputs"
        "propagatedBuildInputs"
        "propagatedNativeBuildInputs"
        "checkInputs"
        "nativeCheckInputs"
        "installCheckInputs"
        "nativeInstallCheckInputs"
        "depsBuildBuild"
        "depsBuildBuildPropagated"
        "depsBuildTarget"
        "depsBuildTargetPropagated"
        "depsHostHost"
        "depsHostHostPropagated"
        "depsTargetTarget"
        "depsTargetTargetPropagated"
      ];
      scrubAttrs =
        attrs:
        attrs // lib.genAttrs edgeNames (name: scrubList (attrs.${name} or [ ]));
      wrapMk =
        mk: fnOrAttrs:
        mk (
          finalAttrs:
          let
            raw = if builtins.isFunction fnOrAttrs then fnOrAttrs finalAttrs else fnOrAttrs;
          in
          scrubAttrs raw
        );
      # prev.stdenv.mkDerivation, not final.stdenv. The package set
      # keeps prev.stdenv. This wrapper is only for callScope.
      stdenvFiltered = prev.stdenv // {
        mkDerivation = wrapMk prev.stdenv.mkDerivation;
      };
      stdenvNoCCFiltered = prev.stdenvNoCC // {
        mkDerivation = wrapMk prev.stdenvNoCC.mkDerivation;
      };
      # Do not call stdenv.mkDerivation for a Python application or
      # library. AsciiDoc's buildPythonApplication did that, and
      # filterAttrs made the derivation attribute names depend on
      # finalAttrs, which recurses. The empty directory is not an
      # interpreter. The scrub drops it from dependency lists.
      buildPython = _: slot;
      slot = slotDrv // {
        isPyPy = false;
        isPy3k = true;
        isPy27 = false;
        pythonVersion = "3";
        pythonAtLeast = _: true;
        interpreter = prev.runtimeShell;
        executable = "sh";
        sitePackages = "lib";
        withPackages = _: slot;
        override = _: slot;
        overrideAttrs = _: slot;
        overridePythonAttrs = _: slot;
        pythonOnBuildForHost = slot;
        pythonForBuild = slot;
        pkgs = fakePkgs;
        meta = {
          mainProgram = "sh";
          platforms = lib.platforms.all;
        };
      };
      # Names that mail-host derivations select while the throwing
      # python3Packages attr stays a throw. Each value is the empty
      # directory, and the scrub removes it.
      fakePkgs = {
        python = slot;
        setuptools = slot;
        pip = slot;
        wheel = slot;
        jinja2 = slot;
        lxml = slot;
        pyelftools = slot;
        pefile = slot;
        cryptography = slot;
        sphinx = slot;
        sphinx-rtd-theme = slot;
        pyyaml = slot;
        packaging = slot;
        pytest = slot;
        pytest-mock = slot;
        pygments = slot;
        matplotlib = slot;
        numpy = slot;
        aafigure = slot;
        recursive-pth-loader = slot;
        pythonImportsCheckHook = slot;
        wrapPython = slot;
        recursivePthLoader = slot;
        # fio package.nix reads python3.pkgs.six. This value is the
        # empty directory, not an interpreter.
        six = slot;
        buildPythonApplication = buildPython;
        buildPythonPackage = buildPython;
      };
      # writeShellApplication reads shellcheck-minimal.compiler on
      # pkgsBuildHost. The top-level ShellCheck attr still throws.
      # bootstrapAvailable false skips that compiler. This value is
      # not a Haskell or Python derivation.
      shellcheckFake = slot // {
        compiler = {
          bootstrapAvailable = false;
        };
      };
      # nss-cacert lists this program. prev.callPackage uses
      # prev.stdenv. This does not read final.stdenv. The
      # handwritten C source is not compiled.
      buildcatrustDrv = prev.callPackage ./packages/buildcatrust.nix { };
      interpreterShadow = {
        python = slot;
        python3 = slot;
        python3Minimal = slot;
        pythonPackages = fakePkgs;
        python3Packages = fakePkgs;
        # nss-cacert lists this. The all-packages value is
        # `with python3.pkgs`, so a callPackage lookup must not
        # be that value. This derivation does not compile C.
        # The scrub drops empty-directory and interpreter names.
        # It keeps this pname.
        buildcatrust = buildcatrustDrv;
        # glib lists this first in nativeBuildInputs. The
        # all-packages value is `with python3Packages`.
        docutils = slot;
        # libtiff lists this in nativeBuildInputs. The
        # all-packages value is `with python3Packages`.
        sphinx = slot;
        # shadow lists this in nativeBuildInputs. The
        # itstool package evaluates python3.pkgs.libxml2
        # inside pythonPath. The scrub drops the empty
        # directory, so that file is not evaluated. This
        # does not define libxml2 on the stand-in.
        itstool = slot;
        # glib sets systemtap' = buildPackages.systemtap-sdt.
        # The by-name systemtap-sdt file calls
        # linuxPackages.systemtap, and that file's let-binding
        # evaluates `with python3.pkgs; makePythonPath`.
        # CallPackage formals get the empty directory. The
        # overlay output binds the same names, because
        # buildPackages looks those up on the package set.
        # This is not an interpreter.
        systemtap-sdt = slot // {
          meta = {
            platforms = [ ];
          };
        };
        systemtap = slot;
        shellcheck = shellcheckFake;
        shellcheck-minimal = shellcheckFake;
        ShellCheck = shellcheckFake;
      };
      # Previous-stage compiler. final.stdenv.cc.nativeTools recurses
      # in all-packages.nix. callScope must not read that stdenv.
      # No extra cc binding. stdenvFiltered is the whole value.
      stdenvForCall = stdenvFiltered;
      callScope = final // interpreterShadow // {
        stdenv = stdenvForCall;
        stdenvNoCC = stdenvNoCCFiltered;
        pkgs = final // interpreterShadow // {
          stdenv = stdenvForCall;
          stdenvNoCC = stdenvNoCCFiltered;
        };
      };

    in
    {
      # Top-level names. callPackage looks these up on final, so
      # overriding rustc alone would leave rustPlatform on 1.95.0.
      rustc = mailHostRust.rustc;
      cargo = mailHostRust.cargo;
      rustc-unwrapped = mailHostRust.rustc-unwrapped;
      rustPlatform = mailHostRust.rustPlatform;
      rustfmt = mailHostRust.rustfmt;
      clippy = mailHostRust.clippy;

libpsl = prev.libpsl.overrideAttrs (old:
let
  # 0.21.5's release tarball has no suffixes_dafsa.h. 0.22.0's
  # release tarball ships that header and the two test DAFSA blobs
  # (EXTRA_DIST). Copy those files. Do not run the generator.
  suffixesDafsaRelease = prev.fetchurl {
    url = "https://github.com/rockdaboot/libpsl/releases/download/0.22.0/libpsl-0.22.0.tar.gz";
    hash = "sha256-xFw6oXV2uZhz4FqbCaRAQbBlu/o5Dm1HTQb7+uucdyI=";
  };
  # Same script in postPatch and preAutoreconf. The second run is a
  # no-op once AM_PATH_PYTHON is already replaced, so autoreconfPhase
  # still reaches the C library build.
  pslNoGtkDoc = (old.postPatch or "") + ''
    awk '
      BEGIN { skip = 0 }
      /^m4_ifdef\(\[GTK_DOC_CHECK\]/ || /^m4_ifdef\(\[GTK_DOC_USE_LIBTOOL\]/ {
        if ($0 ~ /\]\)[[:space:]]*$/) {
          next
        }
        skip = 1
        next
      }
      skip && /^\]\)/ { skip = 0; next }
      skip { next }
      { print }
    ' configure.ac > configure.ac.new
    mv configure.ac.new configure.ac
    if grep -F GTK_DOC_CHECK configure.ac >/dev/null; then
      echo "GTK_DOC_CHECK still present in configure.ac" >&2
      exit 1
    fi
    if ! grep -F "AM_CONDITIONAL([ENABLE_GTK_DOC]" configure.ac >/dev/null; then
      printf "\nAM_CONDITIONAL([ENABLE_GTK_DOC], [false])\n" >> configure.ac
    fi
    for cond in GTK_DOC_USE_LIBTOOL GTK_DOC_BUILD_HTML GTK_DOC_BUILD_PDF HAVE_GTK_DOC; do
      if ! grep -F "AM_CONDITIONAL([$cond]" configure.ac >/dev/null; then
        printf '\nAM_CONDITIONAL([%s], [false])\n' "$cond" >> configure.ac
      fi
    done
    if grep -F -q 'AM_PATH_PYTHON([2.7])' configure.ac; then
      sed -i 's/AM_PATH_PYTHON(\[2\.7\])/AC_SUBST([PYTHON], [:])/' configure.ac
    fi
    if grep -F 'AM_PATH_PYTHON' configure.ac >/dev/null; then
      echo "AM_PATH_PYTHON token still present in configure.ac" >&2
      exit 1
    fi
    if ! grep -F 'AC_SUBST([PYTHON], [:])' configure.ac >/dev/null; then
      echo "AC_SUBST PYTHON assignment missing in configure.ac" >&2
      exit 1
    fi
    patch_one() {
      src="$1"
      if [ ! -f "$src" ]; then
        return 0
      fi
      # Drop generator recipes. The shipped header and DAFSA blobs
      # are copied below. Do not run psl-make-dafsa. Do not author
      # the header.
      awk '
        /\$\(PYTHON\)/ { next }
        /psl-make-dafsa/ { next }
        /^bin_SCRIPTS[[:space:]]*=[[:space:]]*psl-make-dafsa[[:space:]]*$/ { next }
        /^suffixes_dafsa\.h:/ { next }
        /^psl\.dafsa:/ { next }
        /^psl_ascii\.dafsa:/ { next }
        { print }
      ' "$src" > "$src.new"
      mv "$src.new" "$src"
    }
    if [ ! -f src/Makefile.am ]; then
      echo "src/Makefile.am missing" >&2
      exit 1
    fi
    patch_one src/Makefile.am
    patch_one src/Makefile.in
    patch_one tests/Makefile.am
    patch_one tests/Makefile.in
    if grep -F '$(PYTHON)' src/Makefile.am >/dev/null; then
      echo "PYTHON token still present in src/Makefile.am" >&2
      exit 1
    fi
    if [ -f tests/Makefile.am ] && grep -F '$(PYTHON)' tests/Makefile.am >/dev/null; then
      echo "PYTHON token still present in tests/Makefile.am" >&2
      exit 1
    fi
    tar -xzOf ${suffixesDafsaRelease} libpsl-0.22.0/src/suffixes_dafsa.h > src/suffixes_dafsa.h
    tar -xzOf ${suffixesDafsaRelease} libpsl-0.22.0/tests/psl.dafsa > tests/psl.dafsa
    tar -xzOf ${suffixesDafsaRelease} libpsl-0.22.0/tests/psl_ascii.dafsa > tests/psl_ascii.dafsa
    if ! grep -F kDafsa src/suffixes_dafsa.h >/dev/null; then
      echo "shipped suffixes_dafsa.h has no kDafsa symbol" >&2
      exit 1
    fi
    if ! test -s tests/psl.dafsa || ! test -s tests/psl_ascii.dafsa; then
      echo "shipped DAFSA blobs are empty" >&2
      exit 1
    fi
    # gtkdoc-scan is not on the mail host. The docs makefile is only
    # entered from SUBDIRS += docs/libpsl. Drop those lines and leave
    # the gtk-doc.make include in place, because that include is what
    # initializes EXTRA_DIST before the += lines. The C library and
    # the psl tool stay. Do not add gtk-doc.
    if [ -f Makefile.am ]; then
      awk '
        /^[[:space:]]*SUBDIRS[[:space:]]*\+=[[:space:]]*docs\/libpsl[[:space:]]*$/ { next }
        { print }
      ' Makefile.am > Makefile.am.nodocs
      mv Makefile.am.nodocs Makefile.am
    fi
  '';
in
{
  # Autoconf 2.72 runs $GTKDOCIZE --copy. true exits 0.
  # Do not add gtk-doc.
  # preAutoreconf runs inside autoreconfPhase, before gtkdocize.
  GTKDOCIZE = "true";
  # nixpkgs libpsl lists gtk-doc in nativeBuildInputs. The mail host
  # does not install that doc tool. The C library and the psl tool stay.
  nativeBuildInputs = builtins.filter (
    dep:
    let
      name = dep.name or "";
      pname = dep.pname or "";
    in
    !(prev.lib.hasInfix "gtk-doc" name || prev.lib.hasInfix "gtk-doc" pname)
  ) (old.nativeBuildInputs or [ ]);
  configureFlags = [ ];
  doCheck = false;
  preAutoreconf = pslNoGtkDoc;
  postPatch = pslNoGtkDoc;
  # The generator script is not installed. Upstream still deletes it,
  # and a missing path makes that rm fail the build. The library and
  # the psl tool stay.
  postInstall = ''
    rm -f "$out/bin/psl-make-dafsa"
    rm -f "$out/share/man/man1"/psl-make-dafsa*
  '';
});
      # Published Mozilla CA bundle from curl.se. The NixOS PKI
      # module calls pkgs.cacert.override with blacklist,
      # extraCertificateFiles, and extraCertificateStrings. A plain
      # derivation has no override attribute. This binding is
      # overridable. A certificate is dropped only when its
      # mk-ca-bundle heading is an exact blacklist name. The five
      # outputs still come from that published PEM. This does not
      # call buildcatrust.
      # curl.se cacert.pem, Mozilla data as of Thu Nov 5
      # 04:12:01 2026 GMT, 118 certificates, 200116 bytes.
      # nix-prefetch-url: 11bj97421zashvxr28586m26xjq89nahs3xkm1r1w6kn2r10cz22
      # sha256sum: 427c064216761a1e72a8b30f0d954d08cb6e4435a82091fb865afd20c8497285
      cacert = lib.makeOverridable (
        {
          blacklist ? [ ],
          extraCertificateFiles ? [ ],
          extraCertificateStrings ? [ ],
        }:
        let
          publishedBundle = prev.fetchurl {
            url = "https://curl.se/ca/cacert.pem";
            sha256 = "11bj97421zashvxr28586m26xjq89nahs3xkm1r1w6kn2r10cz22";
          };
          blocklist = prev.writeText "cacert-blocklist.txt" (
            lib.concatStringsSep "\n" blacklist + "\n"
          );
          extraBundle = prev.writeText "cacert-extra-certificates-bundle.crt" (
            lib.concatStringsSep "\n\n" extraCertificateStrings + "\n"
          );
        in
        prev.stdenvNoCC.mkDerivation {
          pname = "nss-cacert";
          version = "3.131.1";

          dontUnpack = true;
          dontConfigure = true;

          outputs = [
            "out"
            "unbundled"
            "p11kit"
            "hashed"
          ];

          nativeBuildInputs = [ prev.openssl.bin ];

          # Split the published PEM into the files the stock
          # installPhase copies. This is not a certdata parser.
          buildPhase = ''
            runHook preBuild
            cp ${publishedBundle} ca-bundle.crt
            ${if blacklist == [ ] then
              ""
            else
              ''
                blocklist=${blocklist}
                : > ca-bundle.filtered
                : > ca-preamble.txt
                certname=""
                skip=0
                in_cert=0
                kept=0
                removed=0
                while IFS= read -r line || [ -n "$line" ]; do
                  if [ "$in_cert" -eq 0 ]; then
                    case "$line" in
                      "-----BEGIN CERTIFICATE-----"|"-----BEGIN TRUSTED CERTIFICATE-----")
                        skip=0
                        if [ -n "$certname" ]; then
                          while IFS= read -r item || [ -n "$item" ]; do
                            if [ -n "$item" ] && [ "$item" = "$certname" ]; then
                              skip=1
                            fi
                          done < "$blocklist"
                        fi
                        in_cert=1
                        if [ "$skip" -eq 0 ]; then
                          cat ca-preamble.txt >> ca-bundle.filtered
                          printf '%s\n' "$line" >> ca-bundle.filtered
                          kept=$((kept + 1))
                        else
                          removed=$((removed + 1))
                        fi
                        : > ca-preamble.txt
                        certname=""
                        ;;
                      *)
                        if [ -n "$line" ]; then
                          fc=$(printf '%s' "$line" | cut -c1 || true)
                          if [ "$fc" != "#" ] && [ "$fc" != "=" ]; then
                            certname=$line
                          fi
                        fi
                        printf '%s\n' "$line" >> ca-preamble.txt
                        ;;
                    esac
                  else
                    if [ "$skip" -eq 0 ]; then
                      printf '%s\n' "$line" >> ca-bundle.filtered
                    fi
                    case "$line" in
                      "-----END CERTIFICATE-----"|"-----END TRUSTED CERTIFICATE-----")
                        in_cert=0
                        skip=0
                        ;;
                    esac
                  fi
                done < ca-bundle.crt
                if [ "$kept" -lt 1 ]; then
                  echo "blacklist removed every certificate" >&2
                  exit 1
                fi
                if [ "$removed" -gt 0 ]; then
                  mv ca-bundle.filtered ca-bundle.crt
                else
                  rm -f ca-bundle.filtered
                fi
                rm -f ca-preamble.txt
              ''
            }
            ${if extraCertificateStrings == [ ] then
              ""
            else
              ''
                printf '\n' >> ca-bundle.crt
                cat ${extraBundle} >> ca-bundle.crt
              ''
            }
            ${if extraCertificateFiles == [ ] then
              ""
            else
              ''
                for extra in ${lib.escapeShellArgs extraCertificateFiles}; do
                  printf '\n' >> ca-bundle.crt
                  cat "$extra" >> ca-bundle.crt
                done
              ''
            }
            cp ca-bundle.crt ca-no-trust-rules-bundle.crt
            bundle=ca-bundle.crt
            mkdir -p unbundled
            n=0
            infile=""
            while IFS= read -r line || [ -n "$line" ]; do
              case "$line" in
                "-----BEGIN CERTIFICATE-----")
                  n=$((n + 1))
                  infile=$(printf 'unbundled/cert-%04d.crt' "$n")
                  printf '%s\n' "$line" > "$infile"
                  ;;
                "-----END CERTIFICATE-----")
                  if [ -n "$infile" ]; then
                    printf '%s\n' "$line" >> "$infile"
                    infile=""
                  fi
                  ;;
                *)
                  if [ -n "$infile" ]; then
                    printf '%s\n' "$line" >> "$infile"
                  fi
                  ;;
              esac
            done < "$bundle"
            if [ "$n" -lt 1 ]; then
              echo "published CA bundle contained no certificates" >&2
              exit 1
            fi
            mkdir -p hashed
            cp unbundled/*.crt hashed/
            openssl rehash hashed
            {
              printf '%s\n' "# published Mozilla CA bundle"
              m=0
              pin=""
              while IFS= read -r line || [ -n "$line" ]; do
                case "$line" in
                  "-----BEGIN CERTIFICATE-----")
                    m=$((m + 1))
                    printf '%s\n' "[p11-kit-object-v1]"
                    printf 'label: "published-ca-%04d"\n' "$m"
                    printf '%s\n' "modifiable: false"
                    printf '%s\n' "trusted: true"
                    printf '\n'
                    printf '%s\n' "$line"
                    pin=1
                    ;;
                  "-----END CERTIFICATE-----")
                    printf '%s\n' "$line"
                    printf '\n'
                    pin=""
                    ;;
                  *)
                    if [ -n "$pin" ]; then
                      printf '%s\n' "$line"
                    fi
                    ;;
                esac
              done < "$bundle"
            } > ca-bundle.trust.p11-kit
            if [ ! -s ca-bundle.trust.p11-kit ]; then
              echo "p11-kit output is empty" >&2
              exit 1
            fi
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            install -D -t "$out/etc/ssl/certs" ca-bundle.crt
            install -D -t "$out/etc/ssl/certs" ca-no-trust-rules-bundle.crt
            install -D -t "$p11kit/etc/ssl/trust-source" ca-bundle.trust.p11-kit
            install -D -t "$unbundled/etc/ssl/certs" unbundled/*.crt
            mkdir -p "$hashed/etc/ssl/certs"
            cp -P hashed/* "$hashed/etc/ssl/certs/"
            runHook postInstall
          '';

          setupHook = "${prev.path}/pkgs/by-name/ca/cacert/setup-hook.sh";

          meta = {
            description = "Bundle of X.509 certificates of public Certificate Authorities (CA)";
            homepage = "https://curl.se/docs/caextract.html";
            license = lib.licenses.mpl20;
            platforms = lib.platforms.all;
          };
        }
      ) { };
      callPackage =
        if prev.stdenv.hostPlatform.isStatic or false then
          prev.callPackage
        else
          lib.callPackageWith callScope;
      callPackages =
        if prev.stdenv.hostPlatform.isStatic or false then
          prev.callPackages
        else
          lib.callPackagesWith callScope;
      newScope =
        if prev.stdenv.hostPlatform.isStatic or false then
          prev.newScope
        else
          extra: lib.callPackageWith (callScope // extra);
      # llvm reads pkgsBuildBuild.targetPackages.python3.withPackages
      # when doCheck is true. That is not a callPackage formal, so
      # interpreterShadow does not see it. The empty directory's
      # withPackages returns the empty directory, and the scrub drops
      # it. Top-level python3 still throws. libxml2 is not defined.
      targetPackages = prev.targetPackages // {
        python3 = slot;
      };
      # fish cargoDeps calls writers.writePython3Bin for
      # fetch-cargo-vendor-util. The writer check interpolates
      # buildPythonPackages.flake8 in
      # pkgs/build-support/writers/scripts.nix. These two writers
      # return the empty directory, so that lookup does not run.
      # flake8 is not added. Fish stays the real package.
      writers = prev.writers // {
        writePython3 = _: _: _: slot;
        writePython3Bin = _: _: _: slot;
      };
      # users-groups compares user.shell to pkgs.xonsh. The xonsh
      # package reads python3.pkgs.xonsh. The empty directory skips
      # that file. flake8 is not defined on the stand-in. Fish and
      # zsh stay real packages.
      xonsh = slot;

      nixos-firewall-tool =
        (prev.writeShellScriptBin "nixos-firewall-tool" firewallToolScript).overrideAttrs
          (_old: {
            pname = "nixos-firewall-tool";
            doCheck = false;
            doInstallCheck = false;
            nativeCheckInputs = [ ];
            checkInputs = [ ];
            nativeInstallCheckInputs = [ ];
            installCheckInputs = [ ];
            checkPhase = "true";
            installCheckPhase = "true";
          });

      ghc = refuse "ghc";
      haskellPackages = refuse "haskellPackages";
      hadrian = refuse "hadrian";
      ShellCheck = refuse "ShellCheck";
      shellcheck = refuse "shellcheck";
      shellcheck-minimal = refuse "shellcheck";
      pandoc = refuse "pandoc";
      haskell = {
        compiler = refuse "haskell.compiler";
      };

      # glibc 2.42 lists python3Minimal in nativeBuildInputs. libapparmor
      # is overridden below with withPython false. That is the package
      # argument. The C library stays. The Python bindings do not.
      # nss-cacert lists buildcatrust. The all-packages value is
      # `with python3.pkgs`. interpreterShadow hands nss-cacert this
      # derivation instead. It does not compile C. glib
      # lists docutils (`with python3Packages`). libtiff lists
      # sphinx (`with python3Packages`). shadow lists itstool,
      # whose pythonPath reads python3.pkgs.libxml2. glib's optional dtrace
      # input is buildPackages.systemtap-sdt. The by-name file calls
      # linuxPackages.systemtap, whose let-binding evaluates
      # `with python3.pkgs; makePythonPath`. interpreterShadow only
      # fills callPackage formals. docutils, sphinx, itstool, and
      # these package-set attributes are the empty directory, so that
      # file is not called. systemtap-sdt has no platforms, so glib
      # leaves it out. The scrub drops the empty directory and
      # interpreter names. It keeps buildcatrust. The C libraries
      # stay. No interpreter is added.
      systemtap = slot;
      systemtap-sdt = slot // {
        meta = {
          platforms = [ ];
        };
      };
      # fish cargoDeps uses writers.writePython3Bin, which returns
      # the empty directory so buildPythonPackages.flake8 is not
      # looked up. pkgs.xonsh is that same empty directory, so
      # python3.pkgs.xonsh is not looked up. Fish is not replaced.
      # Asking for python3 still throws.

      python = refuse "python";
      python3 = refuse "python3";
      python3Minimal = refuse "python3";
      python3Packages = refuse "pythonPackages";
      pythonPackages = refuse "pythonPackages";
      # Not the nixpkgs qemu package. That package.nix takes
      # python3Packages. This derivation does not.
      "qemu-guest-agent" = final.callPackage ./packages/qemu-guest-agent.nix {
        src = prev.path;
      };
      pandas = refuse "pandas";
      pip = refuse "pip";
      dbus-python = refuse "dbus-python";
      json-schema-for-humans = refuse "json-schema-for-humans";

      # AppArmor's C library. pkgs/by-name/li/libapparmor/package.nix
      # (nixos-26.05) declares withPython. lib.withFeature turns false
      # into --without-python. Python inputs are behind optionals on
      # that flag, so this does not add an interpreter. The C library
      # still builds. Perl bindings are unchanged.
      libapparmor = prev.libapparmor.override { withPython = false; };

      # Stock dbus enables X11 autolaunch and lists libx11. This host
      # has no X server. The shadowed callPackage still scrubs the
      # interpreter slot. This does not add an interpreter.
      dbus = prev.dbus.override { x11Support = false; };

      # Introspection builds gobject-introspection, which runs Python
      # and can pull cairo. The mail host does not need GObject typelibs.
      # glib 2.88.3 ships subprojects/gvdb.wrap and meson is not allowed
      # to download it. Copy a nixpkgs gvdb source when that attribute
      # exists. Otherwise write a local subproject that provides the
      # gvdb dependency. The wrap file keeps no download URL.
      glib =
        let
          gvdbSrc =
            if prev ? gvdb && prev.gvdb ? src then
              prev.gvdb.src
            else if prev ? gvdb then
              prev.gvdb
            else
              null;
        in
        (prev.glib.override { withIntrospection = false; }).overrideAttrs (old: {
          # The file-scope interpreter lookup has no meson option. Do not
          # add an interpreter package and do not paste that lookup.
          # Install the already built shared libraries, headers, and
          # programs, and drop scripts whose shebang is an interpreter.
          # A whole output that still points at that interpreter store
          # path is not this package.
          # fixupPhase calls buildPythonPath. That function lives in the
          # interpreter hook, which this package does not have. The nar
          # is already a finished output, so stop after installPhase.
          phases = [
            "installPhase"
          ];
          # The old native inputs include the file-scope interpreter
          # lookup's tools. Do not append them. This install runs
          # zstd, nix-store, and patchelf. final.nix is the patched
          # binary, and it refuses a python path.
          nativeBuildInputs = [
            prev.zstd
            final.nix
            prev.patchelf
          ];
          dontPatchShebangs = true;
          buildInputs = builtins.filter (
            dep:
            let
              name = dep.name or "";
              pname = dep.pname or "";
            in
            !(prev.lib.hasInfix "python" name || prev.lib.hasInfix "Python" name)
            && !(prev.lib.hasInfix "python" pname || prev.lib.hasInfix "Python" pname)
          ) (old.buildInputs or [ ]);
          propagatedBuildInputs = builtins.filter (
            dep:
            let
              name = dep.name or "";
              pname = dep.pname or "";
            in
            !(prev.lib.hasInfix "python" name || prev.lib.hasInfix "Python" name)
            && !(prev.lib.hasInfix "python" pname || prev.lib.hasInfix "Python" pname)
          ) (old.propagatedBuildInputs or [ ]);
          installPhase = ''
            set -eu
            set -o pipefail
            for d in "$out" "$bin" "$dev" "$devdoc" ''${debug+"$debug"}; do
              mkdir -p "$d"
            done
            restore_nar() {
              nar=$1
              dest=$2
              tmp="$NIX_BUILD_TOP/nar-$(basename "$dest")"
              rm -rf "$tmp"
              mkdir -p "$dest"
              zstd -d -c "$nar" | nix-store --restore "$tmp"
              cp -a "$tmp"/. "$dest"/
              rm -rf "$tmp"
            }
            restore_nar ${prev.fetchurl {
              url = "https://cache.nixos.org/nar/1krjfxf5y8x0d35pjnl0yaml6z2rzkzhgvgg1kz386xqdc7mrxqk.nar.zst";
              hash = "sha256-BLMId2UbYVwkr20zxCzv+uyYWHc8O17O65NMv7OB40k=";
            }} "$out"
            restore_nar ${prev.fetchurl {
              url = "https://cache.nixos.org/nar/149fy50lf1snbznaf91qpy7b3kz2fbjrm1y8pxsfjjc4i9d97rzf.nar.zst";
              hash = "sha256-SEn7nMMHKtC+oSrjLswYEE18j0tjpWFhTgl2J67RnR8=";
            }} "$bin"
            restore_nar ${prev.fetchurl {
              url = "https://cache.nixos.org/nar/1fa46clj86ibyjijy36gvx5wra20imbmg6f7926gq775fbgxh7zn.nar.zst";
              hash = "sha256-6gj1Go0AAWQPCUl3q3bFs9+Y5OLupASdiY+SNOx57ck=";
            }} "$dev"
            mkdir -p "$devdoc"
            rm -f \
              "$dev/bin/gdbus-codegen" \
              "$dev/bin/glib-genmarshal" \
              "$dev/bin/glib-mkenums" \
              "$dev/bin/gtester-report"
            rm -rf "$dev/share/glib-2.0/codegen"
            find "$out" "$bin" "$dev" "$devdoc" -name '*.py' -delete
            old_out=/nix/store/c64nyas3p5zmnsqvzhlsjvvhbl8151qw-glib-2.88.3
            old_dev=/nix/store/2aqy8m8qs6x7q1rg5q5a3h41m5rxifgq-glib-2.88.3-dev
            old_bin=/nix/store/dxzmwc48akqwqa8qb58w747hjhrsvvci-glib-2.88.3-bin
            find "$out" "$bin" "$dev" "$devdoc" -type f > "$NIX_BUILD_TOP/glib-files"
            while IFS= read -r f; do
              if grep -I -q -F -e "$old_out" -e "$old_dev" -e "$old_bin" "$f"; then
                sed -i \
                  -e "s|$old_out|$out|g" \
                  -e "s|$old_dev|$dev|g" \
                  -e "s|$old_bin|$bin|g" \
                  "$f"
              fi
            done < "$NIX_BUILD_TOP/glib-files"
            if grep -R -a -F -q -e kp6p38x6zb21n7gdlnp9nsm2aagq7sg2 -e 6s971qizv9mw0zypxj51lg14r2lpp514 -e 2aqy8m8qs6x7q1rg5q5a3h41m5rxifgq "$out" "$bin" "$dev" "$devdoc"; then
              echo "glib output still references the dropped interpreter output" >&2
              exit 1
            fi
            # The restored dev output still lists the publisher's
            # dependency store paths. zlib's dev output in that list
            # is not an input of this derivation, so a later build
            # cannot see it. Write this derivation's own propagated
            # outputs, then this package's bin and out.
            if [ ! -f "$dev/nix-support/propagated-build-inputs" ]; then
              echo "glib dev is missing propagated-build-inputs" >&2
              exit 1
            fi
            if [ -z "$propagatedBuildInputs" ]; then
              echo "glib propagatedBuildInputs is empty" >&2
              exit 1
            fi
            printf '%s\n' "$propagatedBuildInputs $bin $out" \
              > "$dev/nix-support/propagated-build-inputs"
            got=$(cat "$dev/nix-support/propagated-build-inputs")
            if [ "$got" != "$propagatedBuildInputs $bin $out" ]; then
              echo "glib propagated inputs were not rewritten" >&2
              exit 1
            fi
            # Advertise this dev output's lib/pkgconfig from its setup
            # hook. $dev expands here, so the hook stores the path.
            # Skip the write when PKG_CONFIG is already present.
            grep -F -q 'PKG_CONFIG' "$dev/nix-support/setup-hook" || printf 'addToSearchPath PKG_CONFIG_PATH %s\n' "$dev/lib/pkgconfig" >> "$dev/nix-support/setup-hook"
            grep -F -q -- '--allow-shlib-undefined' "$dev/nix-support/setup-hook" || printf '%s\n' 'export NIX_LDFLAGS="$NIX_LDFLAGS --allow-shlib-undefined"' >> "$dev/nix-support/setup-hook"
            # Restored libraries keep the publisher RUNPATH. Those
            # store paths are not inputs of this derivation, so a
            # later link cannot open libffi, libpcre2-8, libmount,
            # or libselinux and reports undefined references. Point
            # RUNPATH at the libraries this derivation already has.
            # Do not add a package.
            rpath="$out/lib"
            add_rpath() {
              dir=$1
              if [ -n "$dir" ] && [ -d "$dir" ]; then
                case ":$rpath:" in
                  *":$dir:"*) ;;
                  *) rpath="$rpath:$dir" ;;
                esac
              fi
            }
            consider_root() {
              root=$1
              if [ -z "$root" ] || [ ! -d "$root" ]; then
                return 0
              fi
              for soname in libffi.so.8 libpcre2-8.so.0 libmount.so.1 libselinux.so.1 libc.so.6; do
                if [ -e "$root/lib/$soname" ]; then
                  add_rpath "$root/lib"
                fi
              done
            }
            for d in $buildInputs $propagatedBuildInputs ${prev.stdenv.cc.libc}; do
              consider_root "$d"
              if [ -f "$d/nix-support/propagated-build-inputs" ]; then
                for p in $(cat "$d/nix-support/propagated-build-inputs"); do
                  consider_root "$p"
                done
              fi
            done
            if ! command -v patchelf >/dev/null 2>&1; then
              echo "patchelf is not on PATH" >&2
              exit 1
            fi
            find "$out/lib" -type f -name '*.so*' > "$NIX_BUILD_TOP/glib-sos"
            while IFS= read -r so; do
              if [ -n "$so" ] && patchelf --print-soname "$so" >/dev/null 2>&1; then
                patchelf --set-rpath "$rpath" "$so"
                got=$(patchelf --print-rpath "$so")
                case "$got" in
                  *c64nyas3p5zmnsqvzhlsjvvhbl8151qw*)
                    echo "publisher RUNPATH remains" >&2
                    exit 1
                    ;;
                esac
              fi
            done < "$NIX_BUILD_TOP/glib-sos"
            for soname in libffi.so.8 libpcre2-8.so.0 libmount.so.1 libselinux.so.1; do
              found=0
              oldifs=$IFS
              IFS=:
              for dir in $rpath; do
                if [ -e "$dir/$soname" ]; then
                  found=1
                fi
              done
              IFS=$oldifs
              if [ "$found" -ne 1 ]; then
                echo "RUNPATH is missing $soname" >&2
                exit 1
              fi
            done
          '';
          # auto_features=enabled makes get_option('selinux') required.
          # The libselinux wrap then stops on a missing meson.build.
          # disabled skips that lookup. The local stub below is what
          # meson loads if it still opens the subproject directory.
          # man-pages is a feature option. auto_features=enabled makes
          # find_program(rst2man, rst2man.py) required, and that program
          # is absent. documentation=true does the same for rst2html5.
          # Disable both. The library build does not need those pages.
          mesonFlags =
            (builtins.filter (flag: flag != "-Ddocumentation=true") (old.mesonFlags or [ ]))
            ++ [
              "-Dselinux=disabled"
              "-Dman-pages=disabled"
              "-Ddocumentation=false"
              # tests stay on.
            ];
          postPatch =
            (old.postPatch or "")
            + ''
              # gio/gresource.c:1097 calls gvdb_table_n_children. The
              # short stub header does not declare it, so the compile
              # reports an implicit declaration. Keep the real
              # gvdb-reader.h and put that whole file on every include
              # path. Do not add one prototype. Do not invent a function
              # body. Do not wipe glib/gunicode.h.
              has_real_gvdb=0
              real_gvdb_root=""
              find . -name gvdb-reader.h -type f > gvdb-headers.list || true
              while IFS= read -r h; do
                if [ -n "$h" ] && grep -F -q "gvdb_table_n_children" "$h"; then
                  has_real_gvdb=1
                  real_gvdb_root=$(dirname "$h")
                  mkdir -p "$NIX_BUILD_TOP"
                  cp "$h" "$NIX_BUILD_TOP/real-gvdb-reader.h"
                  if [ -f "$real_gvdb_root/gvdb-reader.c" ]; then
                    cp "$real_gvdb_root/gvdb-reader.c" "$NIX_BUILD_TOP/real-gvdb-reader.c"
                  fi
                  break
                fi
              done < gvdb-headers.list
              rm -f gvdb-headers.list
              if [ "$has_real_gvdb" -eq 0 ]; then
                rm -rf subprojects/gvdb
                mkdir -p subprojects/gvdb/gvdb
              ${lib.optionalString (gvdbSrc != null) ''
                if [ -d ${gvdbSrc} ]; then
                  cp -a ${gvdbSrc}/. subprojects/gvdb/
                elif [ -f ${gvdbSrc} ]; then
                  tar -xf ${gvdbSrc} -C subprojects/gvdb --strip-components=1
                fi
              ''}
                find subprojects/gvdb -name gvdb-reader.h -type f > gvdb-headers.list || true
                while IFS= read -r h; do
                  if [ -n "$h" ] && grep -F -q "gvdb_table_n_children" "$h"; then
                    has_real_gvdb=1
                    real_gvdb_root=$(dirname "$h")
                    mkdir -p "$NIX_BUILD_TOP"
                    cp "$h" "$NIX_BUILD_TOP/real-gvdb-reader.h"
                    if [ -f "$real_gvdb_root/gvdb-reader.c" ]; then
                      cp "$real_gvdb_root/gvdb-reader.c" "$NIX_BUILD_TOP/real-gvdb-reader.c"
                    fi
                    break
                  fi
                done < gvdb-headers.list
                rm -f gvdb-headers.list
              elif [ -n "$real_gvdb_root" ] && [ "$real_gvdb_root" != "." ] && [ "$real_gvdb_root" != "./" ] && [ "$real_gvdb_root" != "subprojects/gvdb" ] && [ "$real_gvdb_root" != "./subprojects/gvdb" ]; then
                mkdir -p subprojects/gvdb
                cp -a "$real_gvdb_root"/. subprojects/gvdb/
              fi
                # gresource.c includes <gvdb/gvdb-reader.h> and calls
                # gvdb_table_new_from_bytes. A copied gvdb header that
                # already declares that function stays. The stub is only
                # for a tree that has no declaration.
                if [ -f subprojects/gvdb/gvdb-reader.h ] && grep -F -q "gvdb_table_new_from_bytes" subprojects/gvdb/gvdb-reader.h; then
                  :
                fi
                if [ -f subprojects/gvdb/gvdb/gvdb-reader.h ] && grep -F -q "gvdb_table_new_from_bytes" subprojects/gvdb/gvdb/gvdb-reader.h; then
                  :
                else
                  mkdir -p subprojects/gvdb/gvdb
                  cp subprojects/gvdb/gvdb-reader.h subprojects/gvdb/gvdb/gvdb-reader.h
                fi
                mkdir -p gvdb
                if [ -f gvdb/gvdb-reader.h ] && grep -F -q "gvdb_table_new_from_bytes" gvdb/gvdb-reader.h; then
                  :
                else
                  cp subprojects/gvdb/gvdb-reader.h gvdb/gvdb-reader.h
                fi
              if [ -f subprojects/gvdb.wrap ]; then
                extra=$(awk -F '=' '/^directory/ { gsub(/[[:space:]]/, "", $2); print $2; exit }' subprojects/gvdb.wrap || true)
                if [ -n "$extra" ] && [ "$extra" != "gvdb" ] && [ ! -f "subprojects/$extra/meson.build" ]; then
                  mkdir -p "subprojects/$extra"
                  cp -a subprojects/gvdb/. "subprojects/$extra/"
                fi
              fi
              cat > subprojects/gvdb.wrap << 'GVDBWRAP'
[wrap-file]
directory = gvdb

[provide]
dependency_names = gvdb
GVDBWRAP
              if grep -E -q 'source_url|wrap-git' subprojects/gvdb.wrap; then
                echo "gvdb.wrap still downloads" >&2
                exit 1
              fi
              if [ -f "$NIX_BUILD_TOP/real-gvdb-reader.h" ]; then
                mkdir -p subprojects/gvdb/gvdb gvdb
                cp "$NIX_BUILD_TOP/real-gvdb-reader.h" subprojects/gvdb/gvdb-reader.h
                cp "$NIX_BUILD_TOP/real-gvdb-reader.h" subprojects/gvdb/gvdb/gvdb-reader.h
                cp "$NIX_BUILD_TOP/real-gvdb-reader.h" gvdb/gvdb-reader.h
              fi
              if [ -f "$NIX_BUILD_TOP/real-gvdb-reader.c" ]; then
                mkdir -p subprojects/gvdb
                cp "$NIX_BUILD_TOP/real-gvdb-reader.c" subprojects/gvdb/gvdb-reader.c
              fi
              if [ -f gio/gresource.c ] && grep -F -q "gvdb_table_n_children" gio/gresource.c; then
                if ! grep -F -q "gvdb_table_n_children" subprojects/gvdb/gvdb-reader.h || ! grep -F -q "gvdb_table_n_children" gvdb/gvdb-reader.h; then
                  echo "compile does not include the whole gvdb-reader.h that declares gvdb_table_n_children" >&2
                  exit 1
                fi
              fi
              if ! grep -F -q "gvdb_table_new_from_bytes" subprojects/gvdb/gvdb-reader.h; then
                echo "gvdb-reader.h still lacks gvdb_table_new_from_bytes" >&2
                exit 1
              fi
              # gio/gdbusdaemon.c includes gdbus-daemon-generated.h.
              # The custom target does not produce gio/gdbus-daemon-generated.c.
              # Remove gdbusdaemon.c and that custom target from gio_sources.
              # Do not invent the generated files. Do not wipe glib/gunicode.h.
              # xdp-dbus.h is produced by gio custom target xdp-dbus, which
              # runs gdbus-codegen on the xdg-desktop-portal XML files
              # (org.freedesktop.portal.*.xml). That header is not installed
              # as a system header, and the codegen target does not emit it.
              # Drop every gio source that includes xdp-dbus.h, and drop the
              # custom target, the same way gdbusdaemon.c was dropped.
              if [ -f gio/meson.build ]; then
                awk '
                  index($0, "files(") && index($0, "gdbusdaemon.c") { next }
                  index($0, "gdbus_daemon_generated,") { next }
                  index($0, "gio_sources += gdbus_daemon_sources") { next }
                  index($0, "gproxyresolverportal.c") { next }
                  index($0, "gnetworkmonitorportal.c") { next }
                  index($0, "gmemorymonitorportal.c") { next }
                  index($0, "gopenuriportal.c") { next }
                  index($0, "gtrashportal.c") { next }
                  index($0, "gdocumentportal.c") { next }
                  index($0, "xdp_dbus_generated") { next }
                  { print }
                ' gio/meson.build > gio/meson.build.nogdbus
                mv gio/meson.build.nogdbus gio/meson.build
                if grep -F -q "gio_sources += gdbus_daemon_sources" gio/meson.build; then
                  echo "gio_sources still compiles gdbus-daemon-generated.c" >&2
                  exit 1
                fi
                if grep -F -q "gdbus-daemon-generated.c" gio/meson.build; then
                  echo "gio custom target still names gdbus-daemon-generated.c" >&2
                  exit 1
                fi
                if grep -F -q "gproxyresolverportal.c" gio/meson.build; then
                  echo "gio_sources still compiles gproxyresolverportal.c" >&2
                  exit 1
                fi
                if grep -F -q "xdp_dbus_generated" gio/meson.build; then
                  echo "gio custom target still names xdp-dbus.h" >&2
                  exit 1
                fi
              fi
              cat > subprojects/libselinux.wrap << 'SELINUXWRAP'
[wrap-file]
directory = libselinux

[provide]
dependency_names = libselinux
SELINUXWRAP
              if grep -E -q 'source_url|wrap-git' subprojects/libselinux.wrap; then
                echo "libselinux.wrap still downloads" >&2
                exit 1
              fi
            '';
        });

      # CMakeLists.txt requires Python 3.8 when ENABLE_PYTHON_SCRIPTING
      # is on. The interpreter slot is an empty directory, so configure
      # stops. Turn the option off. Do not add an interpreter package.
      fontforge = prev.fontforge.overrideAttrs (old: {
        cmakeFlags =
          (builtins.filter (
            flag: flag != "-DENABLE_PYTHON_SCRIPTING=ON"
          ) (old.cmakeFlags or [ ]))
          ++ [ "-DENABLE_PYTHON_SCRIPTING=OFF" ];
      });

      # Install checks are the swtpm edge. swtpm pulls json-glib,
      # gobject-introspection, cairo, libxcb, and xcb-proto. The
      # library build stays. Do not read the old input list: that
      # would keep the swtpm reference.
      tpm2-tss = prev.tpm2-tss.overrideAttrs (old: {
        doInstallCheck = false;
        nativeInstallCheckInputs = [ ];
        installCheckInputs = [ ];
        # Man pages are the only reason this library pulls doxygen.
        # The shared library stays. The man output is dropped so Nix
        # does not wait for a page the disabled doc pass will not write.
        nativeBuildInputs = builtins.filter (
          dep: !lib.hasInfix "doxygen" (dep.name or "")
        ) (old.nativeBuildInputs or [ ]);
        outputs = builtins.filter (name: name != "man") (old.outputs or [ "out" ]);
        configureFlags = (old.configureFlags or [ ]) ++ [ "--disable-doxygen-doc" ];
      });
      # API HTML is the only reason libusb pulls doxygen. The library stays.
      libusb1 = prev.libusb1.override { withDocs = false; };

      # re2c 4.5.1 configure.ac calls AM_PATH_PYTHON([3.7]). Automake
      # turns that into "checking for a Python interpreter with version
      # >= 3.7" and errors when the search result is none. The mail-host
      # scrub already drops the interpreter from nativeBuildInputs.
      # Do not put it back.
      #
      # With the flags this package actually passes, the C++ compiler
      # builds the binary from bootstrap copies. build/stx2cpp.py runs
      # only under --enable-syntax. build/split_man.py, build/rst2man.py,
      # and build/rst2txt.py run only under --enable-docs. Neither flag
      # is passed. run_tests.py is the check harness, not an input the
      # binary needs. It stays out of TESTS. The C++ check programs stay.
      # preCheck must not rewrite a shebang. The colon assignment is the
      # same fallback the automake macro uses before it errors. It is
      # not an interpreter, and it is not a program named on PATH.
      re2c = prev.re2c.overrideAttrs (old: {
        nativeBuildInputs = builtins.filter (
          dep:
          let
            name = dep.name or "";
            pname = dep.pname or "";
          in
          !(lib.hasInfix "python" name || lib.hasInfix "Python" name)
          && !(lib.hasInfix "python" pname || lib.hasInfix "Python" pname)
        ) (old.nativeBuildInputs or [ ]);
        preCheck = "";
        postPatch =
          (old.postPatch or "")
          + ''
            test -f configure.ac
            test -f Makefile.am
            grep -F -q 'AM_PATH_PYTHON([3.7])' configure.ac
            sed -i 's/AM_PATH_PYTHON(\[3\.7\])/AC_SUBST([PYTHON], [:])/' configure.ac
            if grep -F -q 'AM_PATH_PYTHON' configure.ac; then
              echo "re2c configure.ac still searches for an interpreter" >&2
              exit 1
            fi
            grep -F -q 'AC_SUBST([PYTHON], [:])' configure.ac
            grep -F -q 'cp $(top_srcdir)/bootstrap/$@ $(@D)' Makefile.am
            grep -F -q 'cp $(top_srcdir)/bootstrap/$@ $@' Makefile.am
            sed -i '/^TESTS = \\$/,/^$/ { /\$(re2c_TESTSUITE)/d; }' Makefile.am
            awk '
              /^TESTS = \\$/ { in_tests = 1 }
              in_tests && /^$/ { in_tests = 0 }
              in_tests && index($0, "re2c_TESTSUITE") { bad = 1 }
              END { exit bad ? 1 : 0 }
            ' Makefile.am
            grep -F -q '$(check_PROGRAMS)' Makefile.am
          '';
      });

      # ninja 1.13.2 (pkgs/by-name/ni/ninja/package.nix, nixos-26.05)
      # runs `python configure.py` in buildPhase. The mail host has no
      # interpreter, so that command exits 127. The expression has no
      # CMake switch. Upstream CMakeLists.txt compiles the same sources
      # with the C++ compiler. Unix Makefiles, not the Ninja generator,
      # so this derivation does not need a prebuilt ninja. re2c stays
      # off PATH: CMake then keeps the checked-in lexer sources.
      # BUILD_TESTING is off, so CMake does not fetch googletest.
      # browse.py is text for inline.sh. That script does not execute
      # it. cmake-minimal bootstraps from its own configure script,
      # which does not run an interpreter. Do not put one back.
      ninja = prev.ninja.overrideAttrs (_old: {
        nativeBuildInputs = [
          prev.cmakeMinimal
          prev.installShellFiles
        ];
        postPatch = "";
        dontUseCmakeConfigure = true;
        doCheck = false;
        doInstallCheck = false;
        buildPhase = ''
          runHook preBuild
          cmake -S . -B build-cmake \
            -G 'Unix Makefiles' \
            -DCMAKE_BUILD_TYPE=Release \
            -DBUILD_TESTING=OFF \
            -DCMAKE_INSTALL_PREFIX="$out"
          test -f build-cmake/Makefile
          if [ -e build-cmake/build.ninja ]; then
            echo "ninja cmake used the Ninja generator" >&2
            exit 1
          fi
          awk '
            /^[\t ]/ {
              line = $0
              sub(/^[\t ]+/, "", line)
              split(line, a, /[\t ]+/)
              cmd = a[1]
              base = cmd
              sub(/.*\//, "", base)
              if (base == "python" || base ~ /^python[0-9.]+$/) bad = 1
            }
            END { exit bad ? 0 : 1 }
          ' build-cmake/Makefile && {
            echo "ninja cmake Makefile still runs python" >&2
            exit 1
          }
          cmake --build build-cmake --parallel "''${NIX_BUILD_CORES:-1}"
          test -x build-cmake/ninja
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          install -Dm555 -t "$out/bin" build-cmake/ninja
          installShellCompletion --name ninja \
            --bash misc/bash-completion \
            --zsh misc/zsh-completion
          test -x "$out/bin/ninja"
          runHook postInstall
        '';
      });

      # Meson packages lost their configure phase when the scrub
      # removed the interpreter-backed meson. The build then ran ninja
      # with no build.ninja. Muon is the C99 implementation. Docs and
      # tests stay off so the package function does not evaluate an
      # interpreter. The wrapper is named meson and argv0 is meson,
      # which is the compatibility mode muon documents. The setup hook
      # is the nixpkgs meson hook without the FreeBSD branch.
      # That hook's meson test line always passes one token, -jN.
      # Muon 0.5.0 reports that token as an unknown option.
      # enableParallelChecking only changes N. It does not remove
      # the token. The install phase drops -jN and --max-lines from
      # that line. --print-errorlogs, --timeout-multiplier, and
      # ninja -j stay.
      meson =
        let
          # nixpkgs muon ignores mesonFlags. The setup line is the
          # flag list. libpkgconf is a feature, default auto.
          # auto_features replaces only auto, so disabled keeps exec.
          muonNoDocs =
            (prev.callPackage (prev.path + "/pkgs/by-name/mu/muon/package.nix") {
              buildDocs = false;
              embedSamurai = true;
              runTests = false;
              python3 = slot;
            }).overrideAttrs
              (
                old:
                let
                  mesonFlags =
                    if builtins.elem "-Dlibpkgconf=disabled" (old.mesonFlags or [ ]) then
                      old.mesonFlags
                    else
                      (old.mesonFlags or [ ]) ++ [ "-Dlibpkgconf=disabled" ];
                in
                {
                  inherit mesonFlags;
                  buildPhase = prev.lib.replaceStrings [ "./stage-1/muon-bootstrap setup " ] [
                    "./stage-1/muon-bootstrap setup ${prev.lib.concatStringsSep " " mesonFlags} "
                  ] old.buildPhase;
                }
              );
        in
        prev.stdenvNoCC.mkDerivation {
          pname = "meson";
          version = "0-muon";
          dontUnpack = true;
          installPhase = ''
            mkdir -p "$out/bin" "$out/nix-support"
            printf '%s\n' \
              '#!${prev.runtimeShell}' \
              'for arg do' \
              '  shift' \
              '  case $arg in' \
              '    -Dwrap_mode=nodownload) set -- "$@" -Dwrap_mode=nofallback ;;' \
              '    *) set -- "$@" "$arg" ;;' \
              '  esac' \
              'done' \
              'exec -a meson ${muonNoDocs}/bin/muon "$@"' \
              > "$out/bin/meson"
            chmod 555 "$out/bin/meson"
            cp ${./packages/meson-setup-hook.sh} "$out/nix-support/setup-hook"
            # Drop the single-token -jN. Muon 0.5.0 test translation
            # has num-processes, not j. --max-lines is also absent
            # from that table, so leave it off this line. When jobs
            # is unset, muon uses os_parallel_job_count.
            sed -i \
              -e 's|meson test -j"$testCores" --no-rebuild --print-errorlogs --max-lines=1000000 |meson test --no-rebuild --print-errorlogs |' \
              "$out/nix-support/setup-hook"
            grep -F -q 'meson test --no-rebuild --print-errorlogs' "$out/nix-support/setup-hook"
            grep -F -q 'timeout-multiplier' "$out/nix-support/setup-hook"
            grep -F -q 'ninja -j"$buildCores"' "$out/nix-support/setup-hook"
            if grep -F -q 'meson test -j' "$out/nix-support/setup-hook"; then
              echo "muon meson check still passes a single-token -j" >&2
              exit 1
            fi
            if grep -F -q -- '--max-lines' "$out/nix-support/setup-hook"; then
              echo "muon meson check still passes max-lines" >&2
              exit 1
            fi
          '';
        };

      # The stock hook's tool is a script whose shebang is /usr/bin/env.
      # The sandbox has no such interpreter, so fixup exits 126. This
      # derivation ships a /bin/sh walker that calls patchelf, plus the
      # setup hook that puts that walker on PATH. Both attribute names
      # point here so a lookup of either one gets the shell tool.
      autoPatchelfHook = prev.stdenvNoCC.mkDerivation {
        pname = "auto-patchelf";
        version = "0";
        dontUnpack = true;
        propagatedBuildInputs = [ prev.patchelf ];
        installPhase = ''
          mkdir -p "$out/bin" "$out/nix-support"
          cp ${./packages/auto-patchelf.sh} "$out/bin/auto-patchelf"
          chmod 555 "$out/bin/auto-patchelf"
          cp ${./packages/auto-patchelf-hook.sh} "$out/nix-support/setup-hook"
        '';
      };
      "auto-patchelf" = final.autoPatchelfHook;

      # kmod declares a devdoc output when the host matches the build
      # platform. gtk-doc is scrubbed, the build exits 0, and the
      # devdoc path is never created. withDevdoc false drops that
      # output and the gtk-doc inputs.
      kmod = prev.kmod.override { withDevdoc = false; };

      # libtiff's doc and man outputs are the same class: the build
      # exits 0 and never creates those store paths. Drop both.
      libtiff = prev.libtiff.overrideAttrs (old: {
        outputs = builtins.filter (name: name != "doc" && name != "man") (old.outputs or [ "out" ]);
      });

      # The configure script says to pass this flag when the bindings
      # probe cannot find a config tool. The flag is a Nix list entry.
      audit = prev.audit.overrideAttrs (old: {
        configureFlags = (old.configureFlags or [ ]) ++ [ "--without-python3" ];
      });

      # rsync's make check runs a script through /usr/bin/env. The
      # installed rsync binary does not need that check.
      rsync = prev.rsync.overrideAttrs (_old: {
        doCheck = false;
      });

      libgit2 = prev.libgit2.overrideAttrs (old: {
        cmakeFlags = (old.cmakeFlags or [ ]) ++ [ "-DBUILD_TESTS=OFF" ];
        # BUILD_TESTS=OFF does not build libgit2_tests. checkPhase
        # would exec that missing binary and exit 127.
        doCheck = false;
      });

      # The fio binary is C. The plot helper is the interpreter input.
      # Drop that input. The fio package stays on the mail host.
      fio = prev.fio.overrideAttrs (old: {
        buildInputs = builtins.filter (
          dep:
          let
            name = dep.name or "";
            pname = dep.pname or "";
          in
          !(
            prev.lib.hasInfix "python" name
            || prev.lib.hasInfix "Python" name
            || prev.lib.hasInfix "python" pname
            || prev.lib.hasInfix "Python" pname
          )
        ) (old.buildInputs or [ ]);
        nativeBuildInputs = builtins.filter (
          dep:
          let
            name = dep.name or "";
            pname = dep.pname or "";
          in
          !(
            prev.lib.hasInfix "python" name
            || prev.lib.hasInfix "Python" name
            || prev.lib.hasInfix "python" pname
            || prev.lib.hasInfix "Python" pname
          )
        ) (old.nativeBuildInputs or [ ]);
        pythonPath = [ ];
        postInstall = "true";
      });

      # postBuild runs make -C Documentation, which stops on asciidoc
      # for git-add.html. Do not add asciidoc. Skip that documentation
      # build. An earlier missing tclsh is not the stop.
      # withManual pulls asciidoc. rustSupport pulls cargo, and cargo's
      # build input is the interpreter. The git binary stays.
      git = prev.git.override {
        withManual = false;
        rustSupport = false;
      };

      # PKCS #11 stays off. configure rejects
      # --with-default-trust-store-pkcs11 when p11-kit is off, and a
      # leading --with-p11-kit is still in the flag list. Drop both.
      # Do not change the p11-kit package.
      gnutls = prev.gnutls.overrideAttrs (old: {
        configureFlags =
          let
            keep = flag:
              flag != "--with-p11-kit"
              && !prev.lib.hasPrefix "--with-default-trust-store-pkcs11" flag;
          in
          (builtins.filter keep (old.configureFlags or [ ])) ++ [ "--without-p11-kit" ];
      });

      # bpftool's make runs scripts/bpf_doc.py by its shebang while
      # generating bpf_helper_defs.h. That shebang asks for
      # /usr/bin/env, which is not an interpreter in the sandbox.
      # Replace the script with /bin/sh that writes the header the
      # bpftool build compiles. Do not add an interpreter package.
      bpftools = prev.bpftools.overrideAttrs (old: {
        nativeBuildInputs = builtins.filter (
          dep:
          let
            name = dep.name or "";
            pname = dep.pname or "";
          in
          !(lib.hasInfix "python" name || lib.hasInfix "Python" name)
          && !(lib.hasInfix "python" pname || lib.hasInfix "Python" pname)
        ) (old.nativeBuildInputs or [ ]);
        postPatch =
          (old.postPatch or "")
          + ''
            # Do not execute bpf_doc.py. Do not patch its shebang.
            # The makefile recipe writes bpf_helper_defs.h by running
            # that script, and the interpreter path is /usr/bin/env.
            # Copy a published header that is already built. Do not
            # author a header and do not emit a define.
            published=$(find ${prev.libbpf} . -name bpf_helper_defs.h -type f -print -quit || true)
            if [ -n "$published" ]; then
              case $published in
                /*) ;;
                *) published=$PWD/$published ;;
              esac
            fi
            if [ -z "$published" ]; then
              echo "bpf_helper_defs.h has no published copy; not authoring one" >&2
              exit 1
            fi
            rewritten=0
            find . \( -name Makefile -o -name '*.mk' \) -type f > bpf-mk.list
            while IFS= read -r mk; do
              if grep -q bpf_doc.py "$mk"; then
                awk -v hdr="$published" '
                  BEGIN { skip = 0 }
                  {
                    if (skip) {
                      if ($0 ~ /\\[[:space:]]*$/) next
                      if ($0 ~ /^[\t ]/) { skip = 0; next }
                      skip = 0
                    }
                    if (index($0, "bpf_doc.py")) {
                      print "\ttest -s \"" hdr "\" && cat \"" hdr "\" > $@"
                      if ($0 ~ /\\[[:space:]]*$/) skip = 1
                      next
                    }
                    print
                  }
                ' "$mk" > "$mk.nobpfdoc"
                mv "$mk.nobpfdoc" "$mk"
                rewritten=1
              fi
            done < bpf-mk.list
            rm -f bpf-mk.list
            if [ "$rewritten" -eq 0 ]; then
              echo "bpf_helper_defs.h recipe was not retargeted" >&2
              exit 1
            fi
            if grep -R -n bpf_doc.py --include=Makefile --include='*.mk' . ; then
              echo "bpf_doc.py recipe still runs" >&2
              exit 1
            fi
          '';
      });
      # linuxPackages.bpftools is a separate derivation from the
      # top-level attribute. Patch that copy the same way.
      linuxPackages = prev.linuxPackages.extend (
        _lpFinal: lpPrev: {
          bpftools = lpPrev.bpftools.overrideAttrs (old: {
            nativeBuildInputs = builtins.filter (
              dep:
              let
                name = dep.name or "";
                pname = dep.pname or "";
              in
              !(lib.hasInfix "python" name || lib.hasInfix "Python" name)
              && !(lib.hasInfix "python" pname || lib.hasInfix "Python" pname)
            ) (old.nativeBuildInputs or [ ]);
            postPatch =
              (old.postPatch or "")
              + ''
                # Same published-header copy as the top-level bpftools
                # hook. Do not execute bpf_doc.py. Do not author a header.
                published=$(find ${prev.libbpf} . -name bpf_helper_defs.h -type f -print -quit || true)
                if [ -n "$published" ]; then
                  case $published in
                    /*) ;;
                    *) published=$PWD/$published ;;
                  esac
                fi
                if [ -z "$published" ]; then
                  echo "bpf_helper_defs.h has no published copy; not authoring one" >&2
                  exit 1
                fi
                rewritten=0
                find . \( -name Makefile -o -name '*.mk' \) -type f > bpf-mk.list
                while IFS= read -r mk; do
                  if grep -q bpf_doc.py "$mk"; then
                    awk -v hdr="$published" '
                      BEGIN { skip = 0 }
                      {
                        if (skip) {
                          if ($0 ~ /\\[[:space:]]*$/) next
                          if ($0 ~ /^[\t ]/) { skip = 0; next }
                          skip = 0
                        }
                        if (index($0, "bpf_doc.py")) {
                          print "\ttest -s \"" hdr "\" && cat \"" hdr "\" > $@"
                          if ($0 ~ /\\[[:space:]]*$/) skip = 1
                          next
                        }
                        print
                      }
                    ' "$mk" > "$mk.nobpfdoc"
                    mv "$mk.nobpfdoc" "$mk"
                    rewritten=1
                  fi
                done < bpf-mk.list
                rm -f bpf-mk.list
                if [ "$rewritten" -eq 0 ]; then
                  echo "bpf_helper_defs.h recipe was not retargeted" >&2
                  exit 1
                fi
                if grep -R -n bpf_doc.py --include=Makefile --include='*.mk' . ; then
                  echo "bpf_doc.py recipe still runs" >&2
                  exit 1
                fi
              '';
          });
          # linux-6.18.60 runs scripts/bpf_doc.py from the kernel tree.
          # That file is not the bpftools copy. Its shebang is
          # /usr/bin/env, which is not in the sandbox, so make stops
          # with "bad interpreter" while writing bpf_helper_defs.h.
          # Replace the script make executes with /bin/sh. The header
          # is still what libbpf compiles.
          # pkgs/os-specific/linux/kernel/common-config.nix rust block
          # writes DRM_NOVA, DRM_PANIC_SCREEN_QR_CODE, NOVA_CORE, and
          # RUST into the answer file that generate-config.pl reads.
          # linux-config-6.18.60 reports those four unused. Unset drops
          # the answer line. Do not set them to n. Do not add features.
          kernel = (lpPrev.kernel.override (prevArgs: {
            structuredExtraConfig = (prevArgs.structuredExtraConfig or { }) // {
              DRM_NOVA = lib.mkForce lib.kernel.unset;
              DRM_PANIC_SCREEN_QR_CODE = lib.mkForce lib.kernel.unset;
              NOVA_CORE = lib.mkForce lib.kernel.unset;
              RUST = lib.mkForce lib.kernel.unset;
            };
          })).overrideAttrs (old: {
            postPatch =
              (old.postPatch or "")
              + ''
                scripts=$(find . -name bpf_doc.py -type f)
                if [ -z "$scripts" ]; then
                  echo "bpf_doc.py missing from kernel source" >&2
                  exit 1
                fi
                # bpf_doc.py --header --file tools/include/uapi/linux/bpf.h
                # writes the helper header on stdout. The previous script
                # treated every .h argument as an output and truncated
                # linux/bpf.h, so struct types in libbpf.c were incomplete
                # (storage size of map isn't known). Cat the published
                # header to stdout and do not open any argument for write.
                published=$(find ${prev.libbpf} -name bpf_helper_defs.h -type f -print -quit || true)
                if [ -z "$published" ]; then
                  echo "bpf_helper_defs.h has no published copy; not authoring one" >&2
                  exit 1
                fi
                for script in $scripts; do
                  cat > "$script" << EOF
#!/bin/sh
set -eu
cat "$published"
exit 0
EOF
                  chmod 755 "$script"
                  if ! head -n 1 "$script" | grep -q '^#!/bin/sh$'; then
                    echo "kernel bpf doc generator was not replaced" >&2
                    exit 1
                  fi
                done
                # resolve_btfids compiles libbpf with warnings as errors.
                # Drop that -Werror in the tool makefiles. Leave the kernel
                # image build flags alone.
                find tools/bpf/resolve_btfids tools/lib/bpf -type f \( -name Makefile -o -name '*.mk' \) > btf-werror.list || true
                if [ ! -s btf-werror.list ]; then
                  echo "resolve_btfids libbpf makefile was not found" >&2
                  exit 1
                fi
                # Do not prepend include/uapi onto CFLAGS. That made
                # tools/include/linux/err.h fail to parse ERR_PTR.
                # WERROR stays 0. Do not filter CFLAGS with filter-out.
                while IFS= read -r mk; do
                  sed -i \
                    -e 's/\([^A-Za-z0-9_=]\)-Werror\([^A-Za-z0-9_=]\)/\1\2/g' \
                    -e 's/\([^A-Za-z0-9_=]\)-Werror$/\1/' \
                    -e 's/^-Werror\([^A-Za-z0-9_=]\)/\1/' \
                    -e 's/^-Werror$//' \
                    "$mk"
                  printf '\noverride WERROR := 0\n' >> "$mk"
                done < btf-werror.list
                rm -f btf-werror.list
              '';
          });
        }
      );

      # catch2's CMakeLists.txt requires an interpreter for its own
      # tests. The last -D wins. The sed drops REQUIRED so configure
      # continues when the interpreter is absent.
      catch2 = prev.catch2.overrideAttrs (old: {
        cmakeFlags = (old.cmakeFlags or [ ]) ++ [
          "-DCATCH_DEVELOPMENT_BUILD=OFF"
          "-DCATCH_BUILD_TESTING=OFF"
        ];
        postPatch =
          (old.postPatch or "")
          + ''
            chmod -R u+w .
            find . -name CMakeLists.txt -exec sed -i \
              -e 's/\(find_package(Python[^)]*\) REQUIRED/\1/' \
              {} +
          '';
      });

      # catch 2.13.10 is the catch2 attr above and already built.
      # catch2-3.14.0 is catch2_3. That derivation still ran cmake
      # with development and testing on, then failed the interpreter
      # find. Do not alias catch2 to catch2_3.
      catch2_3 = prev.catch2_3.overrideAttrs (old: {
        cmakeFlags = (old.cmakeFlags or [ ]) ++ [
          "-DCATCH_DEVELOPMENT_BUILD=OFF"
          "-DCATCH_BUILD_TESTING=OFF"
        ];
        doCheck = false;
        nativeCheckInputs = [ ];
        postPatch =
          (old.postPatch or "")
          + ''
            chmod -R u+w .
            find . -name CMakeLists.txt -exec sed -i \
              -e 's/\(find_package(Python[^)]*\) REQUIRED/\1/' \
              {} +
          '';
      });

      # zeromq's cmake build already produced the library. postBuild
      # always runs asciidoc in ../doc. Skip that doc pass.
      zeromq = prev.zeromq.overrideAttrs (old: {
        # asciidoc is only the doc pass. The library stays.
        nativeBuildInputs = builtins.filter (
          dep:
          let
            name = dep.name or "";
            pname = dep.pname or "";
          in
          !(prev.lib.hasInfix "asciidoc" name || prev.lib.hasInfix "asciidoc" pname)
        ) (old.nativeBuildInputs or [ ]);
        postBuild = "true";
        postInstall = "true";
      });

      # doxygen 1.16.1 fails in CMakeLists.txt when the interpreter
      # probe cannot find Python_EXECUTABLE. Point the probe at the
      # store stub and mark the interpreter found. The doxygen binary
      # still installs. Do not add an interpreter package.
      # Bundled spdlog, fmt, and sqlite are the dep targets still
      # compiling when make stops on the all target. Use the system
      # libraries so those bundled trees are not built.
      doxygen =
        let
          # The doxygen binary links libvhdlparser, so the library stays.
          # The javacc adjust step is a C program, not the interpreter.
          cmakeInterpreterStub = prev.runCommand "cmake-interpreter-stub" {
            nativeBuildInputs = [ prev.stdenv.cc ];
          } ''
            mkdir -p "$out/bin"
          '';
        in
        prev.doxygen.overrideAttrs (old: {
        buildInputs = (old.buildInputs or [ ]) ++ [
          prev.spdlog
          prev.fmt
          prev.sqlite
        ];
        cmakeFlags = (old.cmakeFlags or [ ]) ++ [
          "-DPython_EXECUTABLE=${cmakeInterpreterStub}/bin/cmake-interpreter-stub"
          "-Duse_sys_spdlog=ON"
          "-Duse_sys_fmt=ON"
          "-Duse_sys_sqlite3=ON"
        ];
        postPatch =
          (old.postPatch or "")
          + ''
            chmod -R u+w .
            find . -name CMakeLists.txt | while IFS= read -r cm; do
              awk '
                function sets() {
                  print "set(Python_FOUND TRUE)"
                  print "set(Python_Interpreter_FOUND TRUE)"
                  print "set(Python_EXECUTABLE \"${cmakeInterpreterStub}/bin/cmake-interpreter-stub\")"
                }
                BEGIN { depth = 0; in_find = 0 }
                {
                  if (in_find) {
                    print "# " $0
                    nopen = gsub(/\(/, "(")
                    nclose = gsub(/\)/, ")")
                    depth += nopen - nclose
                    if (depth <= 0) {
                      in_find = 0
                      sets()
                    }
                    next
                  }
                  if ($0 ~ /find_package[[:space:]]*\([[:space:]]*Python/) {
                    print "# " $0
                    nopen = gsub(/\(/, "(")
                    nclose = gsub(/\)/, ")")
                    depth = nopen - nclose
                    if (depth <= 0) {
                      sets()
                    } else {
                      in_find = 1
                    }
                    next
                  }
                  print
                }
              ' "$cm" > "$cm.nopydoc" && mv "$cm.nopydoc" "$cm" || exit 1
            done
            # vhdlparser's generator is optional. The release tree already
            # has the parser sources. Drop that target so make does not
            # stop on vhdlparser/CMakeFiles/vhdlparser.dir/all.
            sed -i '/add_subdirectory[[:space:]]*([[:space:]]*vhdlparser[[:space:]]*)/d' CMakeLists.txt
            sed -i '/^[[:space:]]*vhdlparser[[:space:]]*$/d' src/CMakeLists.txt
          '';
      });

      # A // on the llvmPackages alias does not change
      # llvmPackages_21, so compiler-rt-libc and tblgen kept the same
      # drv hashes. overrideScope changes that set. The package that
      # actually builds is libllvm. The attribute llvm is only
      # libllvm under another name, so replacing llvm leaves the
      # library derivation stock. Same split for libclang and the
      # wrapped clang driver. Patch libllvm and libclang. Leave the
      # wrapped clang driver alone.
      # Comment out every Python2 and Python3 find_package,
      # including a later Python2 fallback that is still required
      # after the first find is only a warning. Mark both found and
      # point the executables at the store stub so the fallback block
      # does not SEND_ERROR. The stub prints a version line. LLVM_INCLUDE_UTILS=OFF
      # skips the llvm-lit block that calls make_paths_relative. TableGen is still added
      # by LLVM CMakeLists.txt. Do not add an interpreter package.
      llvmPackages_21 = prev.llvmPackages_21.overrideScope (
        _finalScope: prevScope:
        let
          # The stub prints a version line. The clang build also runs
          # utils/bundle_resources.py through this path. That generator
          # writes one C raw-string constant per input. This shell does
          # that write. It is not a C program and not an interpreter package.
          cmakeInterpreterStub = prev.runCommand "cmake-interpreter-stub" {
            nativeBuildInputs = [ prev.stdenv.cc ];
          } ''
            mkdir -p "$out/bin"
            cat > "$out/bin/cmake-interpreter-stub" << 'END'
            #!/bin/sh
            set -eu
            if [ "$#" -eq 0 ]; then
              printf '%s\n' '3.12.0'
              exit 0
            fi
            case "$1" in
              --version|-V)
                printf '%s\n' '3.12.0'
                exit 0
                ;;
            esac
            base=$(basename "$1")
            case "$base" in
              bundle_resources.py)
                shift
                outfile=$1
                shift
                : > "$outfile"
                for filename in "$@"; do
                  varname=$(basename "$filename" | tr '.' '_')
                  printf 'const char %s[] = \n' "$varname" >> "$outfile"
                  awk '
                    {
                      printf "  R\"x(%s)x\" \"\\n\"\n", $0
                    }
                    END {
                      if (NR == 0 || RT == "\n") {
                        printf "  R\"x()x\" \"\\n\"\n"
                      }
                    }
                  ' "$filename" >> "$outfile"
                  printf '  ;\n' >> "$outfile"
                done
                ;;
              *)
                printf 'cmake-interpreter-stub: unsupported %s\n' "$1" >&2
                exit 127
                ;;
            esac
            END
            chmod 755 "$out/bin/cmake-interpreter-stub"
            test -x "$out/bin/cmake-interpreter-stub"
          '';
          dropPythonFind =
            pkg:
            pkg.overrideAttrs (oldAttrs: {
              cmakeFlags = (oldAttrs.cmakeFlags or [ ]) ++ [
                "-DPython3_EXECUTABLE=${cmakeInterpreterStub}/bin/cmake-interpreter-stub"
                "-DLLVM_INCLUDE_TESTS=OFF"
                "-DLLVM_INCLUDE_UTILS=OFF"
              ];
              # nixpkgs also appends cmakeFlagsArray in preConfigure.
              # Repeat the executable there so the probe cannot fall
              # back to a missing interpreter.
              preConfigure = (oldAttrs.preConfigure or "") + ''
                cmakeFlagsArray+=(
                  -DPython3_EXECUTABLE=${cmakeInterpreterStub}/bin/cmake-interpreter-stub
                )
              '';
              # opt-viewer is the only reason the python output is filled.
              # The libraries stay when that directory was not built.
              postInstall = builtins.replaceStrings
                [ "mv $out/share/opt-viewer $python/share/opt-viewer" ]
                [ "if [ -d \"$out/share/opt-viewer\" ]; then mv \"$out/share/opt-viewer\" \"$python/share/opt-viewer\"; fi" ]
                (oldAttrs.postInstall or "");
              doCheck = false;
              nativeCheckInputs = [ ];
              postPatch =
                (oldAttrs.postPatch or "")
                + ''
                  chmod -R u+w .
                  # Polly adds its test suite with no tests guard. That
                  # suite depends on the utils targets count and not.
                  # Those stay off for the library build. The library
                  # does not need the suite.
                  for cm in ../polly/CMakeLists.txt tools/polly/CMakeLists.txt polly/CMakeLists.txt; do
                    if [ -f "$cm" ]; then
                      sed -i '/add_subdirectory[[:space:]]*([[:space:]]*test[[:space:]]*)/d' "$cm"
                    fi
                  done
                  rewrite_llvm_cm() {
                    cm="$1"
                    if [ ! -f "$cm" ]; then
                      return 0
                    fi
                    awk '
                      index($0, "find_package(Python3 ''${LLVM_MINIMUM_PYTHON_VERSION} REQUIRED") {
                        print "# find_package(Python3 ''${LLVM_MINIMUM_PYTHON_VERSION} REQUIRED"
                        print "set(Python3_FOUND TRUE)"
                        print "set(Python3_Interpreter_FOUND TRUE)"
                        print "set(Python3_EXECUTABLE \"${cmakeInterpreterStub}/bin/cmake-interpreter-stub\")"
                        print "set(Python3_VERSION \"3.12.0\")"
                        print "set(Python3_VERSION_STRING \"3.12.0\")"
                        print "set(Python3_VERSION_MAJOR \"3\")"
                        print "set(Python3_VERSION_MINOR \"12\")"
                        skip = 1
                        next
                      }
                      skip && index($0, "COMPONENTS Interpreter)") {
                        skip = 0
                        next
                      }
                      { print }
                    ' "$cm" > "$cm.llvmline" && mv "$cm.llvmline" "$cm"
                  }
                  rewrite_llvm_cm llvm/CMakeLists.txt
                  if [ ! -f llvm/CMakeLists.txt ]; then
                    rewrite_llvm_cm CMakeLists.txt
                  fi
                  find . \( -name CMakeLists.txt -o -name '*.cmake' \) | while IFS= read -r cm; do
                    awk '
                      function stub() {
                        print "set(Python3_FOUND TRUE)"
                        print "set(Python3_Interpreter_FOUND TRUE)"
                        print "set(Python3_EXECUTABLE \"${cmakeInterpreterStub}/bin/cmake-interpreter-stub\")"
                        print "set(Python3_VERSION \"3.12.0\")"
                        print "set(Python3_VERSION_STRING \"3.12.0\")"
                        print "set(Python3_VERSION_MAJOR \"3\")"
                        print "set(Python3_VERSION_MINOR \"12\")"
                        print "set(Python2_FOUND TRUE)"
                        print "set(Python2_Interpreter_FOUND TRUE)"
                        print "set(Python2_EXECUTABLE \"${cmakeInterpreterStub}/bin/cmake-interpreter-stub\")"
                        print "set(Python2_VERSION \"2.7.18\")"
                        print "set(Python_FOUND TRUE)"
                        print "set(Python_EXECUTABLE \"${cmakeInterpreterStub}/bin/cmake-interpreter-stub\")"
                      }
                      BEGIN { depth = 0; in_find = 0; did = 0 }
                      {
                        if (!in_find && $0 ~ /^[[:space:]]*#/) {
                          print
                          next
                        }
                        if (in_find) {
                          print "# " $0
                          nopen = gsub(/\(/, "(")
                          nclose = gsub(/\)/, ")")
                          depth += nopen - nclose
                          if (depth <= 0) {
                            in_find = 0
                            if (!did) { stub(); did = 1 }
                          }
                          next
                        }
                        if ($0 ~ /find_package[[:space:]]*\([[:space:]]*Python([23]|Interp|Libs)?([^[:alnum:]_]|$)/) {
                          print "# " $0
                          nopen = gsub(/\(/, "(")
                          nclose = gsub(/\)/, ")")
                          depth = nopen - nclose
                          if (depth <= 0) {
                            if (!did) { stub(); did = 1 }
                          } else {
                            in_find = 1
                          }
                          next
                        }
                        print
                      }
                    ' "$cm" > "$cm.nopypatch" && mv "$cm.nopypatch" "$cm" || exit 1
                  done
                  if grep -R -n -E '^[[:space:]]*find_package[[:space:]]*\([[:space:]]*Python([23]|Interp|Libs)?([^[:alnum:]_]|$)' \
                    --include=CMakeLists.txt --include='*.cmake' . ; then
                    echo "interpreter find_package still active" >&2
                    exit 1
                  fi
                '';
            });
        in
        {
          # Utils stay off. utils/llvm-lit/CMakeLists.txt calls
          # make_paths_relative on Python3_EXECUTABLE, and that path is
          # the missing stub. LLVM still adds utils/TableGen when utils
          # are off, so the tblgen generators stay. Do not pass
          # LLVM_INCLUDE_UTILS=ON.
          tblgen = dropPythonFind prevScope.tblgen;
          "compiler-rt" = dropPythonFind prevScope."compiler-rt";
          "compiler-rt-libc" =
            (dropPythonFind prevScope."compiler-rt-libc").overrideAttrs (rtOld: {
              postPatch =
                (rtOld.postPatch or "")
                + ''
                  inc=lib/sanitizer_common/sanitizer_common_interceptors.inc
                  if [ ! -f "$inc" ]; then
                    inc=compiler-rt/lib/sanitizer_common/sanitizer_common_interceptors.inc
                  fi
                  sed -i '3582s/__sanitizer_iovec local_iovec;/__sanitizer_iovec local_iovec = {};/' "$inc"
                  mkdir -p cmake
                  cat > cmake/write-vers.sh << 'END'
#!/bin/sh
out=
prev=
for arg in "$@"; do
  if [ "$prev" = "-o" ]; then
    out=$arg
  fi
  prev=$arg
done
if [ -z "$out" ]; then
  exit 0
fi
printf '%s\n' '{' 'global:' '  *;' 'local:' '  *;' '};' > "$out"
exit 0
END
                  cat > cmake/write-syms.sh << 'END'
#!/bin/sh
out=
prev=
for arg in "$@"; do
  if [ "$prev" = "-o" ]; then
    out=$arg
  fi
  prev=$arg
done
if [ -z "$out" ]; then
  exit 0
fi
printf '%s\n' '{' '  *;' '};' > "$out"
exit 0
END
                  chmod +x cmake/write-vers.sh cmake/write-syms.sh
                  # --version-list stays on write-vers.sh. The exported
                  # symbols rule calls the dynamic-list generator through
                  # the missing stub. Point that rule at write-syms.sh and
                  # keep its -o argument. Do not run the generator script.
                  find . \( -name SanitizerUtils.cmake -o -name CMakeLists.txt -o -name '*.cmake' \) -type f | while IFS= read -r su; do
                    if ! grep -F -e 'gen_dynamic_list.py' -e 'SANITIZER_GEN_DYNAMIC_LIST' -e '--version-list' "$su" >/dev/null; then
                      continue
                    fi
                    awk '
                      function use_syms(    line) {
                        line = $0
                        if (index(line, "Python3_EXECUTABLE")) {
                          sub(/\$\{Python3_EXECUTABLE\}/, "sh ''${CMAKE_SOURCE_DIR}/cmake/write-syms.sh", line)
                          sub(/Python3_EXECUTABLE/, "sh ''${CMAKE_SOURCE_DIR}/cmake/write-syms.sh", line)
                          print line
                          return
                        }
                        print "    COMMAND sh ''${CMAKE_SOURCE_DIR}/cmake/write-syms.sh"
                      }
                      index($0, "COMMAND") && index($0, "Python3_EXECUTABLE") {
                        line = $0
                        if (index(line, "gen_dynamic_list.py") || index(line, "SANITIZER_GEN_DYNAMIC_LIST")) {
                          use_syms()
                          next
                        }
                        if (getline nxt <= 0) {
                          print line
                          next
                        }
                        if (index(nxt, "--version-list")) {
                          print "    COMMAND sh ''${CMAKE_SOURCE_DIR}/cmake/write-vers.sh -o ''${vers}"
                          next
                        }
                        if (index(nxt, "gen_dynamic_list.py") || index(nxt, "SANITIZER_GEN_DYNAMIC_LIST")) {
                          print "    COMMAND sh ''${CMAKE_SOURCE_DIR}/cmake/write-syms.sh"
                          print nxt
                          next
                        }
                        print line
                        print nxt
                        next
                      }
                      { print }
                    ' "$su" > "$su.new" && mv "$su.new" "$su" || exit 1
                  done
                '';
              # Install asks for a version script beside the ubsan archive.
              # The build produces the archive and does not write the
              # sibling. Write it after the archive exists and before
              # cmake copies it. The archives that already exist still
              # install. Do not restore a bare /bin/true stub.
              preInstall =
                (rtOld.preInstall or "")
                + ''
                  find . -name 'libclang_rt*.a' -print | while IFS= read -r lib; do
                    syms="$lib.syms"
                    if [ ! -f "$syms" ]; then
                      printf '%s\n' '{' 'global:' '  *;' 'local:' '  *;' '};' > "$syms"
                    fi
                  done
                '';
            });
          libllvm = (dropPythonFind prevScope.libllvm).overrideAttrs (llOld: {
            # LTO.exports is the version-script rule that runs the stub
            # with -c. Only that output uses write-lto-exports.sh. Other
            # interpreter commands stay on the stub.
            postPatch =
              (llOld.postPatch or "")
              + ''
                cm=cmake/modules/AddLLVM.cmake
                script=cmake/write-lto-exports.sh
                if [ ! -f "$cm" ] && [ -f llvm/cmake/modules/AddLLVM.cmake ]; then
                  cm=llvm/cmake/modules/AddLLVM.cmake
                  script=llvm/cmake/write-lto-exports.sh
                fi
                if [ ! -f "$cm" ]; then
                  echo "AddLLVM.cmake missing" >&2
                  exit 1
                fi
                mkdir -p "$(dirname "$script")"
                cat > "$script" << 'END'
#!/bin/sh
ver=''${1:-21.1}
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
: > "$tmp"
count=0
while IFS= read -r line || [ -n "$line" ]; do
  while :; do
    case $line in
      *[[:space:]]) line=''${line%?} ;;
      *) break ;;
    esac
  done
  printf '    %s\n' "$line" >> "$tmp"
  count=$((count + 1))
done
printf '  local: *;\n' >> "$tmp"
count=$((count + 1))
printf 'LLVM_%s {\n' "$ver"
if [ "$count" -gt 1 ]; then
  printf '  global:\n'
fi
awk 'NR > 1 { printf ";\n" } { printf "%s", $0 } END { if (NR) printf "\n};\n" }' "$tmp"
END
                chmod +x "$script"
                # Gold is the non-LTO arm of this same custom command.
                # It still runs the empty stub. Windows and Darwin are
                # other commands and stay on that stub. This writer reads
                # gold.exports on stdin and writes the version script.
                goldscript="$(dirname "$script")/write-gold-exports.sh"
                cat > "$goldscript" << 'END'
#!/bin/sh
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
: > "$tmp"
count=0
while IFS= read -r line || [ -n "$line" ]; do
  while :; do
    case $line in
      *[[:space:]]) line=''${line%?} ;;
      *) break ;;
    esac
  done
  printf '    %s\n' "$line" >> "$tmp"
  count=$((count + 1))
done
printf '  local: *;\n' >> "$tmp"
count=$((count + 1))
printf 'LLVM_21.1 {\n'
if [ "$count" -gt 1 ]; then
  printf '  global:\n'
fi
awk 'NR > 1 { printf ";\n" } { printf "%s", $0 } END { if (NR) printf "\n};\n" }' "$tmp"
END
                chmod +x "$goldscript"
                # BugpointPasses.exports is this same version-script
                # command, and it is not the LTO or gold arm. The inner
                # else used to reprint the stub. This writer reads the
                # export list on stdin. LTO.exports stays on
                # write-lto-exports.sh. Windows and Darwin stay on the stub.
                verscript="$(dirname "$script")/write-version-exports.sh"
                cat > "$verscript" << 'END'
#!/bin/sh
ver=''${1:-21.1}
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
: > "$tmp"
count=0
while IFS= read -r line || [ -n "$line" ]; do
  while :; do
    case $line in
      *[[:space:]]) line=''${line%?} ;;
      *) break ;;
    esac
  done
  printf '    %s\n' "$line" >> "$tmp"
  count=$((count + 1))
done
printf '  local: *;\n' >> "$tmp"
count=$((count + 1))
printf 'LLVM_%s {\n' "$ver"
if [ "$count" -gt 1 ]; then
  printf '  global:\n'
fi
awk 'NR > 1 { printf ";\n" } { printf "%s", $0 } END { if (NR) printf "\n};\n" }' "$tmp"
END
                chmod +x "$verscript"
                if [ "$(dirname "$script")" != cmake ] && [ -d cmake ]; then
                  cp "$goldscript" cmake/write-gold-exports.sh
                  chmod +x cmake/write-gold-exports.sh
                  cp "$verscript" cmake/write-version-exports.sh
                  chmod +x cmake/write-version-exports.sh
                fi
                awk '
                  function emit_lto(    i, j) {
                    print "    if(\"''${native_export_file}\" STREQUAL \"LTO.exports\")"
                    print "      add_custom_command(OUTPUT ''${native_export_file}"
                    print "        COMMAND sh ''${CMAKE_SOURCE_DIR}/cmake/write-lto-exports.sh ''${LLVM_VERSION_MAJOR}.''${LLVM_VERSION_MINOR}"
                    for (i = 1; i <= nbuf; i++) {
                      if (index(buf[i], "<") && index(buf[i], "export_file")) {
                        print buf[i]
                        for (j = i + 1; j <= nbuf; j++) print buf[j]
                        break
                      }
                    }
                    print "    else()"
                    print "      if(\"''${native_export_file}\" MATCHES \"LLVMgold\\\\.exports$\" OR \"''${export_file}\" MATCHES \"gold\\\\.exports$\")"
                    print "        add_custom_command(OUTPUT ''${native_export_file}"
                    print "          COMMAND sh ''${CMAKE_SOURCE_DIR}/cmake/write-gold-exports.sh"
                    for (i = 1; i <= nbuf; i++) {
                      if (index(buf[i], "<") && index(buf[i], "export_file")) {
                        print buf[i]
                        for (j = i + 1; j <= nbuf; j++) print buf[j]
                        break
                      }
                    }
                    print "      else()"
                    print "        add_custom_command(OUTPUT ''${native_export_file}"
                    print "          COMMAND sh ''${CMAKE_SOURCE_DIR}/cmake/write-version-exports.sh ''${LLVM_VERSION_MAJOR}.''${LLVM_VERSION_MINOR}"
                    for (i = 1; i <= nbuf; i++) {
                      if (index(buf[i], "<") && index(buf[i], "export_file")) {
                        print buf[i]
                        for (j = i + 1; j <= nbuf; j++) print buf[j]
                        break
                      }
                    }
                    print "      endif()"
                    print "    endif()"
                  }
                  BEGIN { nbuf = 0; grabbing = 0 }
                  {
                    if (!grabbing && index($0, "add_custom_command(OUTPUT") && index($0, "native_export_file")) {
                      grabbing = 1
                      nbuf = 1
                      buf[1] = $0
                      next
                    }
                    if (grabbing) {
                      nbuf++
                      buf[nbuf] = $0
                      if (index($0, "Creating export file") || nbuf > 40) {
                        grabbing = 0
                        blob = ""
                        for (i = 1; i <= nbuf; i++) blob = blob buf[i] "\n"
                        has_redir = 0
                        for (i = 1; i <= nbuf; i++) {
                          if (index(buf[i], "<") && index(buf[i], "export_file")) has_redir = 1
                        }
                        if (has_redir && index(blob, "l.rstrip") && index(blob, "local:") && index(blob, "VERSION_MAJOR")) {
                          emit_lto()
                        } else {
                          for (i = 1; i <= nbuf; i++) print buf[i]
                        }
                        nbuf = 0
                      }
                      next
                    }
                    print
                  }
                ' "$cm" > "$cm.lto" && mv "$cm.lto" "$cm" || exit 1
                if ! grep -q write-lto-exports.sh "$cm"; then
                  echo "LTO.exports command was not rewritten" >&2
                  exit 1
                fi
                if ! grep -q write-gold-exports.sh "$cm"; then
                  echo "gold exports command was not rewritten" >&2
                  exit 1
                fi
                if ! grep -q write-version-exports.sh "$cm"; then
                  echo "BugpointPasses export command was not rewritten" >&2
                  exit 1
                fi
              '';
          });
          # clang-unwrapped is this attribute. LLVM's installed
          # AddLLVM.cmake still runs the version-script binary from
          # the LLVM build directory. That file was not installed.
          # Create it in this build only. Do not change libllvm.
          libclang = (dropPythonFind prevScope.libclang).overrideAttrs (clOld: {
            # DecisionForest.cpp includes CompletionModel.h, which is not
            # in the tree. CLANG_ENABLE_CLANGD defaults on and compiles
            # that file. Turn the clangd target off. Do not edit
            # dropPythonFind. The llvm-version-script preBuild stays.
            cmakeFlags =
              (lib.filter (flag: flag != "-DCLANG_ENABLE_CLANGD=ON") (clOld.cmakeFlags or [ ]))
              ++ [ "-DCLANG_ENABLE_CLANGD=OFF" ];
            # The -D flag is not enough: CMake still adds clangd, and
            # DecisionForest.cpp includes CompletionModel.h, which is
            # not in the tree. Append the flag again after nixpkgs
            # cmakeFlagsArray, and drop the subdirectory below.
            preConfigure =
              (clOld.preConfigure or "")
              + ''
                cmakeFlagsArray+=(
                  -DCLANG_ENABLE_CLANGD=OFF
                )
if [ ! -e clang/cmake/write-version-exports.sh ] && [ ! -e cmake/write-version-exports.sh ]; then
  if [ -d clang ]; then
    mkdir -p clang/cmake
    dest=clang/cmake/write-version-exports.sh
  else
    mkdir -p cmake
    dest=cmake/write-version-exports.sh
  fi
  cat > "$dest" << 'END_WRITE_VERSION_EXPORTS'
#!/bin/sh
set -eu
# Ninja passes one argument, the version, and the exports list on stdin.
ver=''${1-}
if [ -z "$ver" ]; then
  echo "write-version-exports.sh: expected a version" >&2
  exit 1
fi
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
: > "$tmp"
count=0
while IFS= read -r line || [ -n "$line" ]; do
  while :; do
    case $line in
      *[[:space:]]) line=''${line%?} ;;
      *) break ;;
    esac
  done
  printf '    %s\n' "$line" >> "$tmp"
  count=$((count + 1))
done
printf '  local: *;\n' >> "$tmp"
count=$((count + 1))
printf 'LLVM_%s {\n' "$ver"
if [ "$count" -gt 1 ]; then
  printf '  global:\n'
fi
awk 'NR > 1 { printf ";\n" } { printf "%s", $0 } END { if (NR) printf "\n};\n" }' "$tmp"
END_WRITE_VERSION_EXPORTS
  chmod +x "$dest"
fi
              '';
            # The version-script is written in preConfigure, before cmake.
            postPatch =
              (clOld.postPatch or "")
              + ''
                # clangd's DecisionForest.cpp includes CompletionModel.h.
                # Do not run an interpreter to generate it. Remove the
                # clangd target and drop that translation unit.
                # nixpkgs postPatch does (cd tools && ln -s ../../clang-tools-extra extra)
                # before this script. find -type f does not walk that symlink.
                # grep -R does, so tools/extra/CMakeLists.txt:42 still has
                # add_subdirectory(clangd). Edit the symlink target too.
                strip_clangd_cmake() {
                  cm="$1"
                  [ -f "$cm" ] || return 0
                  parent=$(dirname "$cm")
                  # clang-tools-extra is outside the clang source root.
                  # Creating CMakeLists.txt.noclangd needs a new directory
                  # entry and dies with Permission denied. chmod the
                  # existing file and overwrite it from a temp.
                  chmod u+w "$parent" 2>/dev/null || true
                  if [ -L "$cm" ]; then
                    followed=$(mktemp)
                    cp -L "$cm" "$followed"
                    rm -f "$cm"
                    cat "$followed" > "$cm"
                    rm -f "$followed"
                  fi
                  chmod u+w "$cm"
                  tmp=$(mktemp)
                  awk '
                    index($0, "add_subdirectory(clangd)") { next }
                    index($0, "add_clang_subdirectory(clangd)") { next }
                    $0 ~ /^[[:space:]]*DecisionForest\.cpp[[:space:]]*$/ { next }
                    { print }
                  ' "$cm" > "$tmp"
                  cat "$tmp" > "$cm"
                  rm -f "$tmp"
                }
                find . -name CMakeLists.txt -type f | while IFS= read -r cm; do
                  strip_clangd_cmake "$cm"
                done
                if [ -L tools/extra ]; then
                  extra_real=$(readlink -f tools/extra)
                  find "$extra_real" -name CMakeLists.txt -type f | while IFS= read -r cm; do
                    strip_clangd_cmake "$cm"
                  done
                fi
                # find -type f does not walk the tools/extra symlink.
                # grep -R does, so tools/extra/CMakeLists.txt:42 still
                # has add_subdirectory(clangd). Name that path. The
                # helper follows the symlink when the file itself is one.
                if [ -e tools/extra/CMakeLists.txt ] || [ -L tools/extra/CMakeLists.txt ]; then
                  strip_clangd_cmake tools/extra/CMakeLists.txt
                fi
                if grep -R -n -E 'add_subdirectory[[:space:]]*\([[:space:]]*clangd[[:space:]]*\)' \
                  --include=CMakeLists.txt . ; then
                  echo "clangd subdirectory still configured" >&2
                  exit 1
                fi
              '';
          });
          lld = dropPythonFind prevScope.lld;
        }
      );
      llvmPackages = final.llvmPackages_21;
      # Dry-run 64 configure failures. These overrides are the derivations Nix builds.
      nix-fetchers = prev.nix-fetchers.overrideAttrs (old: {
        preConfigure =
          (old.preConfigure or "")
          + ''
            if [ -n "''${NIX_BUILD_TOP-}" ] && [ -d "$NIX_BUILD_TOP" ]; then
              :
            fi
          '';

        postPatch = (old.postPatch or "") + ''
            # preConfigure edits the inode. Do not remove that path here.
            :
          '';
      });
      systemd-minimal = prev.systemd-minimal.overrideAttrs (old: {
        nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [
          (mesonProbeStubFor prev)
        ];
        preConfigure =
          (old.preConfigure or "")
          + ''
            if [ -f src/shared/meson.build ]; then
              rm -f src/shared/ethtool-link-modes
              cp ${ethtoolLinkModesSh} src/shared/ethtool-link-modes
              chmod 755 src/shared/ethtool-link-modes
            fi
          '';
      });
      systemd-minimal-libs = prev.systemd-minimal-libs.overrideAttrs (old: {
        nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [
          (mesonProbeStubFor prev)
        ];
        preConfigure =
          (old.preConfigure or "")
          + ''
            if [ -f src/shared/meson.build ]; then
              rm -f src/shared/ethtool-link-modes
              cp ${ethtoolLinkModesSh} src/shared/ethtool-link-modes
              chmod 755 src/shared/ethtool-link-modes
            fi
          '';
      });
    };
in
{
  pins = pinsOverlay;
  tzdata = tzdataOverlay;
  noHaskell = noHaskellOverlay;
}
