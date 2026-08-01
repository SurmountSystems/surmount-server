#!/usr/bin/env bash
# TLS handshake + certificate date check (host end-to-end helper).
# Read-only. Requires: openssl.
# Usage:
#   ./scripts/check-tls.sh host:port [servername]
# Example:
#   ./scripts/check-tls.sh services.surmount.systems:443
#   ./scripts/check-tls.sh mail.surmount.systems:993 mail.surmount.systems
#
# Standalone ops helper. Host e2e (`nix run .#e2e-host`) embeds the same check
# in Rust when SURMOUNT_E2E_TLS_HOST or BASE_URL is set.
# Local comprehensive e2e uses in-process self-signed PEMs (nix run .#e2e), not this.
#
# See docs/EDGE_AND_TLS.md and docs/OPS.md.

set -euo pipefail

TARGET="${1:-}"
if [[ -z "${TARGET}" ]]; then
  echo "Usage: $0 host:port [servername]" >&2
  exit 2
fi

HOST="${TARGET%%:*}"
PORT="${TARGET##*:}"
SERVERNAME="${2:-$HOST}"

if ! command -v openssl >/dev/null 2>&1; then
  echo "error: openssl not found" >&2
  exit 127
fi

echo "== TLS check ${HOST}:${PORT} (SNI ${SERVERNAME}) =="

# Show peer cert summary and dates. s_client exits non-zero on some verify
# failures; we still want the human-readable dump.
set +e
OUT="$(
  echo | openssl s_client -connect "${HOST}:${PORT}" -servername "${SERVERNAME}" 2>/dev/null \
    | openssl x509 -noout -subject -issuer -dates 2>/dev/null
)"
RC=$?
set -e

if [[ -z "${OUT}" ]]; then
  echo "error: could not retrieve certificate (connect or handshake failed)" >&2
  exit 1
fi

echo "${OUT}"

# Expiry: expired cert fails host end-to-end (exit 1). Under 30 days warns only.
NOT_AFTER="$(echo "${OUT}" | sed -n 's/^notAfter=//p' | head -n1)"
if [[ -n "${NOT_AFTER}" ]] && command -v date >/dev/null 2>&1; then
  if EXP_EPOCH="$(date -d "${NOT_AFTER}" +%s 2>/dev/null)"; then
    NOW_EPOCH="$(date +%s)"
    DAYS=$(( (EXP_EPOCH - NOW_EPOCH) / 86400 ))
    echo "days_until_expiry=${DAYS}"
    if (( DAYS < 0 )); then
      echo "error: certificate appears expired" >&2
      exit 1
    elif (( DAYS < 30 )); then
      echo "warning: certificate expires in under 30 days" >&2
    fi
  fi
fi

exit 0
