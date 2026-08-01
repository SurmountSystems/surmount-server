# Surmount Server developer tasks.
# Quality bar SoT: flake checks.ci (same as CI). Heavy VM/FOD checks are separate.
# End-to-end SoT: nix run .#e2e (local) and nix run .#e2e-host (env-gated host).
# just e2e / e2e-host are thin wrappers. Host e2e is never in checks.ci.

set shell := ["bash", "-euo", "pipefail", "-c"]

system := `nix eval --impure --raw --expr 'builtins.currentSystem'`

# List recipes (default).
default:
    @just --list

# CI quality bar: rust fmt/clippy/test + module eval contracts + nixfmt.
# Same constituents as checks.<system>.ci (no mail-vm / stalwart FOD / full toplevel).
check:
    nix build --print-build-logs ".#checks.{{system}}.ci"

# Format Rust + Nix (writes).
fmt:
    cd crates && cargo fmt
    find modules tests hosts nix -name '*.nix' -print0 | xargs -0 nixfmt
    nixfmt flake.nix

# Rust unit/integration tests only (fast host loop).
test:
    cd crates && cargo test

# Clippy with warnings denied.
clippy:
    cd crates && cargo clippy --all-targets -- -D warnings

# Local comprehensive end-to-end (hermetic; optional Tor when present).
# SoT: nix run .#e2e (Rust binary packages.e2e).
e2e:
    nix run ".#e2e"

# Host end-to-end (requires SURMOUNT_E2E_HOST=1; exit 2 if unset).
# SoT: nix run .#e2e-host. Never a flake check.
e2e-host:
    nix run ".#e2e-host"

# Optional heavy checks (not in `just check`).
check-heavy:
    nix build --print-build-logs ".#checks.{{system}}.mail-vm-test"
