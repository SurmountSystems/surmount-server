# Public HTTP Host -> Arti HS nickname helpers (per-site v3 onions).
# HS private keys stay on the host under onionServiceStateDir. This file
# never embeds .onion addresses or key material.

{ lib }:
let
  inherit (lib)
    filter
    hasPrefix
    hasSuffix
    stringLength
    toLower
    unique
    ;

  normalizeHost = h: toLower (lib.removeSuffix "." (lib.trim h));

  nicknameCharsetOk = n: builtins.match "[A-Za-z0-9][A-Za-z0-9_-]*" n != null;

  # Arti nickname: start alphanumeric, then alnum / _ / -. Host dots become '-'.
  hostToNickname =
    host:
    let
      n = normalizeHost host;
      slug = lib.replaceStrings [ "." ] [ "-" ] n;
      raw = "site-${slug}";
    in
    if nicknameCharsetOk raw && stringLength raw <= 96 then raw else null;

  isMailHost =
    {
      host,
      primaryDomain,
      mailHostname,
      extraMailHostnames,
    }:
    let
      h = normalizeHost host;
      primary = normalizeHost primaryDomain;
      mail = normalizeHost mailHostname;
      extras = map normalizeHost extraMailHostnames;
    in
    (h != "" && h == mail) || (primary != "" && h == "mail.${primary}") || builtins.elem h extras;

  publicHttpHosts =
    {
      primaryDomain,
      servicesHostname,
      mailHostname,
      extraMailHostnames ? [ ],
      staticVhostHosts ? [ ],
    }:
    let
      primary = normalizeHost primaryDomain;
      services = normalizeHost servicesHostname;
      skip =
        h:
        isMailHost {
          host = h;
          inherit primaryDomain mailHostname extraMailHostnames;
        };
      apex = if primary == "" then [ ] else [ primary ];
      www = if primary == "" then [ ] else [ "www.${primary}" ];
      svc = if services == "" then [ ] else [ services ];
      mta = if primary == "" then [ ] else [ "mta-sts.${primary}" ];
      extras = map normalizeHost staticVhostHosts;
      ordered = apex ++ www ++ svc ++ mta ++ extras;
      cleaned = filter (h: h != "" && !(hasPrefix "." h) && !(skip h)) ordered;
    in
    unique cleaned;

  # Services console keeps artiHiddenService.nickname (existing HS identity).
  nicknameForHost =
    {
      host,
      servicesHostname,
      consoleNickname,
    }:
    let
      h = normalizeHost host;
      services = normalizeHost servicesHostname;
    in
    if h != "" && h == services then consoleNickname else hostToNickname h;
in
{
  inherit
    normalizeHost
    nicknameCharsetOk
    hostToNickname
    isMailHost
    publicHttpHosts
    nicknameForHost
    ;
}
