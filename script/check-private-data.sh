#!/usr/bin/env bash
# Private-data gate for Surmount Server (patterns only).
#
# Scans for high-confidence private material classes via ripgrep regexes.
# Never embeds real IPs, keys, tokens, or host private data in this script.
#
# Modes:
#   --staged   Scan staged files (ACM). Default when no mode given.
#   --tree     Scan tracked files (git ls-files), excluding detector fixtures.
#   --paths P  Scan the given paths only (for detector tests / fixtures).
#
# Exit 0 = clean. Exit 1 = hit(s) or usage/tool error.
# On hit: print class + file path only (not the secret line).
set -euo pipefail

usage() {
  cat <<'EOF' >&2
Usage:
  script/check-private-data.sh [--staged]
  script/check-private-data.sh --tree
  script/check-private-data.sh --paths PATH [PATH ...]

Scans for private-data pattern classes (PEM, age secret, tokens, secret
basenames, path-gated public IPv4 under hosts/, long SSH public keys).
Patterns only: no real secrets are stored in this tool.

  --staged   Staged files only (git diff --cached). Default.
  --tree     All tracked files (excludes script/testdata/private-data/).
  --paths    Explicit paths (fixtures; does not exclude testdata).

Requires: rg (ripgrep) on PATH.
EOF
}

if ! command -v rg >/dev/null 2>&1; then
  echo "check-private-data: rg (ripgrep) is required but not on PATH." >&2
  echo "  Install ripgrep, or enter a nix shell that provides it." >&2
  exit 1
fi

MODE=""
PATHS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h | --help)
      usage
      exit 0
      ;;
    --staged)
      MODE="staged"
      shift
      ;;
    --tree)
      MODE="tree"
      shift
      ;;
    --paths)
      MODE="paths"
      shift
      if [[ $# -eq 0 ]]; then
        echo "check-private-data: --paths requires at least one path" >&2
        exit 1
      fi
      while [[ $# -gt 0 ]]; do
        case "$1" in
          --*)
            break
            ;;
          *)
            PATHS+=("$1")
            shift
            ;;
        esac
      done
      ;;
    *)
      echo "check-private-data: unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ -z "${MODE}" ]]; then
  MODE="staged"
fi

REPO_ROOT=""
if REPO_ROOT=$(git rev-parse --show-toplevel 2>/dev/null); then
  :
else
  REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi

HIT=0
report_hit() {
  local class="$1"
  local path="$2"
  printf 'private-data: %s  %s\n' "${class}" "${path}" >&2
  HIT=1
}

is_fixture_path() {
  local p="$1"
  case "${p}" in
    script/testdata/private-data | script/testdata/private-data/* | \
    ./script/testdata/private-data | ./script/testdata/private-data/*)
      return 0
      ;;
  esac
  return 1
}

# Scanner / self-test source may document pattern shapes. Never treat them as
# product hits in --staged / --tree (fixtures still use --paths).
is_scanner_tool_path() {
  local p="$1"
  case "${p}" in
    script/check-private-data.sh | ./script/check-private-data.sh | \
    script/test-check-private-data.sh | ./script/test-check-private-data.sh)
      return 0
      ;;
  esac
  return 1
}

check_basename() {
  local path="$1"
  local base
  base="$(basename -- "${path}")"

  # secrets/README.md is layout docs only (not secrets.yaml material).
  if [[ "${path}" == "secrets/README.md" || "${path}" == "./secrets/README.md" ]]; then
    return 0
  fi

  case "${base}" in
    .env | .env.*)
      report_hit "secret-basename:.env" "${path}"
      ;;
    keys.txt)
      report_hit "secret-basename:keys.txt" "${path}"
      ;;
    id_rsa | id_ed25519 | id_ecdsa | id_dsa)
      report_hit "secret-basename:ssh-private-name" "${path}"
      ;;
    secrets.yaml | secrets.yml)
      report_hit "secret-basename:secrets.yaml" "${path}"
      ;;
  esac

  case "${base}" in
    *.pem)
      report_hit "secret-basename:pem" "${path}"
      ;;
    *.agekey)
      report_hit "secret-basename:agekey" "${path}"
      ;;
    *.p12 | *.pfx)
      report_hit "secret-basename:pkcs12" "${path}"
      ;;
  esac

  case "${base}" in
    *decrypted*)
      report_hit "secret-basename:decrypted" "${path}"
      ;;
  esac
}

path_is_hosts_or_secrets() {
  local p="$1"
  case "${p}" in
    hosts/* | ./hosts/* | secrets/* | ./secrets/*)
      return 0
      ;;
  esac
  return 1
}

path_is_hosts() {
  local p="$1"
  case "${p}" in
    hosts/* | ./hosts/*)
      return 0
      ;;
  esac
  return 1
}

# Allowlisted IPv4 ranges for hosts/** (docs / private / link-local / all-bind).
# Patterns only: never real production addresses.
ipv4_is_allowed() {
  local ip="$1"
  if [[ "${ip}" == "0.0.0.0" ]]; then
    return 0
  fi
  case "${ip}" in
    127.* | 10.* | 192.168.* | 169.254.*)
      return 0
      ;;
    203.0.113.* | 198.51.100.* | 192.0.2.*)
      return 0
      ;;
  esac
  # 172.16.0.0/12
  if [[ "${ip}" =~ ^172\.(1[6-9]|2[0-9]|3[0-1])\. ]]; then
    return 0
  fi
  return 1
}

# Path-gated checks apply under hosts/secrets for product modes; for --paths
# (fixtures) always apply so detector tests can live under script/testdata/.
want_path_gated() {
  local rel="$1"
  if [[ "${MODE}" == "paths" ]]; then
    return 0
  fi
  path_is_hosts_or_secrets "${rel}"
}

want_hosts_ipv4() {
  local rel="$1"
  if [[ "${MODE}" == "paths" ]]; then
    return 0
  fi
  path_is_hosts "${rel}"
}

# Scan content of a regular file at abs; report hits against rel path.
scan_content() {
  local abs="$1"
  local rel="$2"

  if [[ ! -f "${abs}" || ! -r "${abs}" ]]; then
    return 0
  fi
  # Text patterns only. Do not use grep with a NUL pattern: C-string tools
  # treat "\0" as an empty pattern and match every file. rg skips binary by default.
  #
  # PEM headers are assembled at runtime so this script source never contains a
  # contiguous matchable private-key header (would false-positive on itself).
  local pem_begin='-----BEGIN'
  local pem_end='-----'
  if rg -q --no-messages \
    -e "${pem_begin} ([A-Z0-9]+ )?PRIVATE KEY${pem_end}" \
    -e "${pem_begin} OPENSSH PRIVATE KEY${pem_end}" \
    -e "${pem_begin} ENCRYPTED PRIVATE KEY${pem_end}" \
    -- "${abs}"; then
    report_hit "pem-or-openssh-private-key" "${rel}"
  fi

  if rg -q --no-messages -e 'AGE-SECRET-KEY-1[A-Z0-9]+' -- "${abs}"; then
    report_hit "age-secret-key" "${rel}"
  fi

  if rg -q --no-messages \
    -e '\bghp_[A-Za-z0-9]{20,}\b' \
    -e '\bgithub_pat_[A-Za-z0-9_]{20,}\b' \
    -e '\bsk_live_[A-Za-z0-9]{16,}\b' \
    -e '\bAKIA[0-9A-Z]{16}\b' \
    -e '\bxox[baprs]-[A-Za-z0-9-]{10,}\b' \
    -- "${abs}"; then
    report_hit "api-token-shape" "${rel}"
  fi

  if rg -q --no-messages -e '\bnsec1[a-z0-9]{20,}\b' -- "${abs}"; then
    report_hit "nostr-nsec" "${rel}"
  fi

  if rg -q --no-messages -i \
    -e '\b(password|secret|token|api_key)\b[[:space:]]*[=:][[:space:]]*['\''"][^'\''"]{8,}['\''"]' \
    -- "${abs}"; then
    report_hit "secret-assignment" "${rel}"
  fi

  if want_path_gated "${rel}"; then
    if rg -q --no-messages \
      -e 'ssh-(ed25519|rsa|ecdsa|dss)[[:space:]]+AAAA[A-Za-z0-9+/]{40,}' \
      -- "${abs}"; then
      report_hit "long-ssh-public-key" "${rel}"
    fi
  fi

  if want_hosts_ipv4 "${rel}"; then
    local ip
    while IFS= read -r ip; do
      [[ -z "${ip}" ]] && continue
      if ! ipv4_is_allowed "${ip}"; then
        report_hit "hosts-public-ipv4" "${rel}"
        break
      fi
    done < <(rg -oN --no-messages -e '[0-9]{1,3}(\.[0-9]{1,3}){3}' -- "${abs}" 2>/dev/null | sort -u || true)
  fi
}

to_repo_rel() {
  local path="$1"
  if [[ "${path}" == /* ]]; then
    case "${path}" in
      "${REPO_ROOT}"/*)
        printf '%s\n' "${path#"${REPO_ROOT}"/}"
        return 0
        ;;
    esac
    printf '%s\n' "${path}"
    return 0
  fi
  printf '%s\n' "${path#./}"
}

scan_one() {
  local path="$1"
  local rel
  rel="$(to_repo_rel "${path}")"

  check_basename "${rel}"

  if [[ "${MODE}" == "staged" ]]; then
    local tmp
    tmp="$(mktemp)"
    if git show ":${rel}" >"${tmp}" 2>/dev/null; then
      scan_content "${tmp}" "${rel}"
    fi
    rm -f "${tmp}"
  else
    local abs="${path}"
    if [[ "${path}" != /* ]]; then
      if [[ -f "${path}" ]]; then
        abs="$(cd "$(dirname -- "${path}")" && pwd)/$(basename -- "${path}")"
      else
        abs="${REPO_ROOT}/${path}"
      fi
    fi
    scan_content "${abs}" "${rel}"
  fi
}

declare -a FILES=()

case "${MODE}" in
  staged)
    if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      echo "check-private-data: --staged requires a git work tree" >&2
      exit 1
    fi
    while IFS= read -r f; do
      [[ -z "${f}" ]] && continue
      if is_fixture_path "${f}" || is_scanner_tool_path "${f}"; then
        continue
      fi
      FILES+=("${f}")
    done < <(git diff --cached --name-only --diff-filter=ACM)
    ;;
  tree)
    if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      echo "check-private-data: --tree requires a git work tree" >&2
      exit 1
    fi
    while IFS= read -r f; do
      [[ -z "${f}" ]] && continue
      if is_fixture_path "${f}" || is_scanner_tool_path "${f}"; then
        continue
      fi
      FILES+=("${f}")
    done < <(git ls-files)
    ;;
  paths)
    if [[ ${#PATHS[@]} -eq 0 ]]; then
      echo "check-private-data: --paths requires at least one path" >&2
      exit 1
    fi
    # Caller (or shell) expands globs into PATHS; also expand relative to repo.
    for p in "${PATHS[@]}"; do
      if [[ -e "${p}" ]]; then
        FILES+=("${p}")
        continue
      fi
      if [[ -e "${REPO_ROOT}/${p}" ]]; then
        FILES+=("${REPO_ROOT}/${p}")
        continue
      fi
      shopt -s nullglob
      matched=0
      for g in ${p} "${REPO_ROOT}"/${p}; do
        if [[ -e "${g}" ]]; then
          FILES+=("${g}")
          matched=1
        fi
      done
      shopt -u nullglob
      if [[ "${matched}" -eq 0 ]]; then
        echo "check-private-data: path not found: ${p}" >&2
        exit 1
      fi
    done
    if [[ ${#FILES[@]} -eq 0 ]]; then
      echo "check-private-data: --paths matched no files" >&2
      exit 1
    fi
    ;;
esac

for f in "${FILES[@]+"${FILES[@]}"}"; do
  scan_one "${f}"
done

if [[ "${HIT}" -ne 0 ]]; then
  cat <<'EOF' >&2

check-private-data: refused. Private-data pattern class matched.
  Remove the material from the index / tree. Keep keys, tokens, PEMs, age
  identities, and real host public IPs on the host only (never in this public
  git tree). Use TEST-NET (203.0.113.0/24, 198.51.100.0/24, 192.0.2.0/24),
  loopback, and short placeholders in committed samples.
  See docs/hygiene.md (pre-commit private-data scan) and docs/SECRETS.md.
EOF
  exit 1
fi

exit 0
