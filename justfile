# Surmount Server developer tasks.
# Host quality bar: just check = fmt (check-only) then clippy then test.
# Full flake aggregate: just ci (alias: just check-ci) -> nix build checks.<system>.ci
# GHA job display name is `just ci` (branch-protection check context footgun).
# Operator bins: nix run .#<app> -- args. just aliases only nix-run (no leftover script/*.sh).
# End-to-end: nix run .#e2e / nix run .#e2e-host. Host e2e is never in checks.ci.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Prefer CI_SYSTEM when set (GHA); else host flake system.
system := env_var_or_default("CI_SYSTEM", `nix eval --impure --raw --expr 'builtins.currentSystem'`)

# List recipes (default).
default:
    @just --list

# CI-style quality bar on the host (same order as typical CI gates):
#   fmt check (error if dirty) -> clippy (-D warnings) -> cargo test.
# Reverse of the common local loop (test, clippy, fmt). Does not write files.
# Does not run module-eval / flake aggregate; use `just ci` for that.
check: fmt clippy test

# Full flake CI aggregate (management-ui + e2e pure + module-eval + nixfmt + ops crates).
# Same as: nix build .#checks.<system>.ci
# Heavy mail-vm / stalwart FOD / full toplevel stay out (see check-heavy).
# GHA runs this recipe; job display name must stay `just ci`.
ci:
    nix build --print-build-logs ".#checks.{{system}}.ci"

# Alias kept so docs/muscle-memory (`just check-ci`) still work.
check-ci: ci

# Format check only (CI-style). Errors if Rust or Nix is not formatted.
# Does not rewrite files. Use `just fmt-write` to apply formatting.
fmt:
    cd crates && cargo fmt --check
    find modules tests hosts nix -name '*.nix' -print0 | xargs -0 -r nix run ".#formatter.{{system}}" -- --check
    nix run ".#formatter.{{system}}" -- --check flake.nix

# Apply formatting (writes). Not part of `just check`.
fmt-write:
    cd crates && cargo fmt
    find modules tests hosts nix -name '*.nix' -print0 | xargs -0 -r nix run ".#formatter.{{system}}" --
    nix run ".#formatter.{{system}}" -- flake.nix

# Rust unit/integration tests only (fast host loop). Hermetic crate tests live
# in flake checks.<system>.ci; prefer `just ci` when Nix is the quality bar.
test:
    cd crates && cargo test

# Clippy with warnings denied (CI-style).
clippy:
    cd crates && cargo clippy --all-targets -- -D warnings

# Local management console for day-to-day UI work (no VPS, no Stalwart required).
# Foreground: Ctrl-C to stop. Override any SURMOUNT_* env before running.
# Opens http://127.0.0.1:8080/ (or SURMOUNT_LISTEN).
# Auth default: SURMOUNT_AUTH_MODE=off (open console). To gate with Nostr:
#   export SURMOUNT_AUTH_MODE=nostr
#   export SURMOUNT_NOSTR_ALLOWLIST=npub1...
#   export SURMOUNT_SESSION_SECRET=$(openssl rand -hex 32)
# Product onion path is host surmount.artiHiddenService (hostname under
# onionServiceStateDir). Lab overrides: SURMOUNT_ONION_URL or
# SURMOUNT_ONION_HOSTNAME_FILE (optional local leftover path is lab only).
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    cd crates

    export SURMOUNT_LISTEN="${SURMOUNT_LISTEN:-127.0.0.1:8080}"
    export SURMOUNT_PRIMARY_DOMAIN="${SURMOUNT_PRIMARY_DOMAIN:-demo.local}"
    export SURMOUNT_MAIL_HOSTNAME="${SURMOUNT_MAIL_HOSTNAME:-mail.demo.local}"
    export SURMOUNT_SERVICES_HOSTNAME="${SURMOUNT_SERVICES_HOSTNAME:-services.demo.local}"
    export SURMOUNT_STALWART_URL="${SURMOUNT_STALWART_URL:-http://127.0.0.1:8081}"

    if [[ -z "${SURMOUNT_ONION_URL:-}" && -n "${SURMOUNT_ONION_HOSTNAME_FILE:-}" && -r "${SURMOUNT_ONION_HOSTNAME_FILE}" ]]; then
      line="$(tr -d '[:space:]' <"${SURMOUNT_ONION_HOSTNAME_FILE}" || true)"
      if [[ -n "$line" ]]; then
        export SURMOUNT_ONION_URL="$line"
      fi
    fi

    listen="$SURMOUNT_LISTEN"
    host_port="${listen##*:}"
    if command -v ss >/dev/null 2>&1; then
      while read -r pid; do
        [[ -z "${pid:-}" ]] && continue
        if [[ -r "/proc/$pid/cmdline" ]] \
          && tr '\0' ' ' <"/proc/$pid/cmdline" | grep -q 'surmount-management-ui'; then
          echo "dev: stopping prior surmount-management-ui (pid $pid) on $listen"
          kill "$pid" 2>/dev/null || true
          sleep 0.4
        fi
      done < <(ss -ltnp 2>/dev/null | sed -n "s/.*:${host_port} .*pid=\\([0-9][0-9]*\\).*/\\1/p" | sort -u)
    fi

    echo "dev: management console → http://${listen}/  (Stalwart probe ${SURMOUNT_STALWART_URL})"
    if [[ -n "${SURMOUNT_ONION_URL:-}" ]]; then
      echo "dev: onion surface → ${SURMOUNT_ONION_URL}"
    else
      echo "dev: onion not provisioned (host: surmount.artiHiddenService + hostname under onionServiceStateDir; lab: SURMOUNT_ONION_URL or SURMOUNT_ONION_HOSTNAME_FILE)"
    fi
    exec cargo run -p surmount-management-ui --bin surmount-management-ui

# Local comprehensive end-to-end (hermetic; optional Tor when present).
e2e:
    nix run ".#e2e"

# Host end-to-end (requires SURMOUNT_E2E_HOST=1; exit 2 if unset). Never a flake check.
e2e-host:
    nix run ".#e2e-host"

# Publish static sites (apex/www from github:SurmountSystems/site, plus extra vhosts).
# Docs: docs/OPS.md, docs/EDGE_AND_TLS.md
[positional-arguments]
deploy *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    nice -n 19 nix flake update surmount-site
    root="$(nice -n 19 nix build --print-build-logs --no-link --print-out-paths ".#surmount-public-site")"
    export SURMOUNT_PUBLIC_SITE_ROOT="${root}"
    exec nix run ".#surmount-deploy-static-sites" -- "$@"

# Operator deploy driver (public tree sync + host-local checks + #mail-vps).
# Docs: docs/deploy-host-local.md
[positional-arguments]
deploy-host *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-deploy-host" -- "$@"

[positional-arguments]
btop *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-btop-host" -- "$@"

[positional-arguments]
host-inxi *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-inxi-host" -- "$@"

[positional-arguments]
et *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-et" -- "$@"

[positional-arguments]
host-logs *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-host-logs" -- "$@"

# Targeted NixOS eval contracts (not module-eval-contract).
test-logging-eval:
    nix eval --impure --json --file tests/logging-contract.nix

test-sshd-hostkeys-eval:
    nix eval --impure --json --file tests/sshd-hostkeys-contract.nix

test-eternal-terminal-eval:
    nix eval --impure --json --file tests/eternal-terminal-contract.nix

test-inxi-eval:
    nix eval --impure --json --file tests/inxi-contract.nix

test-remote-builder-eval:
    nix eval --impure --json --file tests/remote-builder-contract.nix

[positional-arguments]
check-private-data *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-private-data" -- "$@"

alias private-data := check-private-data

[positional-arguments]
check-leftover-agent-homes *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-leftover-homes" -- "$@"

# D1 host hybrid TLS negotiation probe (operator; env-gated). Never a flake check.
check-tls-hybrid:
    nix run ".#surmount-tls-hybrid"

# Interactive no-echo Domain A secret intake (laptop). Docs: docs/SECRETS.md
[positional-arguments]
secrets-prompt *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-secrets-prompt" -- "$@"

[positional-arguments]
diskstation-afp-mount *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-diskstation-afp-mount" -- "$@"

[positional-arguments]
diskstation-discover *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-diskstation-discover" -- "$@"

[positional-arguments]
sync-static-sites-from-ds3018xs *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-sync-static-sites" -- "$@"

[positional-arguments]
mailplus-copy-uid *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-copy-mailplus-uid" -- "$@"

[positional-arguments]
add-stalwart-token *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#add-stalwart-token" -- "$@"

[positional-arguments]
bootstrap-stalwart-token *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#bootstrap-stalwart-api-token" -- "$@"

[positional-arguments]
stalwart-recovery-unlock *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#stalwart-recovery-unlock" -- "$@"

[positional-arguments]
fix-public-dashboard *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-fix-public-dashboard" -- "$@"

[positional-arguments]
secrets-install-host *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#secrets-install-host" -- "$@"

[positional-arguments]
secrets-export-bw-to-staging *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#secrets-export-bw-to-staging" -- "$@"

[positional-arguments]
host-cutover *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-host-cutover" -- "$@"

[positional-arguments]
cutover-step step *args:
    #!/usr/bin/env bash
    set -euo pipefail
    step="{{step}}"
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-host-cutover" -- --step "${step}" "$@"

[positional-arguments]
render-host-profile-acme *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-render-host-profile-acme" -- "$@"

[positional-arguments]
domain-audit *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-domain-audit" -- "$@"

[positional-arguments]
dns-zone-namecheap *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-dns-zone" -- "$@"

[positional-arguments]
rdns-shc *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-shc" -- "$@"

[positional-arguments]
free-stalwart-public-443 *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#free-stalwart-public-443" -- "$@"

[positional-arguments]
register-dkim *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#register-dkim" -- "$@"

[positional-arguments]
point-stalwart-mail-tls *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#point-stalwart-mail-tls" -- "$@"

[positional-arguments]
laptop-renew-cert *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-laptop-renew-cert" -- "$@"

# Optional heavy checks (not in `just check` / `just ci`).
check-heavy:
    nix build --print-build-logs ".#checks.{{system}}.mail-vm-test"
