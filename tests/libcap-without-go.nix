# Mail-host libcap stays the C library. Instantiate only. Do not build.
#
#   nix-instantiate --eval --strict -A red tests/libcap-without-go.nix
#   nix-instantiate --eval --strict -A ok tests/libcap-without-go.nix
#
# red throws. The fixture still has GOLANG=yes and a go-1 compiler on
# nativeBuildInputs. ok requires that throw, requires the raw nixpkgs
# libcap (no overlay) to throw the same way, and passes
# nixosConfigurations.mail-vps.pkgs.libcap. That package is the mail-host
# overlay list, not pkgs.libcap from raw nixpkgs. The release splora-web
# derivation must not list dart-sass, buf, or a Go compiler on
# nativeBuildInputs. nix/splora-web-nixpkgs.nix does not overlay libcap.
# Pass text for libcap includes GOLANG=no and nativeBuildInputs-no-go.
let
  hasInfix =
    needle: hay:
    let
      n = builtins.stringLength needle;
      h = builtins.stringLength hay;
      go =
        i:
        if i + n > h then
          false
        else if builtins.substring i n hay == needle then
          true
        else
          go (i + 1);
    in
    go 0;

  hasPrefix =
    prefix: text: builtins.substring 0 (builtins.stringLength prefix) text == prefix;

  mustThrow =
    name: value:
    let
      result = builtins.tryEval value;
    in
    if result.success then throw (name + " was expected to fail") else name;

  mustPass =
    name: value:
    if
      builtins.isString value
      && hasInfix "GOLANG=no" value
      && hasInfix "nativeBuildInputs-no-go" value
    then
      name + " " + value
    else
      throw (name + " did not pass");

  depName =
    dep:
    if builtins.isString dep then
      dep
    else if builtins.isAttrs dep then
      dep.name or dep.pname or ""
    else
      "";

  depPname = dep: if builtins.isAttrs dep then dep.pname or "" else "";

  isGoCompiler =
    dep:
    let
      name = depName dep;
      pname = depPname dep;
    in
    pname == "go" || name == "go" || hasPrefix "go-" name || hasInfix "go-1" name;

  checkLibcap =
    pkg:
    let
      rawFlags = pkg.makeFlags or null;
      flags = if builtins.isList rawFlags then rawFlags else throw "makeFlags missing";
      badFlag = builtins.any (flag: !builtins.isString flag) flags;
      golangYes = builtins.any (
        flag: builtins.isString flag && (flag == "GOLANG=yes" || hasPrefix "GOLANG=yes" flag)
      ) flags;
      golangNo = builtins.any (flag: flag == "GOLANG=no") flags;
      libFlag = builtins.any (flag: flag == "lib=lib") flags;
      captreeFlag = builtins.any (flag: builtins.isString flag && hasInfix "captree" flag) flags;
      inputs =
        if !(pkg ? nativeBuildInputs) then
          throw "nativeBuildInputs missing"
        else if !(builtins.isList pkg.nativeBuildInputs) then
          throw "nativeBuildInputs missing"
        else
          pkg.nativeBuildInputs;
      names = map depName inputs;
      goHits = builtins.filter isGoCompiler inputs;
      goNames = map depName goHits;
      outputs = pkg.outputs or null;
      post = pkg.postPatch or "";
      nameText = if names == [ ] then "none" else builtins.concatStringsSep "," names;
      passed =
        "GOLANG=no nativeBuildInputs-no-go count="
        + builtins.toString (builtins.length names)
        + " names="
        + nameText
        + " lib-output capsh-stays progs-prefix-stays captree-not-a-product";
    in
    if badFlag then
      throw "makeFlags missing"
    else if golangYes then
      throw ("GOLANG=yes still present nativeBuildInputs=" + builtins.concatStringsSep "," names)
    else if !golangNo then
      throw "GOLANG=no missing"
    else if goHits != [ ] then
      throw ("go compiler on nativeBuildInputs: " + builtins.concatStringsSep "," goNames)
    else if captreeFlag then
      throw "captree is a build product"
    else if !builtins.isList outputs then
      throw "outputs missing"
    else if builtins.elem "captree" outputs then
      throw "captree is a build product"
    else if !(builtins.elem "lib" outputs) then
      throw "lib output missing"
    else if !(builtins.elem "out" outputs) then
      throw "out output missing"
    else if !libFlag then
      throw "libcap library make flag missing"
    else if !builtins.isString post || !hasInfix "progs/capsh.c" post then
      throw "capsh is missing from the build"
    else if !hasInfix "prefix=$lib" post then
      throw "libcap library prefix missing"
    else if !hasInfix "exec_prefix=$out" post then
      throw "progs install prefix missing"
    else if hasInfix "captree" post then
      throw "captree is a build product"
    else
      passed;

  # Still-Go fixture. progs/capsh.c and the lib/out prefixes are present
  # so the throw is GOLANG=yes, not a missing C library.
  redFixture = {
    pname = "libcap";
    version = "2.78";
    outputs = [
      "out"
      "dev"
      "lib"
      "man"
      "doc"
      "pam"
    ];
    nativeBuildInputs = [
      {
        name = "go-1.27.4";
        pname = "go";
      }
    ];
    makeFlags = [
      "lib=lib"
      "GOLANG=yes"
    ];
    postPatch = ''
      substituteInPlace progs/capsh.c
      substituteInPlace progs/mkcapshdoc.sh
      substituteInPlace Make.Rules \
        --replace-fail 'prefix=/usr' "prefix=$lib" \
        --replace-fail 'exec_prefix=' "exec_prefix=$out"
    '';
  };

  # GOLANG=no is not enough when a Go compiler is still an input.
  goCompilerFixture = redFixture // {
    makeFlags = [
      "lib=lib"
      "GOLANG=no"
    ];
  };

  flake = builtins.getFlake (toString ../.);

  rawLibcap = (import flake.inputs.nixpkgs { system = "x86_64-linux"; }).libcap;

  # nixosConfigurations.mail-vps.pkgs is the mail-host overlay list
  # (surmountPins, which imports nix/libcap-without-go-overlay.nix,
  # then tzdata, the Surmount package overlay, noHaskell, and the glibc
  # overlays). Not raw nixpkgs.
  mailLibcap = flake.nixosConfigurations.mail-vps.pkgs.libcap;

  # Crane buildTrunkPackage appends dart-sass. The release package
  # filters that input. A hit here means the release graph still builds
  # Sass, buf, or a Go compiler.
  webNative = flake.packages.x86_64-linux.splora-web.nativeBuildInputs or [ ];
  webBad = builtins.filter (
    dep:
    let
      name = depName dep;
      pname = depPname dep;
    in
    pname == "dart-sass"
    || pname == "buf"
    || pname == "go"
    || name == "dart-sass"
    || name == "buf"
    || name == "go"
    || hasPrefix "dart-sass-" name
    || hasPrefix "buf-" name
    || hasPrefix "go-" name
    || hasInfix "go-1" name
  ) webNative;
  webNativeOk =
    if webBad != [ ] then
      throw (
        "splora-web nativeBuildInputs still include "
        + builtins.concatStringsSep "," (map depName webBad)
      )
    else
      "splora-web-native-no-dart-sass-buf-go";

  lockedRev =
    let
      lock = builtins.fromJSON (builtins.readFile ../flake.lock);
      nodeName = lock.nodes.root.inputs.nixpkgs;
    in
    lock.nodes.${nodeName}.locked.rev;

  expectedRev = "823a281b8fec58291825dfdab54b1a34fb48dc5b";
in
{
  red = checkLibcap redFixture;

  ok =
    if lockedRev != expectedRev then
      throw ("nixpkgs rev is " + lockedRev)
    else
      builtins.concatStringsSep " " [
        (mustThrow "fixture-golang-yes" (checkLibcap redFixture))
        (mustThrow "fixture-go-compiler" (checkLibcap goCompilerFixture))
        (mustThrow "raw-golang-yes" (checkLibcap rawLibcap))
        (mustPass "mail-host-libcap" (checkLibcap mailLibcap))
        webNativeOk
        ("nixpkgs=" + lockedRev)
      ];
}
