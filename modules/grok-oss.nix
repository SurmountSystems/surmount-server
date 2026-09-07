# Optional Grok OSS on the mail host. Default off.
# Enable from host-local. User grok owns ~/.grok. Binary on PATH.
# Do not auto-start the TUI on boot (operator starts it in tmux).
# Hard MemoryMax on the grok user slice. Not Nice=19 (sshd class).
# Install pkgs.tmux here so attach does not depend only on Eternal Terminal.

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  go = cfg.grokOss;
  inherit (lib) mkIf mkMerge;
  userSliceName = "user-${toString go.uid}";
  grokHome = go.home;
in
{
  config = mkMerge [
    (mkIf (cfg.enable && go.enable) {
      assertions = [
        {
          assertion = go.package != null;
          message = "surmount.grokOss.package must be set when surmount.grokOss.enable is true (bin/grok-oss). Default-off leaves package null so eval does not pull grok-oss. Fail-closed: do not enable without the flake package.";
        }
        {
          assertion = go.memoryMax != null && go.memoryMax != "";
          message = "surmount.grokOss.memoryMax must be a non-empty systemd MemoryMax string when enable is true (empty/null refuse; scaffold default 4G is a grok-oss budget, not a published guest size).";
        }
        {
          assertion = builtins.substring 0 1 go.home == "/";
          message = "surmount.grokOss.home must be an absolute path.";
        }
        {
          assertion = go.user != "root";
          message = "surmount.grokOss.user must not be root (do not cap the whole machine).";
        }
      ];

      users.groups.${go.user} = {
        gid = lib.mkDefault go.uid;
      };

      users.users.${go.user} = {
        isNormalUser = true;
        description = "Grok OSS operator TUI (no boot start)";
        group = go.user;
        uid = go.uid;
        home = grokHome;
        createHome = true;
        hashedPassword = "!";
        linger = false;
      };

      environment.systemPackages = [ pkgs.tmux ] ++ lib.optional (go.package != null) go.package;

      systemd.tmpfiles.rules = [
        "d ${grokHome} 0750 ${go.user} ${go.user} -"
        "d ${grokHome}/.grok 0700 ${go.user} ${go.user} -"
      ];

      # Cap the login slice (tmux / grok-oss under user grok). No TUI unit.
      systemd.slices.${userSliceName} = {
        description = "grok user slice (hard memory cap)";
        sliceConfig = {
          MemoryAccounting = true;
          MemoryMax = go.memoryMax;
        };
      };
    })
  ];
}
