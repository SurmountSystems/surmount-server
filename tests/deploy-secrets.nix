# Deploy-secrets thin pure charset contracts (no full nixosSystem).
# Full secrets module eval lives in tests/module-eval.nix (single CI full eval).

{
  pkgs ? null,
  lib ? null,
  sops-nix ? null,
}:
let
  hostPaths = if lib != null then import ../modules/lib/host-paths.nix { inherit lib; } else null;

  pure =
    if hostPaths == null then
      {
        ok = [ "skipped-no-lib" ];
      }
    else
      let
        t1 =
          assert hostPaths.strictHostPath "/run/surmount-secrets/tls/cert.pem";
          "t1-strict-ok";
        t2 =
          assert !(hostPaths.strictHostPath "secrets/foo");
          "t2-relative-reject";
        t3 =
          assert !(hostPaths.strictHostPath "/tmp/x;rm");
          "t3-meta-reject";
        t4 =
          assert !(hostPaths.strictHostPath "/tmp/../etc/passwd");
          "t4-dotdot-reject";
        t5 =
          assert hostPaths.optionalStrictHostPath "";
          "t5-empty-optional";
        t6 =
          assert hostPaths.allStrictHostPaths [
            "/run/a"
            "/var/lib/surmount/b"
          ];
          "t6-all-strict";
      in
      {
        ok = [
          t1
          t2
          t3
          t4
          t5
          t6
        ];
      };

  # Optional note: full requireDeployMaterial + path-kind wiring is in
  # module-eval.nix. Do not re-import nixosSystem here (Issue 7).
  _unused = {
    inherit pkgs sops-nix;
  };
in
{
  pure = pure.ok;
  module = [ ];
  ok = pure.ok;
}
