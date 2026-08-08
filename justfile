# Surmount Server developer tasks.
# Host quality bar: just check = fmt (check-only) then clippy then test (CI-style).
# GitHub Actions / full flake aggregate: just ci (alias: just check-ci).
#   -> nix build checks.<system>.ci
# GHA job display name is `just ci` (branch-protection check context footgun;
# see .github/workflows/ci.yml and grok-oss ci.yml).
# End-to-end SoT: nix run .#e2e (local) and nix run .#e2e-host (env-gated host).
# just e2e / e2e-host are thin wrappers. Host e2e is never in checks.ci.

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

# Full flake CI aggregate (management-ui + e2e pure + module-eval + nixfmt, etc.).
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
# Nix uses the flake formatter (nixfmt-rfc-style) so host PATH need not include nixfmt.
fmt-write:
    cd crates && cargo fmt
    find modules tests hosts nix -name '*.nix' -print0 | xargs -0 -r nix run ".#formatter.{{system}}" --
    nix run ".#formatter.{{system}}" -- flake.nix

# Rust unit/integration tests only (fast host loop).
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
# If a prior local onion demo left onion.url, that address is surfaced automatically.
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    # Recipes run from the directory that contains this justfile.
    cd crates

    export SURMOUNT_LISTEN="${SURMOUNT_LISTEN:-127.0.0.1:8080}"
    export SURMOUNT_PRIMARY_DOMAIN="${SURMOUNT_PRIMARY_DOMAIN:-demo.local}"
    export SURMOUNT_MAIL_HOSTNAME="${SURMOUNT_MAIL_HOSTNAME:-mail.demo.local}"
    export SURMOUNT_SERVICES_HOSTNAME="${SURMOUNT_SERVICES_HOSTNAME:-services.demo.local}"
    export SURMOUNT_STALWART_URL="${SURMOUNT_STALWART_URL:-http://127.0.0.1:8081}"

    # Optional: surface a previously published local demo onion (never invent one).
    onion_file="${SURMOUNT_ONION_HOSTNAME_FILE:-/tmp/surmount-local-onion-demo/onion.url}"
    if [[ -z "${SURMOUNT_ONION_URL:-}" && -r "$onion_file" ]]; then
      line="$(tr -d '[:space:]' <"$onion_file" || true)"
      if [[ -n "$line" ]]; then
        export SURMOUNT_ONION_URL="$line"
      fi
    fi

    listen="$SURMOUNT_LISTEN"
    host_port="${listen##*:}"
    # Free only our binary on this port (never pkill -f; avoids self-kill).
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
      echo "dev: onion not configured (set SURMOUNT_ONION_URL or run a local Arti demo)"
    fi
    exec cargo run -p surmount-management-ui --bin surmount-management-ui

# Local comprehensive end-to-end (hermetic; optional Tor when present).
# SoT: nix run .#e2e (Rust binary packages.e2e).
e2e:
    nix run ".#e2e"

# Host end-to-end (requires SURMOUNT_E2E_HOST=1; exit 2 if unset).
# SoT: nix run .#e2e-host. Never a flake check.
e2e-host:
    nix run ".#e2e-host"

# Optional heavy checks (not in `just check` / `just ci`).
check-heavy:
    nix build --print-build-logs ".#checks.{{system}}.mail-vm-test"
