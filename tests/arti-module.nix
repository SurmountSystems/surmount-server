# Arti HS thin pure contracts (no full nixosSystem).
# Full Arti module eval lives in tests/module-eval.nix (single CI full eval).
# Do not hand-copy option defaults here as a false-green table; pure checks
# only cover path charset / absolute / no-key-material helpers.

{
  pkgs ? null,
  lib ? null,
  sops-nix ? null,
}:
let
  # Documented conventional defaults (must match modules/options.nix).
  # These are path-shape contracts only, not a substitute for module-eval.
  conventionalOnionServiceStateDir = "/run/surmount-secrets/arti/onion-service";
  conventionalStateDir = "/var/lib/surmount/arti";

  isAbsolute = p: builtins.isString p && p != "" && builtins.substring 0 1 p == "/";

  hasInfix =
    needle: haystack:
    let
      n = builtins.stringLength needle;
      h = builtins.stringLength haystack;
      go =
        i:
        if i + n > h then
          false
        else if builtins.substring i n haystack == needle then
          true
        else
          go (i + 1);
    in
    if n == 0 then true else go 0;

  noKeyMaterial =
    s:
    !(
      hasInfix "BEGIN OPENSSH" s
      || hasInfix "BEGIN PRIVATE" s
      || hasInfix "PRIVATE KEY" s
      || hasInfix "hs_ed25519_secret_key" s
    );

  # When lib is available, re-use host-paths.nix (same SoT as modules).
  hostPaths = if lib != null then import ../modules/lib/host-paths.nix { inherit lib; } else null;

  pure = [
    (
      assert isAbsolute conventionalOnionServiceStateDir;
      "t1-hs-state-absolute"
    )
    (
      assert isAbsolute conventionalStateDir;
      "t2-state-dir-absolute"
    )
    (
      assert noKeyMaterial conventionalOnionServiceStateDir;
      "t3-no-key-material-hs"
    )
    (
      assert noKeyMaterial conventionalStateDir;
      "t4-no-key-material-state"
    )
  ]
  ++ (
    if hostPaths == null then
      [ ]
    else
      [
        (
          assert hostPaths.strictHostPath conventionalOnionServiceStateDir;
          "t5-hs-state-strict-host-path"
        )
        (
          assert hostPaths.strictHostPath conventionalStateDir;
          "t6-state-dir-strict-host-path"
        )
        (
          assert !(hostPaths.strictHostPath "relative/arti");
          "t7-relative-reject"
        )
      ]
  );

  # Optional note: full arti contracts (startDaemon gate, ConditionPathIsDirectory)
  # are in module-eval.nix. Do not re-import nixosSystem here (Issue 7).
  _unused = {
    inherit pkgs sops-nix;
  };
in
{
  ok = pure;
  pure = pure;
  # Empty: full module contracts are only in module-eval-contract.
  module = [ ];
}
