# Surmount Server developer tasks.
# Host quality bar: just check = fmt (check-only) then clippy then test.
# Full flake aggregate: just ci (alias: just check-ci) -> nix build checks.<system>.ci
# GHA job display name is `just ci` (branch-protection check context footgun).
# Operator bins: nix run .#<app> -- args. just aliases only nix-run (no leftover script/*.sh).
# End-to-end: nix run .#e2e / nix run .#e2e-host. Host e2e is never in checks.ci.
# Guest (remote builder) down: BUILD_LOCAL=true just <recipe>
#   sets NIX_CONFIG builders empty so this laptop compiles. Not a SKU.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Honor BUILD_LOCAL for every recipe that calls nix (including nix run).
export NIX_CONFIG := if env_var_or_default("BUILD_LOCAL", "") == "" { env_var_or_default("NIX_CONFIG", "") } else { "builders =\nmax-jobs = auto\n" }

# ssh-ng builder client. Nix 2.34 prepends NIX_SSHOPTS, and OpenSSH keeps
# the first value of each option. Nix does not set these itself. This does
# not retry a nixos-rebuild switch. remote-nix-build retries a dropped
# nix build only. An operator-set NIX_SSHOPTS is left as-is.
export NIX_SSHOPTS := env_var_or_default("NIX_SSHOPTS", "-o ServerAliveInterval=30 -o ServerAliveCountMax=10 -o TCPKeepAlive=yes -o ConnectionAttempts=3")

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

# Force the same `just ci` aggregate onto the ssh-ng builder (max-jobs 0).
# This laptop must not rustc. Refuses BUILD_LOCAL. Not GHA. Not grok-build's
# workspace-cargo-quality / nextest helper. Guest must list surmount-remote.
# Flake eval is local and can take minutes with no rustc; say that first.
# Do not pass -v/--debug: one -v prints every nixpkgs file (flood).
# -L streams remote rustc. --print-out-paths prints the store path on
# success so a cache hit is not a blank prompt. No --log-format raw:
# that hid "these N derivations" / waiting-for-machine and looked dead.
# No 20s heartbeat. No --store ssh-ng. Machines file plus max-jobs 0.
# NIX_SSHOPTS keepalives are separate from that heartbeat. A dropped
# ssh-ng connection retries the nix build. A derivation failure does not.
# This is not a nixos-rebuild switch.
check-remote:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -n "${BUILD_LOCAL:-}" ]]; then
      echo "just check-remote: refuse BUILD_LOCAL (that is laptop rustc)" >&2
      exit 2
    fi
    exec just remote-nix-build ".#checks.{{system}}.ci" "check-remote"

# Same force-remote path for cargo-audit (also in checks.ci).
# Same ssh-ng keepalive and connection retry as check-remote.
# A derivation failure is not retried.
audit-remote:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -n "${BUILD_LOCAL:-}" ]]; then
      echo "just audit-remote: refuse BUILD_LOCAL" >&2
      exit 2
    fi
    exec just remote-nix-build ".#checks.{{system}}.cargo-audit" "audit-remote"

# One ssh-ng nix build. Retries a dropped connection. Does not retry a
# derivation that actually failed, and does not run nixos-rebuild.
remote-nix-build installable label:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -n "${BUILD_LOCAL:-}" ]]; then
      echo "just {{label}}: refuse BUILD_LOCAL (that is laptop rustc)" >&2
      exit 2
    fi
    installable="{{installable}}"
    label="{{label}}"
    echo "==> just ${label}: nix build ${installable} (max-jobs 0)" >&2
    echo "==> just ${label}: eval can take a few minutes; then remote logs or a store path" >&2
    echo "==> just ${label}: ssh-ng keepalives via NIX_SSHOPTS; connection drops retry; derivation failures do not" >&2

    is_connection_drop() {
      local log="$1"
      # A finished remote builder (rustc, cargo fmt, and the like) is not a drop.
      if grep -E -q 'builder failed with exit code|Reason: builder failed' "$log"; then
        return 1
      fi
      grep -E -q 'failed to start SSH connection|failed to start SSH master connection|unable to open connection to remote store|unexpected end-of-file|unexpected EOF|kex_exchange_identification|Connection reset by peer|Connection timed out|Connection refused|Broken pipe|closed by remote host|Timeout, server .+ not responding' "$log"
    }

    max=3
    attempt=1
    log=""
    outf=""
    fifo=""
    trap 'rm -f "${log:-}" "${outf:-}" "${fifo:-}"' EXIT
    while [[ "$attempt" -le "$max" ]]; do
      log="$(mktemp)"
      outf="$(mktemp)"
      fifo="$(mktemp -u)"
      mkfifo "$fifo"
      tee "$log" <"$fifo" >&2 &
      tee_pid=$!
      set +e
      nix build --option max-jobs 0 --option warn-dirty false --print-build-logs --print-out-paths --no-link "$installable" >"$outf" 2>"$fifo"
      rc=$?
      set -e
      wait "$tee_pid" || true
      rm -f "$fifo"
      fifo=""
      if [[ "$rc" -eq 0 ]]; then
        echo "==> just ${label}: ok" >&2
        cat "$outf"
        rm -f "$log" "$outf"
        log=""
        outf=""
        exit 0
      fi
      if is_connection_drop "$log"; then
        rm -f "$log" "$outf"
        log=""
        outf=""
        if [[ "$attempt" -lt "$max" ]]; then
          echo "==> just ${label}: connection failed (attempt ${attempt} of ${max}); retrying the nix build" >&2
          attempt=$((attempt + 1))
          sleep 2
          continue
        fi
        echo "==> just ${label}: connection failed after ${max} attempts" >&2
        exit 1
      fi
      rm -f "$log" "$outf"
      log=""
      outf=""
      echo "==> just ${label}: build failed (not a connection drop); not retrying" >&2
      exit "$rc"
    done
    echo "==> just ${label}: connection failed after ${max} attempts" >&2
    exit 1

# Offline RustSec audit of crates/Cargo.lock (advisory-db flake input).
# warn-dirty is off because this command runs after nix flake update
# and cargo update, while those lock files are still uncommitted.
audit:
    nix build --option warn-dirty false --print-build-logs ".#checks.{{system}}.cargo-audit"

# Ban sha1/md5 crates in the lockfile (cargo-deny bans).
deny:
    nix build --print-build-logs ".#checks.{{system}}.cargo-deny-bans"

# Alias kept so docs/muscle-memory (`just check-ci`) still work.
check-ci: ci

# Format check only (CI-style). Errors if Rust or Nix is not formatted.
# Does not rewrite files. Use `just fmt-write` to apply formatting.
fmt:
    cargo fmt --check
    find modules tests hosts nix -name '*.nix' -print0 | xargs -0 -r nix run ".#formatter.{{system}}" -- --check
    nix run ".#formatter.{{system}}" -- --check flake.nix

# Apply formatting (writes). Not part of `just check`.
fmt-write:
    cargo fmt
    find modules tests hosts nix -name '*.nix' -print0 | xargs -0 -r nix run ".#formatter.{{system}}" --
    nix run ".#formatter.{{system}}" -- flake.nix

# Rust unit/integration tests only (fast host loop). Hermetic crate tests live
# in flake checks.<system>.ci; prefer `just ci` when Nix is the quality bar.
test:
    cargo test

# Clippy with warnings denied (CI-style).
clippy:
    cargo clippy --all-targets -- -D warnings

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

# Apex/www from github:SurmountSystems/site, plus extra vhosts. Docs: docs/OPS.md, docs/EDGE_AND_TLS.md
# `just publish` publishes static sites (not a NixOS generation).
[positional-arguments]
publish *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    nice -n 19 nix flake update surmount-site
    root="$(nice -n 19 nix build --print-build-logs --no-link --print-out-paths ".#surmount-public-site")"
    export SURMOUNT_PUBLIC_SITE_ROOT="${root}"
    exec nix run ".#surmount-deploy-static-sites" -- "$@"

# Default target is root@surmount-1. Extra arguments append after the host-local flag.
# This is the operator's real switch. Agents must not run `just deploy`. Agents run `just deploy-dry-run`.
[positional-arguments]
deploy *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-deploy-host" -- --target root@surmount-1 --host-local /home/hunter/.local/share/surmount/host-local "$@"

# Same target as `just deploy`, with `--dry-run` before `--target`. Extra arguments append.
# `--dry-run` rsyncs, bootstraps gccarch-x86-64-v3, and runs `nixos-rebuild build`.
# It does not switch the running generation and does not install secrets.
# Agents may run `just deploy-dry-run`. `--install-secrets` on a dry-run only prints the plan.
[positional-arguments]
deploy-dry-run *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-deploy-host" -- --dry-run --target root@surmount-1 --host-local /home/hunter/.local/share/surmount/host-local "$@"

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

# Interactive btop on the mail host via Eternal Terminal (not raw SSH).
# Laptop: exec the Eternal Terminal client at $HOME/.nix-profile/bin/et.
# That is the client command -v et prints. Do not nix run or copy a binary.
# Host btop is the one already installed on the mail host. -e (--noexit) leaves a shell.
# Nested guest: local btop, no et. Needs a live tty.
[positional-arguments]
btop *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    if [[ -f /etc/surmount/root-justfile ]]; then
      exec btop "$@"
    fi
    exec "$HOME/.nix-profile/bin/et" -k 5 -p 2022 --ssh-option "IdentityFile=${SURMOUNT_DEPLOY_SSH_IDENTITY:-$HOME/.ssh/id_ed25519}" -e -c btop root@surmount-1 "$@"

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

# Attach grok-oss on the mail host via SSH + tmux as user grok (not Eternal Terminal).
# Nested guest: the crate attaches local tmux session grok-oss as user grok.
[positional-arguments]
grok-oss *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-grok-oss" -- "$@"

# Print grok-oss running --json from the mail host as user grok.
# SSH, no TTY, no HTTP bind. Nested guest prints local JSON.
[positional-arguments]
grok-oss-running *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-grok-oss" -- --running "$@"

[positional-arguments]
host-logs *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-host-logs" -- "$@"

# Read-only guest diagnose: units + nixbuilder user-slice MemoryMax.
# Target: --target, else SURMOUNT_DEPLOY_TARGET, else agent-target.env.
# Does not print addresses. Does not mutate the guest.
host-status *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    target="${SURMOUNT_DEPLOY_TARGET:-}"
    envf="${HOME}/.local/share/surmount/agent-target.env"
    if [[ -z "${target}" && -r "${envf}" ]]; then
      # shellcheck disable=SC1090
      set -a
      # shellcheck disable=SC1091
      source "${envf}"
      set +a
      target="${SURMOUNT_DEPLOY_TARGET:-}"
    fi
    if [[ "${1:-}" == "--target" ]]; then
      target="${2:-}"
    fi
    if [[ -z "${target}" ]]; then
      echo "host-status: set SURMOUNT_DEPLOY_TARGET or --target USER@HOST" >&2
      exit 2
    fi
    ssh -o BatchMode=yes -o ConnectTimeout=8 -- "${target}" \
      'u=$(id -u nixbuilder 2>/dev/null || echo ""); echo gen=$(readlink /run/current-system | sed "s|.*/||"); echo qemu=$(systemctl is-active qemu-guest-agent 2>/dev/null || echo n/a); echo et=$(systemctl is-active eternal-terminal 2>/dev/null || echo n/a); for s in sshd stalwart-mail surmount-management-ui; do printf "%s=%s " "$s" "$(systemctl is-active $s)"; done; echo; if [[ -n "$u" ]]; then echo "nixbuilder_uid=$u"; systemctl show "user-${u}.slice" -p MemoryMax,MemoryCurrent,Description --no-pager; fi; echo lake=$(pgrep -c -x lake 2>/dev/null || echo 0)'

# Laptop: SSH surmount-scram --now to the guest.
# Default target: SURMOUNT_DEPLOY_TARGET, else agent-target.env, else root@surmount-1.
# Guest /root/justfile `just scram` is local (no SSH). Does not start Lake.
scram *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    target="${SURMOUNT_DEPLOY_TARGET:-}"
    envf="${HOME}/.local/share/surmount/agent-target.env"
    if [[ -z "${target}" && -r "${envf}" ]]; then
      set -a
      # shellcheck disable=SC1091
      source "${envf}"
      set +a
      target="${SURMOUNT_DEPLOY_TARGET:-}"
    fi
    if [[ "${1:-}" == "--target" ]]; then
      target="${2:-}"
      shift 2 || true
    fi
    if [[ -z "${target}" ]]; then
      target="root@surmount-1"
    fi
    echo "scram: ${target} surmount-scram --now" >&2
    ssh -o BatchMode=yes -o ConnectTimeout=8 -- "${target}" -- surmount-scram --now



# Targeted NixOS eval contracts (not module-eval-contract).
test-logging-eval:
    nix eval --impure --json --file tests/logging-contract.nix

test-sshd-hostkeys-eval:
    nix eval --impure --json --file tests/sshd-hostkeys-contract.nix

test-eternal-terminal-eval:
    nix eval --impure --json --file tests/eternal-terminal-contract.nix

test-inxi-eval:
    nix eval --impure --json --file tests/inxi-contract.nix

test-btop-eval:
    nix eval --impure --json --file tests/btop-contract.nix

test-perf-tools-eval:
    nix eval --impure --json --file tests/perf-tools-contract.nix

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
rekey *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${1:-}" == "--" ]]; then
      shift
    fi
    exec nix run ".#surmount-rekey" -- "$@"

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

# Write locked flake inputs and fixed-output sources as ordinary directories under ./vendor.
# Not a mail-host switch. Do not run this recipe as a substitute for deploy.
vendor:
    #!/usr/bin/env bash
    set -euo pipefail

    # nix help-stores: Store URL format is local or an absolute root.
    # A root other than / is a local chroot store. The help example
    # `nix run --store /tmp/root` stores files at /tmp/root/nix/store.
    # nix copy --help: an absolute --to path is that chroot store.
    # This recipe uses an absolute path under ./vendor/nix-store so the
    # bytes sit on the cwd disk. On surmount-1 the checkout is
    # /mnt/hdd/projects/server, so vendor/ is on /mnt/hdd.
    # Do not build the mail host. Do not run just deploy or just deploy-host.

    if [[ "${VENDOR_NICED:-}" != "1" ]]; then
      export VENDOR_NICED=1
      if [[ -f "${0:-}" ]]; then
        if ionice -c 3 -- /bin/true >/dev/null 2>&1; then
          exec ionice -c 3 nice -n 19 bash "$0" "$@"
        fi
        exec nice -n 19 bash "$0" "$@"
      fi
      renice -n 19 "$$" >/dev/null 2>&1 || true
      ionice -c 3 -p "$$" >/dev/null 2>&1 || true
    fi

    ROOT="$(pwd)"
    if [[ ! -f "$ROOT/flake.nix" || ! -f "$ROOT/flake.lock" ]]; then
      echo "just vendor: run from the repo root (flake.nix and flake.lock)." >&2
      exit 2
    fi
    cd "$ROOT"

    if ! command -v nix >/dev/null 2>&1; then
      echo "just vendor: nix is not on PATH." >&2
      exit 2
    fi
    if ! command -v jq >/dev/null 2>&1; then
      echo "just vendor: jq is not on PATH." >&2
      exit 2
    fi
    if ! command -v tar >/dev/null 2>&1; then
      echo "just vendor: tar is not on PATH." >&2
      exit 2
    fi

    VENDOR="$ROOT/vendor"
    STORE="$VENDOR/nix-store"
    INPUTS="$VENDOR/inputs"
    SOURCES="$VENDOR/sources"
    WORK="$VENDOR/tmp"
    case "$STORE" in
      "$VENDOR"/*) ;;
      *)
        echo "just vendor: refusing store path outside ./vendor." >&2
        exit 2
        ;;
    esac
    mkdir -p "$INPUTS" "$SOURCES" "$WORK" "$STORE"
    export TMPDIR="$WORK"
    export TEMP="$WORK"
    export TMP="$WORK"

    cores="$(nproc)"
    half=$(( cores / 2 ))
    if (( half < 1 )); then
      half=1
    fi

    # Empty builders: do not send this fetch to a remote machine.
    nixcmd=(
      nix
      --store "$STORE"
      --option builders ""
      --option max-jobs "$half"
      --option warn-dirty false
      --option allow-import-from-derivation false
    )

    redact_url() {
      local url="$1" out="" part scheme
      local IFS=' '
      for part in $url; do
        scheme="${part%%://*}"
        if [[ "$part" == *"://"* && "$scheme" == "file" ]]; then
          part="(local file url omitted)"
        elif [[ "$part" =~ (token|password|secret)= ]]; then
          part="(url omitted)"
        fi
        out+="${out:+ }$part"
      done
      printf '%s\n' "$out"
    }

    physical_of() {
      local logical="$1"
      case "$logical" in
        "$STORE"/nix/store/*)
          printf '%s\n' "$logical"
          ;;
        /nix/store/*)
          printf '%s\n' "$STORE$logical"
          ;;
        *)
          return 1
          ;;
      esac
    }

    export_tree() {
      local src="$1" dest="$2"
      case "$dest" in
        "$INPUTS"/*|"$SOURCES"/*) ;;
        *)
          echo "just vendor: refusing export outside vendor/inputs or vendor/sources." >&2
          return 1
          ;;
      esac
      if [[ -L "$src" ]]; then
        src="$(readlink -f -- "$src")"
      fi
      if [[ ! -d "$src" ]]; then
        echo "just vendor: missing source directory $src" >&2
        return 1
      fi
      mkdir -p "$(dirname -- "$dest")"
      chmod u+w "$(dirname -- "$dest")" 2>/dev/null || true
      rm -rf -- "$dest"
      if ! cp -a --link "$src" "$dest"; then
        rm -rf -- "$dest"
        cp -a "$src" "$dest"
      fi
      chmod u+w "$dest" 2>/dev/null || true
      if [[ -L "$dest" || ! -d "$dest" ]]; then
        echo "just vendor: $dest is not a real directory." >&2
        return 1
      fi
    }

    unpack_archive() {
      local src="$1" dest="$2" base
      base="$(basename -- "$src")"
      case "$base" in
        *.nar|*.narinfo)
          cp -a "$src" "$dest/"
          return 0
          ;;
        *.zip)
          if command -v unzip >/dev/null 2>&1; then
            unzip -q -o "$src" -d "$dest"
          elif command -v bsdtar >/dev/null 2>&1; then
            bsdtar -xf "$src" -C "$dest"
          else
            cp -a "$src" "$dest/"
            echo "just vendor: left $base packed (no unzip on PATH)." >&2
          fi
          ;;
        *.tar|*.tar.gz|*.tgz|*.tar.bz2|*.tbz2|*.tar.xz|*.txz|*.tar.zst)
          if ! tar -xf "$src" -C "$dest"; then
            cp -a "$src" "$dest/"
            echo "just vendor: could not unpack $base; left the archive file." >&2
          fi
          ;;
        *)
          cp -a "$src" "$dest/"
          ;;
      esac
    }

    export_output() {
      local logical="$1" dest="$2" phys="" target="" base=""
      case "$dest" in
        "$SOURCES"/*) ;;
        *)
          echo "just vendor: refusing source export outside vendor/sources." >&2
          return 1
          ;;
      esac
      if phys="$(physical_of "$logical")" && [[ -e "$phys" ]]; then
        :
      elif [[ -e "$logical" ]]; then
        phys="$logical"
      else
        return 1
      fi
      if [[ -L "$phys" ]]; then
        target="$(readlink -f -- "$phys")"
        phys="$target"
      fi
      rm -rf -- "$dest"
      mkdir -p "$dest"
      if [[ -d "$phys" ]]; then
        if ! cp -a --link "$phys"/. "$dest"/; then
          rm -rf -- "$dest"
          mkdir -p "$dest"
          cp -a "$phys"/. "$dest"/
        fi
      elif [[ -f "$phys" ]]; then
        base="$(basename -- "$phys")"
        unpack_archive "$phys" "$dest"
      else
        echo "just vendor: output is not a file or directory: $phys" >&2
        return 1
      fi
      if [[ -L "$dest" || ! -d "$dest" ]]; then
        echo "just vendor: $dest is not a real directory." >&2
        return 1
      fi
    }

    stamp="$(
      find flake.nix flake.lock nix modules hosts -type f -print0 \
        | sort -z \
        | xargs -0 sha256sum \
        | sha256sum \
        | awk '{print $1}'
    )"

    # A missing manifest is an empty object. Do not require a stub file.
    # When that object has no narHash, leave real input directories in place.
    prev="$VENDOR/manifest.json"
    empty_manifest='{"inputs":[],"sources":[],"fodEvalErrors":[],"fodSkipped":[]}'
    if [[ -L "$prev" || ! -f "$prev" ]] || ! jq -e . "$prev" >/dev/null 2>&1; then
      if [[ -e "$prev" && ! -L "$prev" ]]; then
        echo "just vendor: existing manifest.json is not JSON. Source trees are left in place." >&2
      fi
      printf '%s\n' "$empty_manifest" > "$WORK/empty-manifest.json"
      prev="$WORK/empty-manifest.json"
    fi
    stub_no_hash=0
    if ! jq -e '[.inputs[]? | .narHash // empty | select(. != "")] | length > 0' "$prev" >/dev/null 2>&1; then
      stub_no_hash=1
    fi

    jq -c '
      def resolve_node($nodes; $path):
        reduce $path[] as $n (
          "root";
          . as $cur
          | ($nodes[$cur].inputs[$n] // error("missing \($n) under \($cur)")) as $next
          | if ($next|type) == "string" then $next
            elif ($next|type) == "array" then resolve_node($nodes; $next)
            else error("bad lock input")
            end
        );
      def walk($nodes; $node; $prefix):
        ($nodes[$node].inputs // {}) | to_entries[]
        | .key as $name
        | .value as $raw
        | (if ($raw|type) == "string" then $raw
           elif ($raw|type) == "array" then resolve_node($nodes; $raw)
           else error("bad lock input")
           end) as $target
        | ($prefix + [$name]) as $p
        | ($nodes[$target]) as $t
        | {
            rel: ($p | join("/")),
            node: $target,
            lockedType: ($t.locked.type // ""),
            rev: ($t.locked.rev // ""),
            narHash: ($t.locked.narHash // ""),
            originalUrl: (
              if ($t.original.type // $t.locked.type) == "github" then
                "github:"
                + ($t.original.owner // $t.locked.owner // "")
                + "/"
                + ($t.original.repo // $t.locked.repo // "")
                + (if ($t.original.ref // "") != "" then "/" + $t.original.ref
                   elif ($t.original.rev // "") != "" then "/" + $t.original.rev
                   else "" end)
              elif ($t.original.url // $t.locked.url // "") != "" then
                ($t.original.url // $t.locked.url)
              else
                ($t.original.type // $t.locked.type // "")
              end
            )
          },
          walk($nodes; $target; $p);
      .nodes as $nodes | walk($nodes; "root"; [])
    ' flake.lock | jq -s 'sort_by(.rel | split("/") | length)' > "$WORK/lock-inputs.json"

    declare -A NEED=()
    any_need=0
    while IFS= read -r rel; do
      nar="$(jq -r --arg rel "$rel" '.[] | select(.rel==$rel) | .narHash' "$WORK/lock-inputs.json")"
      dest="$INPUTS/$rel"
      have="$(jq -r --arg rel "$rel" '[.inputs[]? | select(.name==$rel) | .narHash // empty][0] // ""' "$prev" || true)"
      if [[ -d "$dest" && ! -L "$dest" && "$stub_no_hash" == "1" ]]; then
        NEED["$rel"]=0
      elif [[ -d "$dest" && ! -L "$dest" && "$have" == "$nar" && -n "$nar" ]]; then
        NEED["$rel"]=0
      else
        NEED["$rel"]=1
        any_need=1
      fi
    done < <(jq -r '.[].rel' "$WORK/lock-inputs.json")

    # Rebuilding a parent directory removes nested input directories.
    while IFS= read -r rel; do
      if [[ "${NEED[$rel]}" == "1" ]]; then
        while IFS= read -r child; do
          case "$child" in
            "$rel"/*) NEED["$child"]=1 ;;
          esac
        done < <(jq -r '.[].rel' "$WORK/lock-inputs.json")
      fi
    done < <(jq -r '.[].rel' "$WORK/lock-inputs.json")
    for rel in "${!NEED[@]}"; do
      if [[ "${NEED[$rel]}" == "1" ]]; then
        any_need=1
      fi
    done

    echo '{}' > "$WORK/archive-by-rel.json"
    echo '[]' > "$WORK/path-info.json"
    if [[ "$any_need" == "1" ]]; then
      echo "just vendor: fetching locked flake inputs into $STORE" >&2
      if ! "${nixcmd[@]}" flake archive \
        --json \
        --no-write-lock-file \
        --no-update-lock-file \
        --print-build-logs \
        . > "$WORK/flake-archive.json" 2>"$WORK/flake-archive.err"; then
        echo "just vendor: flake archive failed." >&2
        tail -n 40 "$WORK/flake-archive.err" >&2 || true
        exit 1
      fi
      if jq -e . "$WORK/flake-archive.json" >/dev/null 2>&1; then
        jq '
          def walk($prefix):
            (.inputs // {}) | to_entries[]
            | .key as $k
            | .value as $v
            | ($prefix + [$k]) as $p
            | { key: ($p | join("/")), value: ($v.path // "") },
              ($v | walk($p));
          [ walk([]) ] | from_entries
        ' "$WORK/flake-archive.json" > "$WORK/archive-by-rel.json"
      else
        echo "just vendor: flake archive did not print JSON. Matching store objects by narHash." >&2
      fi
      if ! "${nixcmd[@]}" path-info --json --all > "$WORK/path-info.json" 2>"$WORK/path-info.err"; then
        echo '[]' > "$WORK/path-info.json"
      fi
    fi

    : > "$WORK/inputs.ndjson"
    input_fail=0
    while IFS= read -r row; do
      rel="$(jq -r '.rel' <<<"$row")"
      nar="$(jq -r '.narHash' <<<"$row")"
      locked_type="$(jq -r '.lockedType' <<<"$row")"
      rev="$(jq -r '.rev' <<<"$row")"
      original="$(jq -r '.originalUrl' <<<"$row")"
      original="$(redact_url "$original")"
      dest="$INPUTS/$rel"
      if [[ "${NEED[$rel]}" == "1" ]]; then
        logical="$(jq -r --arg rel "$rel" '.[$rel] // empty' "$WORK/archive-by-rel.json")"
        if [[ -z "$logical" || "$logical" == "null" ]]; then
          logical="$(jq -r --arg nar "$nar" '
            def rows:
              if type == "array" then .[]
              elif type == "object" then
                to_entries[]
                | { path: .key }
                  + (if (.value | type) == "object" then .value else {} end)
              else empty end;
            [ rows | select(.narHash == $nar) | .path ] | map(select(. != null and . != "")) | first // empty
          ' "$WORK/path-info.json")"
        fi
        phys=""
        if [[ -n "$logical" ]]; then
          phys="$(physical_of "$logical" || true)"
          if [[ -n "$phys" && ! -d "$phys" && -d "$logical" ]]; then
            phys="$logical"
          fi
        fi
        if [[ -z "$phys" || ! -d "$phys" ]]; then
          echo "just vendor: no store directory for input $rel" >&2
          input_fail=1
          continue
        fi
        if ! export_tree "$phys" "$dest"; then
          input_fail=1
          continue
        fi
      fi
      if [[ ! -d "$dest" || -L "$dest" ]]; then
        echo "just vendor: input $rel is not a real directory." >&2
        input_fail=1
        continue
      fi
      jq -nc \
        --arg name "$rel" \
        --arg lockedType "$locked_type" \
        --arg rev "$rev" \
        --arg narHash "$nar" \
        --arg originalUrl "$original" \
        --arg vendorPath "vendor/inputs/$rel" \
        '{name:$name,lockedType:$lockedType,rev:$rev,narHash:$narHash,originalUrl:$originalUrl,vendorPath:$vendorPath}' \
        >> "$WORK/inputs.ndjson"
    done < <(jq -c '.[]' "$WORK/lock-inputs.json")

    if [[ "$input_fail" != "0" ]]; then
      echo "just vendor: flake input export failed. Fixed-output export was not started." >&2
      exit 1
    fi

    input_count="$(jq -s 'length' "$WORK/inputs.ndjson")"
    echo "just vendor: $input_count flake inputs are real directories under vendor/inputs." >&2

    # Second run: do not delete matching trees, and do not re-eval when the
    # previous fixed-output record had no errors and the lock stamp matches.
    if [[ -n "$prev" && "$any_need" == "0" ]]; then
      prev_stamp="$(jq -r '.stamp // ""' "$prev")"
      prev_err="$(jq -r '(.fodEvalErrors // []) | length' "$prev")"
      prev_skip="$(jq -r '(.fodSkipped // []) | length' "$prev")"
      sources_ok=1
      while IFS= read -r spath; do
        [[ -z "$spath" ]] && continue
        if [[ ! -d "$ROOT/$spath" || -L "$ROOT/$spath" ]]; then
          sources_ok=0
        fi
      done < <(jq -r '.sources[]?.vendorPath' "$prev")
      if [[ "$prev_stamp" == "$stamp" && "$prev_err" == "0" && "$prev_skip" == "0" && "$sources_ok" == "1" ]]; then
        jq --arg stamp "$stamp" --slurpfile inputs "$WORK/inputs.ndjson" \
          '.inputs = $inputs | .stamp = $stamp' "$prev" \
          > "$VENDOR/manifest.json.new"
        mv -f "$VENDOR/manifest.json.new" "$VENDOR/manifest.json"
        echo "just vendor: inputs match the lock. Left existing directories in place." >&2
        exit 0
      fi
    fi

    : > "$WORK/sources.ndjson"
    : > "$WORK/fod-errors.ndjson"
    : > "$WORK/fod-skipped.ndjson"
    : > "$WORK/drv-index.ndjson"

    record_error() {
      local label="$1" errfile="$2"
      local message="${3:-evaluation failed; this output was not built}"
      # The name goes in the manifest. Nix stderr stays in vendor/tmp only.
      : "$errfile"
      jq -nc --arg output "$label" \
        --arg message "$message" \
        '{output:$output,message:$message}' >> "$WORK/fod-errors.ndjson"
      echo "just vendor: eval failed for $label (recorded, not built). Detail: $errfile" >&2
    }

    # Nix 2.33+ prints {"version": <number>, "derivations": {<basename>: <drv>}}.
    # Older Nix prints a map of derivation path to derivation object.
    # Never read .outputs from the version number. Basename store paths
    # become /nix/store/<basename>. A parse failure is recorded and the recipe continues.
    extract_drvs() {
      local json="$1"
      local label="$2"
      local parsed="$WORK/drv-parsed.ndjson"
      rm -f "$parsed"
      if ! jq -c '
        def as_store:
          if type != "string" or . == "" then empty
          elif startswith("/nix/store/") then .
          elif startswith("/") then .
          else "/nix/store/" + .
          end;
        def drv_entries:
          if (type == "object")
            and ((.version | type) == "number")
            and ((.derivations | type) == "object")
          then .derivations | to_entries[]
          elif type == "object" then to_entries[]
          else error("derivation show is not an object")
          end;
        drv_entries
        | select(.value | type == "object")
        | select((.value.outputs | type) == "object")
        | .key as $raw
        | .value as $d
        | ($d.outputs) as $outs
        | {
            drv: ($raw | as_store),
            isFod: (
              (([ $outs | to_entries[]
                  | select(.value | type == "object")
                  | (.value.hash // "")
                  | select(type == "string" and . != "")
                ] | length) > 0)
              or (
                (($d.env.outputHash? // "") | type) == "string"
                and (($d.env.outputHash? // "") != "")
              )
            ),
            outPaths: [
              $outs | to_entries[]
              | select(.value | type == "object")
              | (.value.path // "")
              | select(type == "string" and . != "")
              | as_store
            ],
            inputDrvs: (
              if ($d.inputDrvs | type) == "object" then
                [ $d.inputDrvs | keys[] | as_store ]
              elif ($d.inputDrvs | type) == "array" then
                [ $d.inputDrvs[] | strings | as_store ]
              elif (($d.inputs? | type) == "object")
                and (($d.inputs.drvs | type) == "object") then
                [ $d.inputs.drvs | keys[] | as_store ]
              else [] end
            ),
            name: ($d.env.name? // $d.name? // "unknown"),
            builder: ($d.builder? // $d.env.builder? // ""),
            urls: ($d.env.urls? // $d.env.url? // ""),
            outputHash: (
              [ $outs | to_entries[]
                | select(.value | type == "object")
                | (.value.hash // "")
                | select(type == "string" and . != "")
              ][0]
              // ($d.env.outputHash? // "")
            ),
            outPath: (
              ($d.env.out? // "") as $envout
              | if ($envout | type) == "string" and ($envout | startswith("/nix/store/")) then
                  $envout
                else
                  (($outs.out? // {}) | if type == "object" then (.path // "") else "" end)
                  | if type == "string" and . != "" then as_store else "" end
                end
            )
          }
        | select(.drv != "")
      ' "$json" > "$parsed"; then
        echo "derivation show JSON could not be parsed" > "$WORK/show.err"
        record_error "$label" "$WORK/show.err" \
          "derivation show JSON could not be parsed; this output was not built"
        rm -f "$json" "$parsed"
        return 0
      fi
      if [[ -s "$parsed" ]]; then
        cat "$parsed" >> "$WORK/drv-index.ndjson"
      fi
      rm -f "$json" "$parsed"
    }

    show_installables() {
      local label="$1"
      shift
      local err="$WORK/show.err"
      local json="$WORK/show.json"
      if [[ "$#" -eq 0 ]]; then
        return 0
      fi
      if ! "${nixcmd[@]}" derivation show \
        --recursive \
        --no-write-lock-file \
        --no-update-lock-file \
        "$@" >"$json" 2>"$err"; then
        record_error "$label" "$err"
        rm -f "$json"
        return 0
      fi
      if ! jq -e . "$json" >/dev/null 2>&1; then
        echo "derivation show did not print JSON" > "$err"
        record_error "$label" "$err"
        rm -f "$json"
        return 0
      fi
      if ! extract_drvs "$json" "$label"; then
        return 0
      fi
    }

    # Fixed-output half. Not a complete source closure when fodEvalErrors
    # or fodSkipped is non-empty. Import-from-derivation is off. This does
    # not build a non-fixed-output derivation and does not switch the host.
    sys=""
    if ! sys="$(nix eval --impure --raw --expr 'builtins.currentSystem' 2>"$WORK/system.err")"; then
      record_error "builtins.currentSystem" "$WORK/system.err"
      sys=""
    fi
    mail_label=".#nixosConfigurations.mail-vps.config.system.build.toplevel"
    show_installables "$mail_label" "$mail_label"

    # Same chroot store as the input archive. A bare nix eval would
    # fetch missing inputs into the root-disk store.
    if [[ -z "$sys" ]]; then
      echo "just vendor: package outputs were not evaluated because currentSystem failed." >&2
    elif ! "${nixcmd[@]}" eval \
      --json \
      ".#packages.${sys}" \
      --apply builtins.attrNames > "$WORK/pkg-names.json" 2>"$WORK/pkg-names.err"; then
      record_error ".#packages.${sys}" "$WORK/pkg-names.err"
    else
      mapfile -t pkg_names < <(jq -r '.[]' "$WORK/pkg-names.json")
      pkg_args=()
      for name in "${pkg_names[@]}"; do
        pkg_args+=(".#packages.${sys}.${name}")
      done
      if ! "${nixcmd[@]}" derivation show \
        --recursive \
        --no-write-lock-file \
        --no-update-lock-file \
        "${pkg_args[@]}" > "$WORK/show.json" 2>"$WORK/show.err"; then
        echo "just vendor: package set eval failed as one group. Trying each package." >&2
        rm -f "$WORK/show.json"
        for name in "${pkg_names[@]}"; do
          show_installables ".#packages.${sys}.${name}" ".#packages.${sys}.${name}"
        done
      else
        if jq -e . "$WORK/show.json" >/dev/null 2>&1 && extract_drvs "$WORK/show.json" ".#packages.${sys}"; then
          :
        else
          echo "package derivation show did not print JSON" > "$WORK/show.err"
          record_error ".#packages.${sys}" "$WORK/show.err"
          rm -f "$WORK/show.json"
        fi
      fi
    fi

    if [[ -s "$WORK/drv-index.ndjson" ]]; then
      if ! jq -s 'unique_by(.drv)' "$WORK/drv-index.ndjson" > "$WORK/drv-index.json"; then
        echo "derivation index could not be parsed" > "$WORK/show.err"
        record_error "derivation-index" "$WORK/show.err" \
          "derivation index could not be parsed; fixed-output outputs were not selected"
        echo '[]' > "$WORK/drv-index.json"
        echo '[]' > "$WORK/fods.json"
      elif ! jq -c '
        (map({key: .drv, value: .}) | from_entries) as $m
        | def go($id; $seen):
            if $seen[$id] then empty
            else
              ($m[$id] // null) as $dep
              | if $dep == null then "__MISSING_DEP__"
                elif $dep.isFod then
                    ($dep.inputDrvs // [])[] | go(.; $seen + {($id): true})
                else ($dep.outPaths // [])[]
                end
            end;
        [ .[] | select(.isFod) | {
          drv,
          name,
          builder,
          urls,
          outputHash,
          outPath,
          need: ([ (.inputDrvs // [])[] | go(.; {}) ] | unique)
        } ]
      ' "$WORK/drv-index.json" > "$WORK/fods.json"; then
        echo "fixed-output selection could not be parsed" > "$WORK/show.err"
        record_error "fixed-output-selection" "$WORK/show.err" \
          "fixed-output selection could not be parsed; fixed-output outputs were not selected"
        echo '[]' > "$WORK/fods.json"
      fi
    else
      echo '[]' > "$WORK/drv-index.json"
      echo '[]' > "$WORK/fods.json"
    fi

    declare -A SEEN_HASH=()
    copy_list="$WORK/to-copy.txt"
    : > "$copy_list"
    build_list="$WORK/to-build.txt"
    : > "$build_list"
    : > "$WORK/pending-sources.ndjson"
    declare -A ALLOWED_DRV=()

    while IFS= read -r fod; do
      drv="$(jq -r '.drv' <<<"$fod")"
      ALLOWED_DRV["$drv"]=1
    done < <(jq -c '.[] | select(.isFod)' "$WORK/drv-index.json" 2>/dev/null || true)

    while IFS= read -r fod; do
      drv="$(jq -r '.drv' <<<"$fod")"
      name="$(jq -r '.name' <<<"$fod")"
      builder="$(jq -r '.builder' <<<"$fod")"
      urls="$(jq -r '.urls' <<<"$fod")"
      urls="$(redact_url "$urls")"
      output_hash="$(jq -r '.outputHash // ""' <<<"$fod")"
      out_path="$(jq -r '.outPath // ""' <<<"$fod")"
      if [[ -z "$output_hash" || "$output_hash" == "null" ]]; then
        jq -nc --arg name "$name" --arg drv "$drv" --arg reason "no output hash" \
          '{name:$name,drv:$drv,reason:$reason}' >> "$WORK/fod-skipped.ndjson"
        continue
      fi
      safe="$(printf '%s' "${output_hash}-${name}" | tr -c 'A-Za-z0-9._+-' '_')"
      dest_rel="vendor/sources/$safe"
      dest="$ROOT/$dest_rel"
      if [[ -d "$dest" && ! -L "$dest" ]]; then
        SEEN_HASH["$output_hash"]=1
        jq -nc \
          --arg name "$name" \
          --arg lockedType "fixed-output" \
          --arg rev "" \
          --arg narHash "$output_hash" \
          --arg originalUrl "$urls" \
          --arg vendorPath "$dest_rel" \
          '{name:$name,lockedType:$lockedType,rev:$rev,narHash:$narHash,originalUrl:$originalUrl,vendorPath:$vendorPath}' \
          >> "$WORK/sources.ndjson"
        continue
      fi
      missing=0
      case "$builder" in
        builtin:*)
          ;;
        *)
          while IFS= read -r need; do
            [[ -z "$need" ]] && continue
            if [[ "$need" == "__MISSING_DEP__" ]]; then
              missing=1
              continue
            fi
            if [[ -e "$STORE$need" ]]; then
              continue
            fi
            if [[ -e "$need" ]]; then
              printf '%s\n' "$need" >> "$copy_list"
            else
              missing=1
            fi
          done < <(jq -r '.need[]?' <<<"$fod")
          ;;
      esac
      if [[ "$missing" != "0" ]]; then
        jq -nc --arg name "$name" --arg drv "$drv" \
          --arg reason "non-fixed-output dependency is not already present, so it was not built" \
          '{name:$name,drv:$drv,reason:$reason}' >> "$WORK/fod-skipped.ndjson"
        continue
      fi
      printf '%s\n' "$drv^*" >> "$build_list"
      jq -nc \
        --arg name "$name" \
        --arg lockedType "fixed-output" \
        --arg rev "" \
        --arg narHash "$output_hash" \
        --arg originalUrl "$urls" \
        --arg vendorPath "$dest_rel" \
        --arg outPath "$out_path" \
        --arg builder "$builder" \
        --arg drv "$drv" \
        '{name:$name,lockedType:$lockedType,rev:$rev,narHash:$narHash,originalUrl:$originalUrl,vendorPath:$vendorPath,outPath:$outPath,builder:$builder,drv:$drv}' \
        >> "$WORK/pending-sources.ndjson"
    done < <(jq -c '.[]' "$WORK/fods.json")

    if [[ -s "$copy_list" ]]; then
      # Source is the default store, where those outputs already exist.
      # --to is the absolute chroot root under ./vendor, not a cache URL.
      if ! sort -u "$copy_list" | nix copy --option builders "" --to "$STORE" --no-check-sigs --stdin; then
        echo "just vendor: copying already-present dependencies into the cwd store failed." >&2
        : > "$WORK/build-kept.txt"
        : > "$WORK/pending-kept.ndjson"
        while IFS= read -r row; do
          [[ -z "$row" ]] && continue
          bldr="$(jq -r '.builder' <<<"$row")"
          case "$bldr" in
            builtin:*)
              printf '%s\n' "$(jq -r '.drv' <<<"$row")^*" >> "$WORK/build-kept.txt"
              printf '%s\n' "$row" >> "$WORK/pending-kept.ndjson"
              ;;
            *)
              jq -nc \
                --arg name "$(jq -r '.name' <<<"$row")" \
                --arg drv "$(jq -r '.drv' <<<"$row")" \
                --arg reason "could not copy an existing dependency into the cwd store" \
                '{name:$name,drv:$drv,reason:$reason}' >> "$WORK/fod-skipped.ndjson"
              ;;
          esac
        done < "$WORK/pending-sources.ndjson"
        mv -f "$WORK/build-kept.txt" "$build_list"
        mv -f "$WORK/pending-kept.ndjson" "$WORK/pending-sources.ndjson"
      fi
    fi

    realise_lines() {
      local list="$1"
      if [[ ! -s "$list" ]]; then
        return 0
      fi
      local dry="$WORK/dry.json"
      local err="$WORK/realise.err"
      if ! "${nixcmd[@]}" build --stdin --dry-run --json --no-link \
        < "$list" > "$dry" 2>"$err"; then
        if [[ "$(grep -c . "$list" || true)" -le 1 ]]; then
          record_error "fixed-output dry-run $(head -n 1 "$list")" "$err"
          return 0
        fi
        local half_line count left right
        count="$(grep -c . "$list" || true)"
        half_line=$(( count / 2 ))
        left="$(mktemp "$WORK/build-left.XXXXXX")"
        right="$(mktemp "$WORK/build-right.XXXXXX")"
        head -n "$half_line" "$list" > "$left"
        tail -n +"$(( half_line + 1 ))" "$list" > "$right"
        realise_lines "$left"
        realise_lines "$right"
        return 0
      fi
      local unexpected=""
      if jq -e . "$dry" >/dev/null 2>&1; then
        while IFS= read -r dpath; do
          [[ -z "$dpath" ]] && continue
          dpath="${dpath#"$STORE"}"
          dpath="${dpath%%^*}"
          if [[ -z "${ALLOWED_DRV[$dpath]:-}" ]]; then
            unexpected="$dpath"
            break
          fi
        done < <(jq -r '
          def rows:
            if type == "array" then .[]
            elif type == "object" then (to_entries[] | .value)
            else empty end;
          [ rows | .drvPath // .drv // empty ] | .[]
        ' "$dry")
      fi
      if [[ -n "$unexpected" ]]; then
        if [[ "$(grep -c . "$list" || true)" -le 1 ]]; then
          local one
          one="$(head -n 1 "$list")"
          jq -nc --arg name "$one" --arg drv "$one" \
            --arg reason "dry-run wanted a derivation that is not fixed-output ($unexpected); not built" \
            '{name:$name,drv:$drv,reason:$reason}' >> "$WORK/fod-skipped.ndjson"
          return 0
        fi
        local half_line count left right
        count="$(grep -c . "$list" || true)"
        half_line=$(( count / 2 ))
        left="$(mktemp "$WORK/build-left.XXXXXX")"
        right="$(mktemp "$WORK/build-right.XXXXXX")"
        head -n "$half_line" "$list" > "$left"
        tail -n +"$(( half_line + 1 ))" "$list" > "$right"
        realise_lines "$left"
        realise_lines "$right"
        return 0
      fi
      if ! "${nixcmd[@]}" build --stdin --no-link --print-build-logs \
        < "$list" >"$WORK/built-paths.txt" 2>"$err"; then
        if [[ "$(grep -c . "$list" || true)" -le 1 ]]; then
          record_error "fixed-output build $(head -n 1 "$list")" "$err"
          return 0
        fi
        local half_line count left right
        count="$(grep -c . "$list" || true)"
        half_line=$(( count / 2 ))
        left="$(mktemp "$WORK/build-left.XXXXXX")"
        right="$(mktemp "$WORK/build-right.XXXXXX")"
        head -n "$half_line" "$list" > "$left"
        tail -n +"$(( half_line + 1 ))" "$list" > "$right"
        realise_lines "$left"
        realise_lines "$right"
      fi
    }

    touch "$WORK/pending-sources.ndjson"
    if [[ -s "$build_list" ]]; then
      echo "just vendor: realising fixed-output outputs that do not need a new non-fixed-output build." >&2
      realise_lines "$build_list"
    fi

    if [[ -s "$WORK/pending-sources.ndjson" ]]; then
      while IFS= read -r row; do
        out_path="$(jq -r '.outPath' <<<"$row")"
        dest_rel="$(jq -r '.vendorPath' <<<"$row")"
        dest="$ROOT/$dest_rel"
        name="$(jq -r '.name' <<<"$row")"
        output_hash="$(jq -r '.narHash' <<<"$row")"
        if [[ -z "$out_path" ]]; then
          jq -nc --arg name "$name" --arg drv "" \
            --arg reason "no output path after realisation" \
            '{name:$name,drv:$drv,reason:$reason}' >> "$WORK/fod-skipped.ndjson"
          continue
        fi
        if ! export_output "$out_path" "$dest"; then
          jq -nc --arg name "$name" --arg drv "" \
            --arg reason "fixed-output output was not in the cwd store after realisation" \
            '{name:$name,drv:$drv,reason:$reason}' >> "$WORK/fod-skipped.ndjson"
          continue
        fi
        SEEN_HASH["$output_hash"]=1
        jq -c 'del(.outPath, .builder, .drv)' <<<"$row" >> "$WORK/sources.ndjson"
      done < "$WORK/pending-sources.ndjson"
    fi

    if [[ -n "$prev" ]]; then
      while IFS= read -r row; do
        [[ -z "$row" ]] && continue
        hash="$(jq -r '.narHash // ""' <<<"$row")"
        path="$(jq -r '.vendorPath // ""' <<<"$row")"
        [[ -z "$hash" || -z "$path" ]] && continue
        if [[ -n "${SEEN_HASH[$hash]:-}" ]]; then
          continue
        fi
        if [[ -d "$ROOT/$path" && ! -L "$ROOT/$path" ]]; then
          printf '%s\n' "$row" >> "$WORK/sources.ndjson"
          SEEN_HASH["$hash"]=1
        fi
      done < <(jq -c '.sources[]?' "$prev")
    fi

    fod_note="Fixed-output outputs exported without building a non-fixed-output derivation. If fodEvalErrors or fodSkipped is non-empty, this is not the full source set of packages and the mail host."
    jq -n \
      --arg note "Ordinary source directories on the cwd disk. vendor/nix-store is a local chroot store whose files are under that directory." \
      --arg stamp "$stamp" \
      --arg storeRoot "vendor/nix-store" \
      --arg fodNote "$fod_note" \
      --slurpfile inputs "$WORK/inputs.ndjson" \
      --slurpfile sources "$WORK/sources.ndjson" \
      --slurpfile fodEvalErrors "$WORK/fod-errors.ndjson" \
      --slurpfile fodSkipped "$WORK/fod-skipped.ndjson" \
      '{
        note: $note,
        storeRoot: $storeRoot,
        stamp: $stamp,
        inputs: $inputs,
        sources: ($sources | unique_by(.narHash)),
        fodEvalErrors: $fodEvalErrors,
        fodSkipped: $fodSkipped,
        fodNote: $fodNote
      }' > "$VENDOR/manifest.json.new"
    mv -f "$VENDOR/manifest.json.new" "$VENDOR/manifest.json"

    while IFS= read -r path; do
      [[ -z "$path" ]] && continue
      if [[ -L "$ROOT/$path" ]]; then
        echo "just vendor: $path is a symlink. Refusing." >&2
        exit 1
      fi
      if [[ ! -d "$ROOT/$path" ]]; then
        echo "just vendor: $path is not a directory." >&2
        exit 1
      fi
    done < <(jq -r '.inputs[].vendorPath, .sources[].vendorPath' "$VENDOR/manifest.json")

    src_count="$(jq '.sources | length' "$VENDOR/manifest.json")"
    err_count="$(jq '.fodEvalErrors | length' "$VENDOR/manifest.json")"
    skip_count="$(jq '.fodSkipped | length' "$VENDOR/manifest.json")"
    echo "just vendor: $input_count inputs, $src_count fixed-output source directories." >&2
    echo "just vendor: fod eval errors $err_count, fod skipped $skip_count." >&2
    if [[ "$err_count" != "0" || "$skip_count" != "0" ]]; then
      echo "just vendor: fixed-output export is not the full source set. See vendor/manifest.json fodEvalErrors and fodSkipped." >&2
    fi
