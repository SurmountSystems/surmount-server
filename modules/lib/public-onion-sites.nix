# Public HTTP Hosts that each get a distinct Arti v3 onion.
# Mail Hosts stay unmapped. HS private keys never appear here.
#
# Console (servicesHostname) keeps artiHiddenService.nickname so the
# already-published management onion identity stays that site.
# Other Hosts use nickname site-<hostname-with-dots-as-hyphens>.

{ lib }:
let
  inherit (lib) toLower;

  nicknameSlug =
    host:
    let
      lower = toLower host;
      chars = lib.stringToCharacters lower;
      mapped = map (c: if c == "." then "-" else c) chars;
    in
    lib.concatStrings mapped;

  siteNickname =
    {
      host,
      servicesHostname,
      consoleNickname,
    }:
    if toLower host == toLower servicesHostname then consoleNickname else "site-${nicknameSlug host}";

  publicHttpHosts =
    {
      primaryDomain,
      servicesHostname,
      mailHostname,
      extraStaticHosts,
      extraMailHostnames,
    }:
    let
      apex = toLower primaryDomain;
      www = "www.${apex}";
      services = toLower servicesHostname;
      mta = "mta-sts.${apex}";
      mail = toLower mailHostname;
      extraMail = map toLower extraMailHostnames;
      isMail = h: h == mail || builtins.elem h extraMail;
      extras = lib.filter (h: h != "" && !isMail h) (map toLower extraStaticHosts);
      base = lib.filter (h: h != "" && !isMail h) [
        apex
        www
        services
        mta
      ];
    in
    lib.unique (base ++ extras);

  publicOnionSites =
    {
      consoleNickname,
      servicesHostname,
      primaryDomain,
      mailHostname,
      extraStaticHosts,
      extraMailHostnames,
    }:
    map
      (host: {
        inherit host;
        nickname = siteNickname {
          inherit host servicesHostname consoleNickname;
        };
      })
      (publicHttpHosts {
        inherit
          primaryDomain
          servicesHostname
          mailHostname
          extraStaticHosts
          extraMailHostnames
          ;
      });

  nicknamesJsonMap =
    sites:
    lib.listToAttrs (
      map (s: {
        name = s.host;
        value = s.nickname;
      }) sites
    );
in
{
  inherit
    nicknameSlug
    siteNickname
    publicHttpHosts
    publicOnionSites
    nicknamesJsonMap
    ;
}
