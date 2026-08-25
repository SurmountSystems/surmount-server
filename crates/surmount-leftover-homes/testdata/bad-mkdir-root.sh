#!/usr/bin/env bash
# Detector fixture only: ROOT-relative leftover home (must fail --paths).
# Synthetic. Never copy this pattern into product scripts.
ROOT="/tmp/surmount-fixture-root"
IN_TREE_STAGE="${ROOT}/.agents/reports/.tmp-cutover-staging-must-not-remain"
mkdir -p "${IN_TREE_STAGE}/tls-cert"
