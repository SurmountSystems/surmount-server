# Cert-copy chown must not swallow a failure.
# Reads modules/mail.nix and modules/management-ui.nix from disk.
# A line that chowns stalwart-mail:stalwart-mail "$dst" and also runs
# "||" on that same command fails this eval. Other "|| true" lines do not.
#
#   nix-instantiate --eval --strict tests/cert-chown-fails.nix
let
  paths = {
    mail = ../modules/mail.nix;
    managementUi = ../modules/management-ui.nix;
  };

  splitLines = text: builtins.filter builtins.isString (builtins.split "\n" text);

  hasInfix =
    needle: hay:
    let
      nlen = builtins.stringLength needle;
      hlen = builtins.stringLength hay;
      go =
        i:
        if i + nlen > hlen then
          false
        else if builtins.substring i nlen hay == needle then
          true
        else
          go (i + 1);
    in
    if nlen == 0 then true else go 0;

  # The cert destination in both copies is the mail/tls directory variable.
  chownCmd = "chown stalwart-mail:stalwart-mail \"$dst\"";

  check =
    name: path:
    let
      text = builtins.readFile path;
      lines = splitLines text;
      chownLines = builtins.filter (line: hasInfix chownCmd line) lines;
      swallowLines = builtins.filter (line: hasInfix "||" line) chownLines;
    in
    if chownLines == [ ] then
      throw ("cert-chown-fails: " + name + " has no cert destination chown of \"$dst\"")
    else if swallowLines != [ ] then
      throw (
        "cert-chown-fails: "
        + name
        + " swallows cert destination chown failure: "
        + builtins.concatStringsSep " | " swallowLines
      )
    else
      name + "-ok";
in
{
  ok = [
    (check "modules/mail.nix" paths.mail)
    (check "modules/management-ui.nix" paths.managementUi)
  ];
}
