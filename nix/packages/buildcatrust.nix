{
  stdenv,
}:

# nss-cacert runs $out/bin/buildcatrust during buildPhase.
# The handwritten C source is not in the tree. This derivation
# does not compile C and does not add an interpreter.
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
