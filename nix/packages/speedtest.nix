# Wrapped Ookla Speedtest CLI (pkgs.ookla-speedtest).
# Always passes --accept-license --accept-gdpr. Extra args still pass through.
# nix run .#speedtest
{
  ookla-speedtest,
  makeWrapper,
  symlinkJoin,
}:
symlinkJoin {
  pname = "speedtest";
  name = "speedtest";
  paths = [ ookla-speedtest ];
  nativeBuildInputs = [ makeWrapper ];
  postBuild = ''
    wrapProgram $out/bin/speedtest \
      --add-flags "--accept-license" \
      --add-flags "--accept-gdpr"
  '';
  meta = ookla-speedtest.meta // {
    description = "Ookla Speedtest CLI with license and GDPR flags accepted";
    mainProgram = "speedtest";
  };
}
