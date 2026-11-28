# Pure checks for four deploy failures. Fixture checks are functions of
# an attrset. The real initrd check evaluates nixosConfigurations.mail-vps
# (the config just deploy activates). This file does not build an initrd
# and does not build the mail-host closure.
#
# Existing eval runner (do not grow it for these checks):
#   tests/module-eval.nix
#   nix eval --raw .#checks.x86_64-linux.module-eval-contract
#
# This file is visible on its own:
#   nix-instantiate --eval --strict -A red tests/dry-run-reveals-deploy.nix
#   nix-instantiate --eval --strict -A ok tests/dry-run-reveals-deploy.nix
#
# red throws. ok requires every bad fixture to throw and every good
# fixture to pass. Standalone ok loads the flake. withFlake takes the
# flake already being evaluated and does not call getFlake again.
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

  splitLines = text: builtins.filter builtins.isString (builtins.split "\n" text);

  quotedOnLine =
    line:
    let
      m = builtins.match ".*\"([^\"]+)\".*" line;
    in
    if m == null then [ ] else [ (builtins.head m) ];

  # First extra-system-features = [ ... ]; list in a module source.
  # That list is the in-repo contract the deploy client must present.
  featureListFrom =
    text:
    let
      lines = splitLines text;
      take =
        on: acc: rest:
        if rest == [ ] then
          acc
        else
          let
            line = builtins.head rest;
            tail = builtins.tail rest;
            start = (!on) && hasInfix "extra-system-features" line && hasInfix "[" line;
            stop = on && hasInfix "];" line;
          in
          if stop then
            acc
          else if on || start then
            take true (acc ++ quotedOnLine line) tail
          else
            take false acc tail;
    in
    take false [ ] lines;

  # switch-to-configuration stop list, not the restart list.
  # A changed service is stopped when restartIfChanged and stopIfChanged
  # are both true. A removed service is stopped when stopOnRemoval is true.
  # restartIfChanged false skips the unit. stopIfChanged false restarts it
  # instead, and that restart is not this stop list.
  unitIsStopped =
    unit:
    let
      removed = unit.removed or false;
      changed = unit.changed or false;
      stopIfChanged = unit.stopIfChanged or true;
      restartIfChanged = unit.restartIfChanged or true;
      stopOnRemoval = unit.stopOnRemoval or true;
    in
    (removed && stopOnRemoval) || (changed && restartIfChanged && stopIfChanged);

  stopNames =
    units: map (unit: unit.name) (builtins.filter unitIsStopped units);

  checkDeployProgram =
    fixture:
    let
      program = fixture.program or null;
      sshOpts = fixture.sshOpts or [ ];
      spellings = [
        "IdentityFile=~/.ssh/id_ed25519"
        "BatchMode=yes"
        "IdentitiesOnly=yes"
      ];
      opts = if builtins.isList sshOpts then sshOpts else [ ];
      asStrings = map (opt: if builtins.isString opt then opt else builtins.toString opt) opts;
      hasSpelling = spelling: builtins.any (opt: opt == spelling || hasInfix spelling opt) asStrings;
      usesHome = builtins.any (opt: hasInfix "$HOME" opt) asStrings;
      missing = builtins.filter (spelling: !hasSpelling spelling) spellings;
    in
    if builtins.isAttrs program then
      throw "deploy program is a Nix set, not a string"
    else if !builtins.isString program then
      throw "deploy program is not a string"
    else if usesHome then
      throw "deploy ssh identity uses $HOME"
    else if missing != [ ] then
      throw ("deploy sshOpts lack " + builtins.concatStringsSep ", " missing)
    else
      "deploy-program-ok";

  checkDeployClientFeatures =
    fixture:
    let
      features = fixture.features or [ ];
      list = if builtins.isList features then features else [ ];
    in
    if builtins.elem "gccarch-x86-64-v3" list then
      "deploy-client-features-ok"
    else
      throw "deploy client feature list omits gccarch-x86-64-v3";

  checkActivationStopList =
    fixture:
    let
      stopList = fixture.stopList or [ ];
      banned = [
        "network-addresses-eth0.service"
        "sshd.service"
      ];
      hit = builtins.filter (name: builtins.elem name banned) stopList;
    in
    if hit != [ ] then
      throw ("activation stop list contains " + builtins.concatStringsSep ", " hit)
    else
      "activation-stop-ok";

  # Kernel module name in hosts/mail-vps/configuration.nix is virtio_scsi.
  # virtio-scsi is the same module with a hyphen. libkmod is the initrd input.
  checkInitrd =
    fixture:
    let
      inputs = fixture.inputs or [ ];
      modules = fixture.modules or [ ];
      inputText = builtins.concatStringsSep " " (
        map (
          item:
          if builtins.isString item then
            item
          else if builtins.isAttrs item then
            builtins.toString (item.source or "")
          else
            builtins.toString item
        ) inputs
      );
      hasLibkmod = hasInfix "libkmod" inputText;
      hasVirtio = builtins.any (name: name == "virtio_scsi" || name == "virtio-scsi") modules;
    in
    if !hasLibkmod then
      throw "initrd inputs omit libkmod"
    else if !hasVirtio then
      throw "initrd module set omits virtio_scsi"
    else
      "initrd-ok";

  mustThrow =
    name: value:
    let
      result = builtins.tryEval value;
    in
    if result.success then throw (name + " was expected to fail") else name;

  mustPass =
    name: value:
    if builtins.isString value then name else throw (name + " did not pass");

  badProgramSet = {
    program = {
      type = "derivation";
    };
    sshOpts = [
      "-o"
      "IdentityFile=$HOME/.ssh/id_ed25519"
    ];
  };

  badProgramHome = {
    program = "deploy";
    sshOpts = [
      "-o"
      "IdentityFile=$HOME/.ssh/id_ed25519"
      "-o"
      "BatchMode=yes"
      "-o"
      "IdentitiesOnly=yes"
    ];
  };

  badProgramOpts = {
    program = "deploy";
    sshOpts = [
      "-o"
      "IdentityFile=~/.ssh/id_ed25519"
    ];
  };

  badProgramMissingIdentity = {
    program = "deploy";
    sshOpts = [
      "-o"
      "BatchMode=yes"
      "-o"
      "IdentitiesOnly=yes"
    ];
  };

  goodProgram = {
    program = "deploy";
    sshOpts = [
      "-o"
      "IdentityFile=~/.ssh/id_ed25519"
      "-o"
      "BatchMode=yes"
      "-o"
      "IdentitiesOnly=yes"
    ];
  };

  badFeatures = {
    features = [ "surmount-remote" ];
  };

  goodFeatures = {
    features = [
      "surmount-remote"
      "gccarch-x86-64-v3"
    ];
  };

  badStop = {
    stopList = [
      "network-addresses-eth0.service"
      "stalwart-mail.service"
    ];
  };

  badStopSshd = {
    stopList = [ "sshd.service" ];
  };

  goodStop = {
    stopList = [
      "stalwart-mail.service"
      "network-local-commands.service"
    ];
  };

  badInitrd = {
    inputs = [ "other-lib" ];
    modules = [ "virtio_scsi" ];
  };

  badInitrdVirtio = {
    inputs = [ "libkmod" ];
    modules = [ "e1000" ];
  };

  goodInitrd = {
    inputs = [ "libkmod" ];
    modules = [ "virtio_scsi" ];
  };

  stubDrv = name: {
    type = "derivation";
    name = name;
    system = "x86_64-linux";
    outPath = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-${name}";
    drvPath = "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-${name}.drv";
  };

  realApp = import ../nix/deploy-app.nix {
    system = "x86_64-linux";
    pkgs = {
      writeShellScript = name: _text: stubDrv name;
    };
    deploy-rs = {
      packages.x86_64-linux.deploy-rs = stubDrv "deploy-rs";
    };
  };

  realNode = import ../nix/deploy-node.nix {
    self = { };
    deploy-rs = { };
  };

  realProgramFixture = {
    program = realApp.program;
    sshOpts = realNode.nodes.mail-vps.sshOpts;
  };

  realProgramStatus =
    let
      result = builtins.tryEval (checkDeployProgram realProgramFixture);
    in
    if result.success then
      "real-deploy-program-pass"
    else if builtins.isAttrs realProgramFixture.program then
      "real-deploy-program-BLOCKED"
    else
      throw "real deploy program failed and is not a Nix set";

  builderText = builtins.readFile ../modules/remote-builder.nix;
  realFeatures = featureListFrom builderText;

  netText = builtins.readFile ../modules/networking.nix;
  eth0Flags =
    hasInfix "network-addresses-eth0.stopIfChanged = false;" netText
    && hasInfix "network-addresses-eth0.restartIfChanged = false;" netText;
  sshdFlags =
    hasInfix "systemd.services.sshd.stopIfChanged = false;" netText
    && hasInfix "systemd.services.sshd.restartIfChanged = false;" netText;
  eth0Gate = hasInfix "config.networking.interfaces ? eth0" netText;

  # Mail-host activation defines eth0 outside this public file. This
  # check does not read that file and does not record an address.
  # Deploy 4 stopped network-addresses-eth0.service, so the interface
  # was present. The module applies the flags only in that case.
  realStopUnits = [
    {
      name = "network-addresses-eth0.service";
      removed = false;
      changed = true;
      stopIfChanged = !eth0Flags;
      restartIfChanged = !eth0Flags;
      stopOnRemoval = true;
    }
    {
      name = "sshd.service";
      removed = false;
      changed = true;
      stopIfChanged = !sshdFlags;
      restartIfChanged = !sshdFlags;
      stopOnRemoval = true;
    }
  ];

  realStopList = stopNames realStopUnits;

  realStopStatus =
    if !eth0Gate then
      throw "networking.nix does not gate network-addresses-eth0 on the eth0 interface"
    else if eth0Flags && sshdFlags then
      mustPass "real-activation-stop" (checkActivationStopList { stopList = realStopList; })
    else
      throw (
        "real activation would stop "
        + builtins.concatStringsSep ", " realStopList
      );

  # storePaths entries are submodules ({ source, enable, ... }), not bare
  # strings. Coercing the set throws. virtio-scsi is the hyphen spelling
  # checkInitrd already accepts. kernelModules is the other initrd list.
  # Standalone ok passes getFlake. withFlake passes the in-eval flake.
  realInitrdFrom =
    flake:
    let
      cfg = flake.nixosConfigurations.mail-vps.config;
      storePaths = cfg.boot.initrd.systemd.storePaths;
      sourceText =
        item:
        if builtins.isString item then
          item
        else if builtins.isAttrs item then
          if item.enable or true then builtins.toString (item.source or "") else ""
        else
          builtins.toString item;
      inputText = builtins.concatStringsSep "\n" (map sourceText storePaths);
      hasLibkmod = hasInfix "libkmod.so.2" inputText;
      moduleNames =
        cfg.boot.initrd.availableKernelModules
        ++ cfg.boot.initrd.kernelModules;
      hasVirtio = builtins.any (name: name == "virtio_scsi" || name == "virtio-scsi") moduleNames;
    in
    if !hasLibkmod then
      throw "initrd inputs omit libkmod"
    else if !hasVirtio then
      throw "initrd module set omits virtio_scsi"
    else
      "initrd-real-pass";

  realInitrdStatus = realInitrdFrom (builtins.getFlake (toString ../.));

  # deploy-dry-run must not call nix-instantiate. That eval is a horizon
  # check of this file. Running it on surmount-1 blocked the closure build
  # on one CPU. A comment is not a command. The first command must be
  # nix run .#deploy-dry-run. Passing does not require nix-instantiate.
  checkJustDryRunRecipe =
    let
      startsWith =
        prefix: text: builtins.substring 0 (builtins.stringLength prefix) text == prefix;
      trimLeft =
        text:
        let
          len = builtins.stringLength text;
          go =
            i:
            if i >= len then
              ""
            else
              let
                ch = builtins.substring i 1 text;
              in
              if ch == " " || ch == "\t" then go (i + 1) else builtins.substring i (len - i) text;
        in
        go 0;
      lines = splitLines (builtins.readFile ../justfile);
      isHeader = line: startsWith "deploy-dry-run" line && hasInfix ":" line;
      headerFound = builtins.any isHeader lines;
      afterHeader =
        let
          go =
            rest:
            if rest == [ ] then
              [ ]
            else if isHeader (builtins.head rest) then
              builtins.tail rest
            else
              go (builtins.tail rest);
        in
        go lines;
      isBodyLine = line: line == "" || startsWith " " line || startsWith "\t" line;
      body =
        let
          go =
            rest:
            if rest == [ ] then
              [ ]
            else if isBodyLine (builtins.head rest) then
              [ (builtins.head rest) ] ++ (go (builtins.tail rest))
            else
              [ ];
        in
        go afterHeader;
      trimmed = map trimLeft body;
      commands = builtins.filter (line: line != "" && !(startsWith "#" line)) trimmed;
      firstCommand = if commands == [ ] then "" else builtins.head commands;
      bodyHasInstantiate = builtins.any (line: hasInfix "nix-instantiate" line) body;
      hasRun = builtins.any (line: hasInfix "nix run .#deploy-dry-run" line) commands;
    in
    if !headerFound then
      throw "deploy-dry-run recipe is missing from justfile"
    else if !hasRun then
      throw "deploy-dry-run recipe has no nix run .#deploy-dry-run"
    else if hasInfix "nix-instantiate" firstCommand then
      throw "deploy-dry-run recipe starts with nix-instantiate"
    else if bodyHasInstantiate then
      throw "deploy-dry-run recipe calls nix-instantiate"
    else if !(hasInfix "nix run .#deploy-dry-run" firstCommand) then
      throw "deploy-dry-run recipe does not start with nix run .#deploy-dry-run"
    else
      "recipe-skips-instantiate";

  # Recipe check is forced first. A leading nix-instantiate throws before
  # the mail-vps initrd eval. Green still forces every assert below.
  okFrom =
    initrdStatus:
    let
      recipeStatus = checkJustDryRunRecipe;
      parts = [
        recipeStatus
        (mustThrow "bad-deploy-program-set" (checkDeployProgram badProgramSet))
        (mustThrow "bad-deploy-program-home" (checkDeployProgram badProgramHome))
        (mustThrow "bad-deploy-program-opts" (checkDeployProgram badProgramOpts))
        (mustThrow "bad-deploy-program-missing-identity" (checkDeployProgram badProgramMissingIdentity))
        (mustPass "good-deploy-program" (checkDeployProgram goodProgram))
        realProgramStatus
        (mustThrow "bad-deploy-client-features" (checkDeployClientFeatures badFeatures))
        (mustPass "good-deploy-client-features" (checkDeployClientFeatures goodFeatures))
        (mustPass "real-deploy-client-features" (checkDeployClientFeatures { features = realFeatures; }))
        (mustThrow "bad-activation-stop" (checkActivationStopList badStop))
        (mustThrow "bad-activation-stop-sshd" (checkActivationStopList badStopSshd))
        (mustPass "good-activation-stop" (checkActivationStopList goodStop))
        realStopStatus
        (mustThrow "bad-initrd-libkmod" (checkInitrd badInitrd))
        (mustThrow "bad-initrd-virtio" (checkInitrd badInitrdVirtio))
        (mustPass "good-initrd" (checkInitrd goodInitrd))
        initrdStatus
      ];
    in
    if recipeStatus == "recipe-skips-instantiate" then
      builtins.concatStringsSep " " parts
    else
      throw "deploy-dry-run recipe check did not pass";
in
{
  inherit
    checkDeployProgram
    checkDeployClientFeatures
    checkActivationStopList
    checkInitrd
    ;

  # Bad initrd fixture. Evaluating this attr throws.
  red = checkInitrd badInitrd;

  ok = okFrom realInitrdStatus;

  # In-flake entry. The passed flake is the one already being evaluated.
  withFlake = flake: {
    red = checkInitrd badInitrd;
    ok = okFrom (realInitrdFrom flake);
  };
}
