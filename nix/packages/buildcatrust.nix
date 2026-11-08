{
  stdenv,
}:

# C binary nss-cacert runs in buildPhase. Source is the file already
# in this repo. Not the PyPI package. No Python, no network fetch.
stdenv.mkDerivation {
  pname = "buildcatrust";
  version = "0";

  # One C file. Do not unpack it as a tarball.
  dontUnpack = true;
  src = ./buildcatrust.c;

  buildPhase = ''
    runHook preBuild
    "$CC" -O2 -o buildcatrust "$src"
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p "$out/bin"
    install -m 755 buildcatrust "$out/bin/buildcatrust"
    runHook postInstall
  '';

  meta.mainProgram = "buildcatrust";
}
