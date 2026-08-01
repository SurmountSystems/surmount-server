#!/usr/bin/env bash
# DNS smoke checks for Surmount mail hosts.
# Read-only. Requires: dig (bind/dnsutils).
# Usage:
#   ./scripts/check-dns.sh [apex-domain] [mail-host]
# Defaults: surmount.systems mail.surmount.systems
#
# See docs/DNS.md and docs/OPS.md. Prefer packaging as a flake app later.

set -euo pipefail

APEX="${1:-surmount.systems}"
MAIL_HOST="${2:-mail.${APEX}}"

if ! command -v dig >/dev/null 2>&1; then
  echo "error: dig not found (install dnsutils / bind-utils)" >&2
  exit 127
fi

section() {
  printf '\n== %s ==\n' "$1"
}

lookup() {
  local type="$1"
  local name="$2"
  echo "-- ${type} ${name}"
  dig +short "$type" "$name" || true
}

section "A/AAAA apex and mail"
lookup A "$APEX"
lookup AAAA "$APEX"
lookup A "$MAIL_HOST"
lookup AAAA "$MAIL_HOST"

section "MX ${APEX}"
lookup MX "$APEX"

section "NS ${APEX}"
lookup NS "$APEX"

section "TXT SPF/DMARC (presence only)"
lookup TXT "$APEX"
lookup TXT "_dmarc.${APEX}"

section "PTR hint (needs mail IP)"
MAIL_IP="$(dig +short A "$MAIL_HOST" | head -n1 || true)"
if [[ -n "${MAIL_IP}" ]]; then
  echo "-- PTR for ${MAIL_IP}"
  dig +short -x "$MAIL_IP" || true
else
  echo "no A record for ${MAIL_HOST}; skip PTR"
fi

section "Done"
echo "Review docs/DNS.md for expected values (DKIM selector is operator-specific)."
echo "This script does not validate correctness of SPF/DKIM/DMARC policy."
