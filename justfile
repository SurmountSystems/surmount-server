# Surmount Server developer tasks.
# A recipe body is one `nix run .#<name> -- "$@"` line, or a dependency
# list with no shell. Most matching flake apps exec surmount-deploy-host
# with that recipe name (argv0). just deploy is the operator switch and
# its flake app execs deploy-rs for node mail-vps.
# Agents run just deploy-dry-run. They must not run just deploy.
# Guest (remote builder) down: BUILD_LOCAL=true just <recipe>
# sets NIX_CONFIG builders empty so this laptop compiles. Not a SKU.

set shell := ["bash", "-euo", "pipefail", "-c"]
set positional-arguments := true

# Honor BUILD_LOCAL for every recipe that calls nix (including nix run).
export NIX_CONFIG := if env_var_or_default("BUILD_LOCAL", "") == "" { env_var_or_default("NIX_CONFIG", "") } else { "builders =\nmax-jobs = auto\n" }

# ssh-ng keepalives. An operator-set NIX_SSHOPTS is left as-is.
export NIX_SSHOPTS := env_var_or_default("NIX_SSHOPTS", "-o ServerAliveInterval=30 -o ServerAliveCountMax=10 -o TCPKeepAlive=yes -o ConnectionAttempts=3")

default:
    nix run .#default -- "$@"

check: fmt clippy test

ci:
    nix run .#ci -- "$@"

check-remote:
    nix run .#check-remote -- "$@"

audit-remote:
    nix run .#audit-remote -- "$@"

remote-nix-build installable label:
    nix run .#remote-nix-build -- "$@"

audit:
    nix run .#audit -- "$@"

deny:
    nix run .#deny -- "$@"

check-ci: ci

fmt:
    nix run .#fmt -- "$@"

fmt-write:
    nix run .#fmt-write -- "$@"

test:
    nix run .#test -- "$@"

clippy:
    nix run .#clippy -- "$@"

dev:
    nix run .#dev -- "$@"

e2e:
    nix run .#e2e -- "$@"

e2e-host:
    nix run .#e2e-host -- "$@"

publish *args:
    nix run .#publish -- "$@"

# Operator switch. Agents must not run this. The flake app execs
# deploy-rs for node mail-vps. It is not surmount-deploy-host.
# Agents run deploy-dry-run.
deploy *args:
    nix run .#deploy -- "$@"

# Does not activate and does not SSH a rollback. Not deploy-rs. That eval is a horizon check of tests/dry-run-reveals-deploy.nix, and running it on surmount-1 blocked the closure build on one CPU.
deploy-dry-run *args:
    nix run .#deploy-dry-run -- "$@"

deploy-host *args:
    nix run .#deploy-host -- "$@"

btop *args:
    nix run .#btop -- "$@"

host-inxi *args:
    nix run .#host-inxi -- "$@"

et *args:
    nix run .#et -- "$@"

grok-oss *args:
    nix run .#grok-oss -- "$@"

grok-oss-running *args:
    nix run .#grok-oss-running -- "$@"

host-logs *args:
    nix run .#host-logs -- "$@"

host-status *args:
    nix run .#host-status -- "$@"

scram *args:
    nix run .#scram -- "$@"

test-logging-eval:
    nix run .#test-logging-eval -- "$@"

test-sshd-hostkeys-eval:
    nix run .#test-sshd-hostkeys-eval -- "$@"

test-eternal-terminal-eval:
    nix run .#test-eternal-terminal-eval -- "$@"

test-inxi-eval:
    nix run .#test-inxi-eval -- "$@"

test-btop-eval:
    nix run .#test-btop-eval -- "$@"

test-perf-tools-eval:
    nix run .#test-perf-tools-eval -- "$@"

test-remote-builder-eval:
    nix run .#test-remote-builder-eval -- "$@"

check-private-data *args:
    nix run .#check-private-data -- "$@"

alias private-data := check-private-data

check-leftover-agent-homes *args:
    nix run .#check-leftover-agent-homes -- "$@"

check-tls-hybrid:
    nix run .#check-tls-hybrid -- "$@"

secrets-prompt *args:
    nix run .#secrets-prompt -- "$@"

rekey *args:
    nix run .#rekey -- "$@"

diskstation-afp-mount *args:
    nix run .#diskstation-afp-mount -- "$@"

diskstation-discover *args:
    nix run .#diskstation-discover -- "$@"

sync-static-sites-from-ds3018xs *args:
    nix run .#sync-static-sites-from-ds3018xs -- "$@"

mailplus-copy-uid *args:
    nix run .#mailplus-copy-uid -- "$@"

add-stalwart-token *args:
    nix run .#add-stalwart-token -- "$@"

bootstrap-stalwart-token *args:
    nix run .#bootstrap-stalwart-token -- "$@"

stalwart-recovery-unlock *args:
    nix run .#stalwart-recovery-unlock -- "$@"

fix-public-dashboard *args:
    nix run .#fix-public-dashboard -- "$@"

secrets-install-host *args:
    nix run .#secrets-install-host -- "$@"

secrets-export-bw-to-staging *args:
    nix run .#secrets-export-bw-to-staging -- "$@"

host-cutover *args:
    nix run .#host-cutover -- "$@"

cutover-step step *args:
    nix run .#cutover-step -- "$@"

render-host-profile-acme *args:
    nix run .#render-host-profile-acme -- "$@"

domain-audit *args:
    nix run .#domain-audit -- "$@"

dns-zone-namecheap *args:
    nix run .#dns-zone-namecheap -- "$@"

rdns-shc *args:
    nix run .#rdns-shc -- "$@"

free-stalwart-public-443 *args:
    nix run .#free-stalwart-public-443 -- "$@"

register-dkim *args:
    nix run .#register-dkim -- "$@"

point-stalwart-mail-tls *args:
    nix run .#point-stalwart-mail-tls -- "$@"

laptop-renew-cert *args:
    nix run .#laptop-renew-cert -- "$@"

check-heavy:
    nix run .#check-heavy -- "$@"

vendor:
    nix run .#vendor -- "$@"

update-flakes:
    nix run .#update-flakes -- "$@"

update-software:
    nix run .#update-software -- "$@"
