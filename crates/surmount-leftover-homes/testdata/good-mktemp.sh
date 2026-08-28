#!/bin/sh
# Detector fixture only: allowed temp (must pass --paths).
WORKDIR="$(mktemp -d)"
IN_TREE_STAGE="${WORKDIR}/staging-must-not-remain"
mkdir -p "${IN_TREE_STAGE}/tls-cert"
# Host notes home is allowed (not repo .agents).
NOTE="${HOME}/.agents/reports/example.md"
printf 'ok\n' >"${NOTE}"
