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
#   pins      Nix 2.35.4 cli. Safe on checks (mkPkgs) and on the mail host.
#   tzdata    tzdata 2026e doCheck = false. Mail-host nixosSystem only.
#             Not on mkPkgs, so checks.x86_64-linux.ci does not rebuild
#             CPython just because now.ck is skipped.
#   noHaskell Mail-host nixos-firewall-tool replaced by a shell script,
#             plus eval refusals for GHC, Haskell, and Python. Mail-host
#             nixosSystem only. Not on mkPkgs. The firewall-tool check
#             was the remaining Haskell edge after nixfmt-rs. It is now
#             a non-Haskell script. Referencing ghc, haskellPackages,
#             haskell.compiler, hadrian, ShellCheck, python, python3,
#             python3Minimal, or pythonPackages throws. glibc's
#             python3Minimal input and libapparmor's python3Packages
#             input are removed. There is no stand-in interpreter.
#
# tzdata 2026e: now.ck is date-dependent. On or after 2026-11-01 the check
# fails because America/Phoenix and America/Vancouver are identical from
# now on (America/Edmonton and America/Winnipeg are the same class). This
# is not a gcc or x86-64-v3 failure. Same attribute change as
# NixOS/nixpkgs pull 582069. gcc.arch and x86-64-v3 stay as they are.
#
# Nix: nixos-26.05 pkgs.nix is nixVersions.stable (the 2.34.9 recipe).
# nixVersions.nix_2_35 is the 2.35.3 recipe: one fetchFromGitHub source
# and meson components, no cargoHash. 2.35.4 is that recipe plus the
# source of commit 21286d605815196c3fff1960550ab08700fc4e6f and version
# 2.35.4. The nixpkgs channel stays nixos-26.05. NixOS 26.11 is not
# released.
#
# The system package is nix-cli, not nix-everything. nix-everything's
# installPhase symlinks doc and man to nix-manual (the HTML manual;
# json-schema-for-humans). Its doCheck pulls nix-functional-tests, and
# those tests' nativeBuildInputs include mercurial (python3.13-pip).
# The mail host runs nix-daemon from nix-cli, including the systemd
# units that component installs. It does not run the HTML manual or
# the functional tests. nix-nswrapper is only the non-root daemon
# ExecStart; the default daemon user is root, and this host does not
# change that.

let
  # Mail-host package set only. flake nixosConfigurations.mail-vps
  # appends this. mkPkgs (checks.x86_64-linux.ci, devShells) does not.
  tzdataOverlay = _final: prev: {
    tzdata = prev.tzdata.overrideAttrs (_old: {
      doCheck = false; # https://lists.iana.org/hyperkitty/list/tz@iana.org/thread/CBEK3HXYBOXGD34ZFDSF64QIDT7ZWGBB/
    });
  };

  pinsOverlay = final: prev: {
    nixVersions = prev.nixVersions.extend (
      _vfinal: vprev:
      let
        # nix-prefetch-url --unpack printed
        # 03h0j2bk97xgm15x02ccph4h1rwwci37mkdy6d1200jxzf51b2a4
        # builtins.convertHash { toHashFormat = "sri"; hashAlgo = "sha256"; }
        # printed sha256-RIkVivtdAiBCM77NekZknOcACbyMCdBLqK+fNJeQAA4=
        nixSrc2354 = prev.fetchFromGitHub {
          owner = "NixOS";
          repo = "nix";
          rev = "21286d605815196c3fff1960550ab08700fc4e6f";
          hash = "sha256-RIkVivtdAiBCM77NekZknOcACbyMCdBLqK+fNJeQAA4=";
        };
        components = vprev.nixComponents_2_35.overrideScope (
          _finalScope: _prevScope: {
            version = "2.35.4";
            src = nixSrc2354;
            patchedSrc = nixSrc2354;
          }
        );
      in
      {
        nixComponents_2_35 = components;
        # nix-cli: binaries and systemd units. Not nix-everything
        # (that package builds nix-manual and mercurial).
        nix_2_35 = components.nix-cli;
        latest = components.nix-cli;
      }
    );

    # Channel stable stays the 2.34.9 nix-everything attribute.
    # Nothing in the mail-host module list selects it. The package
    # the system runs is pkgs.nix, the 2.35.4 nix-cli above.
    nix = final.nixVersions.nix_2_35;
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
      # forbidden interpreters by name or pname.
      isForbiddenDep =
        dep:
        if dep == null then
          true
        else if !builtins.isAttrs dep then
          false
        else
          let
            name = dep.name or "";
            pname = dep.pname or "";
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
            lib.concatMap (dep: if builtins.isList dep then dep else [ dep ]) xs
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
      interpreterShadow = {
        python = slot;
        python3 = slot;
        python3Minimal = slot;
        pythonPackages = fakePkgs;
        python3Packages = fakePkgs;
        # nss-cacert lists this. The all-packages value is
        # `with python3.pkgs`, so a callPackage lookup must not
        # be that value. The scrub drops the empty directory.
        buildcatrust = slot;
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
        # systemtap's buildCommand evaluates
        # `with python3.pkgs; makePythonPath`. An empty
        # platform list makes glib's withDtrace false, so
        # that file is not evaluated and systemtap is not a
        # native build input. This is not an interpreter.
        systemtap-sdt = slot // {
          meta = {
            platforms = [ ];
          };
        };
        shellcheck = shellcheckFake;
        shellcheck-minimal = shellcheckFake;
        ShellCheck = shellcheckFake;
      };
      callScope = final // interpreterShadow // {
        stdenv = stdenvFiltered;
        stdenvNoCC = stdenvNoCCFiltered;
        pkgs = final // interpreterShadow;
      };
      # Build-time package set used by systemd (python3Packages) and
      # by writeShellApplication (shellcheck-minimal). Direct
      # final.python3 and final.shellcheck still throw.
      buildScope = final // interpreterShadow;
    in
    {
      stdenv = stdenvFiltered;
      stdenvNoCC = stdenvNoCCFiltered;
      callPackage = lib.callPackageWith callScope;
      callPackages = lib.callPackagesWith callScope;
      newScope = extra: lib.callPackageWith (callScope // extra);
      buildPackages = buildScope;
      pkgsBuildHost = buildScope;
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
      # lists python3Packages even when its Python bindings are unused.
      # nss-cacert lists buildcatrust (`with python3.pkgs`). glib
      # lists docutils (`with python3Packages`). libtiff lists
      # sphinx (`with python3Packages`). shadow lists itstool,
      # whose pythonPath reads python3.pkgs.libxml2. glib's optional dtrace
      # input is buildPackages.systemtap-sdt, whose buildCommand
      # evaluates `with python3.pkgs; makePythonPath`. Those
      # lookups hit interpreterShadow. systemtap-sdt has no
      # platforms, so glib leaves it out. The scrub drops the empty
      # directory. The C libraries stay. No interpreter is added.
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
      pandas = refuse "pandas";
      pip = refuse "pip";
      dbus-python = refuse "dbus-python";
      json-schema-for-humans = refuse "json-schema-for-humans";
    };
in
{
  pins = pinsOverlay;
  tzdata = tzdataOverlay;
  noHaskell = noHaskellOverlay;
}
