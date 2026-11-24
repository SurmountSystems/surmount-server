# Fail closed unless the mail-vps deploy-rs profile asks for rollback,
# its activate path is a derivation, and the switch app execs deploy-rs.
# A missing flag throws. Refuses check-jsonschema and deployChecks.
#
#   nix-instantiate --eval --strict tests/deploy-rs-rollback.nix
#
# The flake check calls this file with { lib, deploy }. That call runs
# the same source checks plus the evaluated node. Standalone eval does
# not build deploy-rs and does not activate a profile.
let
  nodeText = builtins.readFile ../nix/deploy-node.nix;
  appText = builtins.readFile ../nix/deploy-app.nix;
  appsText = builtins.readFile ../nix/just-recipe-apps.nix;
  flakeText = builtins.readFile ../flake.nix;
  justText = builtins.readFile ../justfile;

  splitLines = text: builtins.filter builtins.isString (builtins.split "\n" text);

  # Line match, not a per-character walk. flake.nix is too long for that walk.
  escapeRegex =
    needle:
    let
      nlen = builtins.stringLength needle;
      special = [
        "."
        "*"
        "+"
        "?"
        "["
        "]"
        "("
        ")"
        "{"
        "}"
        "|"
        "^"
        "$"
        "\\"
      ];
      go =
        i:
        if i >= nlen then
          ""
        else
          let
            c = builtins.substring i 1 needle;
            esc = if builtins.elem c special then "\\${c}" else c;
          in
          esc + go (i + 1);
    in
    go 0;

  lineHas = needle: line: builtins.match (".*" + escapeRegex needle + ".*") line != null;

  hasInfix = needle: text: builtins.any (lineHas needle) (splitLines text);

  countInfix =
    needle: text: builtins.length (builtins.filter (lineHas needle) (splitLines text));

  hasLine = needle: builtins.any (line: line == needle) (splitLines justText);

  sourceProblems =
    if hasInfix "check-jsonschema" nodeText || hasInfix "deployChecks" nodeText then
      "nix/deploy-node.nix must not reference check-jsonschema or deployChecks"
    else if hasInfix "check-jsonschema" appText || hasInfix "deployChecks" appText then
      "nix/deploy-app.nix must not reference check-jsonschema or deployChecks"
    else if hasInfix "surmount-deploy-host" appText then
      "nix/deploy-app.nix must not exec surmount-deploy-host"
    else if
      countInfix "autoRollback = true" nodeText != 2
      || countInfix "autoRollback = false" nodeText != 0
    then
      "nodes.mail-vps and profiles.system autoRollback must both be true"
    else if
      countInfix "magicRollback = true" nodeText != 2
      || countInfix "magicRollback = false" nodeText != 0
    then
      "nodes.mail-vps and profiles.system magicRollback must both be true"
    else if !hasInfix "nodes.mail-vps" nodeText then
      "deploy.nodes.mail-vps is absent"
    else if !hasInfix ''hostname = "surmount-1"'' nodeText then
      "nodes.mail-vps.hostname must be surmount-1"
    else if !hasInfix ''sshUser = "root"'' nodeText then
      "nodes.mail-vps.sshUser must be root"
    else if !hasInfix "deploy-rs.lib.x86_64-linux.activate.nixos" nodeText then
      "profiles.system.path must use deploy-rs.lib.x86_64-linux.activate.nixos"
    else if !hasInfix "deploy-rs.packages." appText then
      "deploy app does not reference deploy-rs"
    else if !hasInfix "/bin/deploy" appText then
      "deploy app does not exec the deploy-rs deploy binary"
    else if !hasInfix ".#mail-vps" appText then
      "deploy app does not select node mail-vps"
    else if !hasInfix "exec " appText then
      "deploy app does not exec deploy-rs"
    else if !hasInfix "--skip-checks" appText then
      "deploy app does not pass --skip-checks"
    else if !hasInfix "import ./nix/deploy-app.nix" flakeText then
      "flake apps.deploy does not import nix/deploy-app.nix"
    else if hasInfix ''"deploy"'' appsText then
      "nix run .#deploy must not be the surmount-deploy-host argv0 symlink"
    else if !hasInfix ''"deploy-dry-run"'' appsText then
      "deploy-dry-run left the surmount-deploy-host app farm"
    else if !hasLine "deploy *args:" || !hasLine "    nix run .#deploy -- \"\$@\"" then
      "just deploy must be nix run .#deploy"
    else if
      !hasLine "deploy-dry-run *args:"
      || !hasLine "    nix run .#deploy-dry-run -- \"\$@\""
    then
      "just deploy-dry-run must stay nix run .#deploy-dry-run"
    else
      null;

  tokens = [
    "deploy-rs-rollback-ok"
    "deploy-app-references-deploy-rs"
    "deploy-dry-run-not-deploy-rs"
  ];

  textOk = if sourceProblems == null then tokens else throw ("deploy-rs-rollback: " + sourceProblems);

  structural =
    { lib, deploy }:
    let
      node = deploy.nodes.mail-vps or (throw "deploy.nodes.mail-vps is absent");
      profile = node.profiles.system or (throw "profiles.system is absent");
      magicRollback =
        if profile ? magicRollback then
          profile.magicRollback
        else
          throw "profiles.system.magicRollback is absent";
      autoRollback =
        if profile ? autoRollback then
          profile.autoRollback
        else
          throw "profiles.system.autoRollback is absent";
      path = if profile ? path then profile.path else throw "profiles.system.path is absent";
      nodeMagic =
        if node ? magicRollback then node.magicRollback else throw "nodes.mail-vps.magicRollback is absent";
      nodeAuto =
        if node ? autoRollback then node.autoRollback else throw "nodes.mail-vps.autoRollback is absent";
    in
    if magicRollback != true then
      throw "profiles.system.magicRollback must be true"
    else if autoRollback != true then
      throw "profiles.system.autoRollback must be true"
    else if nodeMagic != true then
      throw "nodes.mail-vps.magicRollback must be true"
    else if nodeAuto != true then
      throw "nodes.mail-vps.autoRollback must be true"
    else if node.hostname or "" != "surmount-1" then
      throw "nodes.mail-vps.hostname must be surmount-1"
    else if node.sshUser or "" != "root" then
      throw "nodes.mail-vps.sshUser must be root"
    else if profile.user or "" != "root" then
      throw "profiles.system.user must be root"
    else if !(lib.isDerivation path) then
      throw "profiles.system.path must be a derivation"
    else
      textOk;
in
{
  ok = textOk;
  __functor = self: args: {
    ok = structural args;
  };
}
