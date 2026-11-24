# Operator switch for `nix run .#deploy` / `just deploy`.
# The program execs the deploy-rs `deploy` binary for node mail-vps.
# Rollback flags stay on the node in nix/deploy-node.nix.
# This file does not activate a profile and does not import upstream checks.
{ pkgs, system, deploy-rs }:
let
  deployBin = deploy-rs.packages.${system}.deploy-rs;
  nodeArg = ".#mail-vps";
  # Single quotes so the shell does not treat # as a comment.
  surmountExecLine = "exec ${deployBin}/bin/deploy --skip-checks '${nodeArg}' \"$@\"";
  program = pkgs.writeShellScript "deploy-rs-mail-vps" ''
    ${surmountExecLine}
  '';
in
{
  type = "app";
  inherit program surmountExecLine;
  surmountDeployRsBin = deployBin;
  surmountNodeArg = nodeArg;
}
