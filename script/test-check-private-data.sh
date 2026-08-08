#!/usr/bin/env bash
# Red/green harness for script/check-private-data.sh
# Uses synthetic fixtures under script/testdata/private-data/ only.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

SCANNER="${ROOT}/script/check-private-data.sh"
FIXTURE_DIR="${ROOT}/script/testdata/private-data"
FAIL=0

if [[ ! -x "${SCANNER}" ]]; then
  echo "test-check-private-data: scanner not executable: ${SCANNER}" >&2
  exit 1
fi

pass() {
  echo "  PASS: $1"
}

fail() {
  echo "  FAIL: $1" >&2
  FAIL=1
}

echo "==> green: --tree on repo (product tree, fixtures excluded)"
if "${SCANNER}" --tree; then
  pass "--tree exits 0"
else
  fail "--tree exited non-zero on clean product tree"
fi

echo "==> green: good host sample fixture"
if "${SCANNER}" --paths "${FIXTURE_DIR}/good-host-sample.txt"; then
  pass "good-host-sample.txt exits 0"
else
  fail "good-host-sample.txt should pass (TEST-NET + short SSH placeholder)"
fi

echo "==> green: sample host configuration"
if "${SCANNER}" --paths "${ROOT}/hosts/mail-vps/configuration.nix"; then
  pass "hosts/mail-vps/configuration.nix exits 0"
else
  fail "hosts/mail-vps/configuration.nix should pass"
fi

echo "==> red: each bad-* fixture must fail"
shopt -s nullglob
bad_files=("${FIXTURE_DIR}"/bad-*.txt)
if [[ ${#bad_files[@]} -eq 0 ]]; then
  fail "no bad-*.txt fixtures found under ${FIXTURE_DIR}"
else
  for f in "${bad_files[@]}"; do
    base="$(basename -- "${f}")"
    if "${SCANNER}" --paths "${f}" >/dev/null 2>&1; then
      fail "${base} should exit non-zero"
    else
      pass "${base} exits non-zero"
    fi
  done
fi
shopt -u nullglob

echo "==> red: basename gate (temp pem name)"
tmpd="$(mktemp -d)"
# shellcheck disable=SC2064
trap 'rm -rf "'"${tmpd}"'"' EXIT
: >"${tmpd}/id_ed25519"
if "${SCANNER}" --paths "${tmpd}/id_ed25519" >/dev/null 2>&1; then
  fail "basename id_ed25519 should fail"
else
  pass "basename id_ed25519 exits non-zero"
fi

echo "==> red: multi bad paths"
if "${SCANNER}" --paths \
  "${FIXTURE_DIR}/bad-pem.txt" \
  "${FIXTURE_DIR}/bad-age.txt" \
  >/dev/null 2>&1; then
  fail "multi bad --paths should exit non-zero"
else
  pass "multi bad --paths exits non-zero"
fi

if [[ "${FAIL}" -ne 0 ]]; then
  echo "test-check-private-data: FAILED" >&2
  exit 1
fi

echo "test-check-private-data: all checks passed"
exit 0
