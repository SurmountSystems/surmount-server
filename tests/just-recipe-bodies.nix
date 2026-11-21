# Contract: every just recipe is one nix run line, or a dependency list.
# A shell body fails this file. Import from the flake checks.
{ lib }:
let
  just = builtins.readFile ../justfile;
  apps = builtins.readFile ../nix/just-recipe-apps.nix;
  flake = builtins.readFile ../flake.nix;
  doc = builtins.readFile ../docs/TEST-AND-DEPLOY.md;
  updater = builtins.readFile ../crates/surmount-deploy-host/src/update_maintenance.rs;
  lines = lib.splitString "\n" just;

  expected = [
    "default:"
    "check: fmt clippy test"
    "ci:"
    "check-remote:"
    "audit-remote:"
    "remote-nix-build installable label:"
    "audit:"
    "deny:"
    "check-ci: ci"
    "fmt:"
    "fmt-write:"
    "test:"
    "clippy:"
    "dev:"
    "e2e:"
    "e2e-host:"
    "publish *args:"
    "deploy *args:"
    "deploy-dry-run *args:"
    "deploy-host *args:"
    "btop *args:"
    "host-inxi *args:"
    "et *args:"
    "grok-oss *args:"
    "grok-oss-running *args:"
    "host-logs *args:"
    "host-status *args:"
    "scram *args:"
    "test-logging-eval:"
    "test-sshd-hostkeys-eval:"
    "test-eternal-terminal-eval:"
    "test-inxi-eval:"
    "test-btop-eval:"
    "test-perf-tools-eval:"
    "test-remote-builder-eval:"
    "check-private-data *args:"
    "check-leftover-agent-homes *args:"
    "check-tls-hybrid:"
    "secrets-prompt *args:"
    "rekey *args:"
    "diskstation-afp-mount *args:"
    "diskstation-discover *args:"
    "sync-static-sites-from-ds3018xs *args:"
    "mailplus-copy-uid *args:"
    "add-stalwart-token *args:"
    "bootstrap-stalwart-token *args:"
    "stalwart-recovery-unlock *args:"
    "fix-public-dashboard *args:"
    "secrets-install-host *args:"
    "secrets-export-bw-to-staging *args:"
    "host-cutover *args:"
    "cutover-step step *args:"
    "render-host-profile-acme *args:"
    "domain-audit *args:"
    "dns-zone-namecheap *args:"
    "rdns-shc *args:"
    "free-stalwart-public-443 *args:"
    "register-dkim *args:"
    "point-stalwart-mail-tls *args:"
    "laptop-renew-cert *args:"
    "check-heavy:"
    "vendor:"
    "update-flakes:"
    "update-software:"
  ];

  # These flake apps already exec their own binaries. Replacing them with
  # surmount-deploy-host would call nix run of the same name again.
  directApps = [
    "e2e"
    "e2e-host"
    "add-stalwart-token"
    "stalwart-recovery-unlock"
    "secrets-install-host"
    "secrets-export-bw-to-staging"
    "register-dkim"
    "free-stalwart-public-443"
    "point-stalwart-mail-tls"
  ];

  recurseIfReplaced = [
    "e2e"
    "e2e-host"
    "register-dkim"
    "free-stalwart-public-443"
    "point-stalwart-mail-tls"
  ];

  skipLine =
    line:
    let
      t = lib.trim line;
    in
    t == ""
    || lib.hasPrefix "#" t
    || lib.hasPrefix "set " t
    || lib.hasPrefix "export " t
    || lib.hasPrefix "alias " t
    || lib.hasPrefix "[" t
    || lib.hasInfix ":=" t;

  recipeLine =
    line:
    (!skipLine line)
    && builtins.match "[A-Za-z_][A-Za-z0-9_-]*.*" line != null
    && lib.hasInfix ":" line;

  headerOf =
    line:
    let
      parts = lib.splitString ":" line;
      left = builtins.head parts;
      words = lib.filter (w: w != "") (lib.splitString " " left);
      name = builtins.head words;
      deps = lib.trim (lib.concatStringsSep ":" (builtins.tail parts));
    in
    {
      inherit name deps;
      header = lib.trim line;
    };

  takeBody =
    i:
    if i >= builtins.length lines then
      {
        cmds = [ ];
        next = i;
      }
    else
      let
        line = builtins.elemAt lines i;
        indented = lib.hasPrefix " " line || lib.hasPrefix "\t" line;
      in
      if !indented then
        {
          cmds = [ ];
          next = i;
        }
      else
        let
          rest = takeBody (i + 1);
          t = lib.trim line;
          keep = t != "" && !lib.hasPrefix "#" t;
        in
        {
          cmds = (if keep then [ t ] else [ ]) ++ rest.cmds;
          next = rest.next;
        };

  walk =
    i: acc:
    if i >= builtins.length lines then
      acc
    else
      let
        line = builtins.elemAt lines i;
      in
      if !recipeLine line then
        if skipLine line then walk (i + 1) acc else walk (i + 1) (acc ++ [ "unparsed line: ${line}" ])
      else
        let
          h = headerOf line;
          body = takeBody (i + 1);
          want = "nix run .#${h.name} -- \"$@\"";
          bodyProblem =
            if body.cmds == [ ] then
              if h.deps == "" then "recipe ${h.name} has an empty body" else null
            else if builtins.length body.cmds != 1 then
              "recipe ${h.name} has ${toString (builtins.length body.cmds)} command lines"
            else if builtins.head body.cmds != want then
              "recipe ${h.name} body is not one nix run line: ${builtins.head body.cmds}"
            else
              null;
          acc2 = acc ++ [ h ] ++ (if bodyProblem == null then [ ] else [ bodyProblem ]);
        in
        walk body.next acc2;

  walked = walk 0 [ ];
  recipes = builtins.filter (x: builtins.isAttrs x) walked;
  bodyProblems = builtins.filter (x: builtins.isString x) walked;
  headers = map (r: r.header) recipes;
  names = map (r: r.name) recipes;

  headerProblem =
    if headers == expected then
      [ ]
    else
      [
        "recipe headers drifted (${toString (builtins.length headers)} parsed, ${toString (builtins.length expected)} expected)"
      ];

  appProblems = lib.concatMap (
    name:
    let
      quoted = "\"${name}\"";
      inApps = lib.hasInfix quoted apps;
      direct = builtins.elem name directApps;
    in
    if direct then
      [ ]
    else if inApps then
      [ ]
    else
      [ "flake app missing for recipe ${name}" ]
  ) names;

  replaceProblems = lib.concatMap (
    name:
    if lib.hasInfix "\"${name}\"" apps then
      [ "flake app ${name} would recurse into nix run of the same name" ]
    else
      [ ]
  ) recurseIfReplaced;

  fileProblems =
    (if lib.hasInfix "#!/" just then [ "justfile still has a shebang" ] else [ ])
    ++ (
      if lib.hasInfix "alias private-data := check-private-data" just then
        [ ]
      else
        [ "private-data alias missing" ]
    )
    ++ (
      if lib.hasInfix "import ./nix/just-recipe-apps.nix" flake then
        [ ]
      else
        [ "flake.nix does not import just-recipe-apps.nix" ]
    )
    ++ (
      if lib.hasInfix "nix run .#update-flakes" doc then
        [ ]
      else
        [ "TEST-AND-DEPLOY.md missing nix run .#update-flakes" ]
    )
    ++ (
      if lib.hasInfix "nix run .#update-software" doc then
        [ ]
      else
        [ "TEST-AND-DEPLOY.md missing nix run .#update-software" ]
    )
    ++ (if lib.hasInfix "just deploy" doc then [ ] else [ "TEST-AND-DEPLOY.md missing just deploy" ])
    ++ (
      if lib.hasInfix "sparse+https://index.crates.menhera.org/10d/" updater then
        [ ]
      else
        [ "updater missing menhera cooldown index" ]
    )
    ++ (
      if lib.hasInfix "\"--update-input\"" updater then
        [ ]
      else
        [ "updater missing nix flake lock --update-input" ]
    )
    ++ (if lib.hasInfix "cargo-audit" updater then [ ] else [ "updater missing hermetic cargo-audit" ])
    ++ (
      if lib.hasInfix "fn walk_flakes" updater then [ ] else [ "updater does not walk flake.nix files" ]
    )
    ++ (
      if lib.hasInfix "#!/" apps || lib.hasInfix "writeShell" apps || lib.hasInfix "meson" apps then
        [ "just-recipe-apps.nix has shell or meson text" ]
      else
        [ ]
    );

  problems = bodyProblems ++ headerProblem ++ appProblems ++ replaceProblems ++ fileProblems;
in
{
  ok =
    if problems == [ ] then
      "just-recipe-bodies-ok"
    else
      throw "just-recipe-bodies:\n${lib.concatStringsSep "\n" problems}";
}
