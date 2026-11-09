{
  stdenv,
}:

# cacert's overlay buildPhase installs a published CA bundle and
# does not run this program. This derivation does not compile C
# and does not add an interpreter. The bin directory stays empty.
stdenv.mkDerivation {
  pname = "buildcatrust";
  version = "0";

  dontUnpack = true;

  buildPhase = ''
    runHook preBuild
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p "$out/bin"
    runHook postInstall
  '';
}
