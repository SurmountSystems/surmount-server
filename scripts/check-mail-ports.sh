#!/usr/bin/env bash
# TCP reachability smoke for mail-related ports.
# Read-only. Prefers nc (nmap-ncat or openbsd-netcat); falls back to bash /dev/tcp.
# Usage:
#   ./scripts/check-mail-ports.sh [host]
# Default host: mail.surmount.systems
#
# Does not authenticate or send mail. See docs/OPS.md and docs/DNS.md.

set -euo pipefail

HOST="${1:-mail.surmount.systems}"
PORTS=(25 465 587 993 4190 80 443)

probe() {
  local host="$1"
  local port="$2"
  if command -v nc >/dev/null 2>&1; then
    # -z scan, -w timeout seconds (flag spelling varies slightly; keep short)
    if nc -z -w 3 "$host" "$port" >/dev/null 2>&1; then
      return 0
    fi
    return 1
  fi
  # bash /dev/tcp fallback
  if timeout 3 bash -c "echo >/dev/tcp/${host}/${port}" >/dev/null 2>&1; then
    return 0
  fi
  return 1
}

echo "== Mail port check host=${HOST} =="
fail=0
for p in "${PORTS[@]}"; do
  if probe "$HOST" "$p"; then
    printf 'OK   %s:%s\n' "$HOST" "$p"
  else
    printf 'FAIL %s:%s\n' "$HOST" "$p"
    fail=1
  fi
done

if (( fail != 0 )); then
  echo "One or more ports failed. From outside, confirm firewall and service bind." >&2
  echo "Loopback-only services (UI 8080, Stalwart HTTP 8081) are expected closed publicly." >&2
  exit 1
fi

echo "All listed ports accepted TCP connect."
