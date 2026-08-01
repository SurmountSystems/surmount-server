# Join: version pin audit

**Date:** 2026-07-30
**Artifact:** `docs/research/version-audit.md`
**Bumps this pass:** none (Stalwart family already latest)

## Headline

- **Stalwart FODs are current:** server 0.16.15, cli 1.0.12, webui 1.0.7,
  spam-filter 3.0.0. Match GitHub `releases/latest` as of audit day.
- **Biggest gap:** flake nixpkgs is **nixos-25.05** (tip frozen ~2026-01-02).
  Current stable branch is **nixos-26.05**. Engine is independent (Surmount
  FODs); OS channel is the host/rustc/openssl story.
- **nixpkgs unstable** still packages stalwart_0_16 **0.16.14** and cli
  **1.0.11** — Surmount FODs are **ahead**.
- **management-ui:** lock matches crates.io within ranges except tokio
  1.53.0 vs 1.53.1 patch. reqwest 0.12 and tower-http 0.6 are intentional
  older majors vs 0.13.4 / 0.7.0.
- **RocksDB:** engine uses upstream lock rocksdb 0.24.0 /
  librocksdb-sys 0.17.3+**10.4.2**. No system rocksdb wired. nixpkgs 25.05
  rocksdb is 10.2.1; unstable 10.10.1. Facebook tip is 11.1.2.
- **crane** and **sops-nix** locks match master HEAD today.
- Process pin written in audit intro: **always re-validate latest** before
  claiming current or bumping.

## Packaging agent next (proposed)

1. No Stalwart FOD bump until a newer tag exists.
2. Plan OS channel move to nixos-26.05 when operator wants it.
3. Optional: `cargo update -p tokio`; optional reqwest/tower-http major review.
)
