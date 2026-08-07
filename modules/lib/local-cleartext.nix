# Shared derivation for local cleartext management API bind (Arti lean path).
# Lean onion reverse-proxy speaks cleartext HTTP (or unix:) to a local target.
# When the primary UI is listenMode=https (TLS), that same TCP target is wrong
# for Arti unless a dedicated loopback cleartext listener exists.
#
# Auto path: https UI + arti enable + no explicit arti backendAddress/UDS +
# no allowCleartextHttpsEscape + no explicit localCleartextListen override.
# Prefer TCP loopback (Arti UDS forward still has upstream gaps).
#
# Port pick always avoids ui.port and active redirect port so Linux cannot
# EADDRINUSE when primary is 0.0.0.0:P and cleartext wants 127.0.0.1:P
# (wildcard and loopback share the port).
#
# Living: docs/EDGE_AND_TLS.md, modules/arti-hidden-service.nix,
# modules/management-ui.nix. Never TLS-on-onion without a separate design.

{ lib }:
let
  inherit (lib) hasPrefix;

  isLoopbackListenAddr = a: a == "127.0.0.1" || a == "::1" || a == "localhost" || hasPrefix "127." a;

  # Explicit operator override (non-empty string).
  explicitLocalCleartext =
    ui:
    if ui.localCleartextListen != null && ui.localCleartextListen != "" then
      ui.localCleartextListen
    else
      null;

  # host:port or [v6]:port shape (trailing :digits).
  listenShapeOk = s: s == null || s == "" || builtins.match ".*:([0-9]+)$" s != null;

  # Parse trailing port (null if shape wrong).
  listenPort =
    s:
    let
      m = if s == null then null else builtins.match ".*:([0-9]+)$" s;
    in
    if m == null then null else lib.toInt (builtins.elemAt m 0);

  # Redirect listen port when product redirect is active; else null.
  activeRedirectPort =
    ui:
    if ui.redirectHttpToHttps && ui.httpRedirectListen != null && ui.httpRedirectListen != "" then
      listenPort ui.httpRedirectListen
    else
      null;

  # True when the product should auto-bind loopback cleartext API and point
  # Arti at it (no explicit backend, https primary without cleartext escape).
  needsAutoLocalCleartext =
    {
      ui,
      hs,
    }:
    ui.enable
    && hs.enable
    && ui.listenMode == "https"
    && !ui.allowCleartextHttpsEscape
    && hs.backendAddress == null
    && hs.backendUnixSocket == null
    && explicitLocalCleartext ui == null;

  # Port is free of primary and active redirect (Linux bind collision).
  portFree =
    ui: p:
    p != ui.port
    && (
      let
        rp = activeRedirectPort ui;
      in
      rp == null || p != rp
    )
    && p >= 1
    && p <= 65535;

  # Prefer classic 8090, then 8091, then primary+1, then 8190 / 18090.
  # Always 127.0.0.1 (numeric only; Rust SocketAddr does not resolve hostnames).
  deriveAutoLocalCleartextListen =
    ui:
    let
      alt = if ui.port >= 65535 then 8092 else ui.port + 1;
      chosen =
        if portFree ui 8090 then
          8090
        else if portFree ui 8091 then
          8091
        else if portFree ui alt then
          alt
        else if portFree ui 8190 then
          8190
        else
          18090;
    in
    "127.0.0.1:${toString chosen}";

  # Effective host:port for SURMOUNT_LOCAL_CLEARTEXT_LISTEN, or null.
  effectiveLocalCleartextListen =
    {
      ui,
      hs,
    }:
    let
      ex = explicitLocalCleartext ui;
    in
    if ex != null then
      ex
    else if needsAutoLocalCleartext { inherit ui hs; } then
      deriveAutoLocalCleartextListen ui
    else
      null;

  # Local cleartext must stay loopback-only (never public cleartext API).
  # Numeric literals only: Rust parse::<SocketAddr> does not resolve "localhost".
  isLoopbackCleartextTarget =
    s: s == null || s == "" || hasPrefix "127.0.0.1:" s || hasPrefix "[::1]:" s;

  # True when local cleartext port collides with primary or redirect port
  # (even if host strings differ: 0.0.0.0:P vs 127.0.0.1:P).
  localCleartextPortCollides =
    {
      ui,
      localListen,
    }:
    let
      lp = listenPort localListen;
      rp = activeRedirectPort ui;
    in
    lp != null && (lp == ui.port || (rp != null && lp == rp));
in
{
  inherit
    isLoopbackListenAddr
    needsAutoLocalCleartext
    deriveAutoLocalCleartextListen
    effectiveLocalCleartextListen
    explicitLocalCleartext
    listenShapeOk
    listenPort
    isLoopbackCleartextTarget
    activeRedirectPort
    localCleartextPortCollides
    portFree
    ;
}
