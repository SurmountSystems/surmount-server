# Research: PQConnect local Nix packaging

**Status:** research finding / packaging scaffold. **Not** operator acceptance
of Day-1 PQConnect deploy.
**Date:** 2026-07-30 (integration-prep dual-pin 2026-08-08)

Related: [pqconnect-and-pqc.md](pqconnect-and-pqc.md),
Surmount open **Q-PQC-*** in [open-choices.md](../open-choices.md).
Living residual: [RESIDUAL.md](../../RESIDUAL.md), [COMPACTION-PIN.md](../COMPACTION-PIN.md).

**Do not mash with D1:** TLS hybrid KEX on Axum/rustls is ordinary HTTPS.
PQConnect is a **separate** path-layer track (D2).

---

## Where packaging lives

Packaging is **not** in surmount-server. It lives in the sibling operator
checkout:

| Item | Path |
|------|------|
| Repo | `/home/hunter/Projects/surmount/pqconnect` |
| Remote | `git@github.com:SurmountSystems/pqconnect.git` (mirror of upstream PQConnect) |
| Branch (work) | `feat/nix-flake` (local; may be uncommitted) |
| Flake | `flake.nix` + `flake.lock` |
| Docs | `README-NIX.md` |
| Package exprs | `nix/packages/*.nix` |

Upstream language is **Python** (flit), not Rust. Crane / rustPlatform do not
apply. Version in tree: **1.2.3**. Principles allow **isolated** upstream
Python for this service without a Surmount product Python control plane.

---

## How Surmount should consume (until published)

**Default (plan):** do **not** wire a surmount-server `flake.nix` path input
until the human has **committed** the sibling flake (agents never invent the
fork tip). Preferred order:

1. **Human** stages and sign-commits sibling `pqconnect` flake packaging.
2. Rebuild packages against living host channel (**nixos-26.05**). Green on
   25.05 alone is historical evidence, not readiness.
3. Optional local path input (operator workspace only), then pin
   `github:SurmountSystems/pqconnect` + rev when the operator wants a
   durable remote pin.
4. Optional NixOS module (server unit, caps, firewall, DNS announce).
5. Host-only keys and lab peer handshake. **Never** PQConnect keys in git.

Path flake input sketch (local only; **not wired** in product flake today):

```nix
# surmount-server/flake.nix inputs (sketch; not wired yet)
pqconnect = {
  url = "path:/home/hunter/Projects/surmount/pqconnect";
  # optional: inputs.nixpkgs.follows = "nixpkgs";
};
```

Then e.g. `inputs.pqconnect.packages.${system}.default` (bins:
`pqconnect`, `pqconnect-server`, `pqconnect-keygen`,
`pqconnect-server-check`).

Later: `github:SurmountSystems/pqconnect` once the flake is on a branch the
operator wants pinned.

**Prerequisite:** flake files in the pqconnect repo must be **git-tracked**
for `nix build` / path inputs to see them. Packaging left the tree dirty for
the human to stage and sign-commit.

**Operator-local path input exception:** only if the operator explicitly
orders a private workspace path input. Document as operator-local; do not
treat uncommitted sibling as a required product path; prefer gitignore over
committing machine-absolute paths. Plan default remains: wait for human
commit.

---

## Secrets and host material (absolute)

| Material | In public git? |
|----------|----------------|
| PQConnect server/client keys | **Never** |
| HS / TLS PEMs / age keys | **Never** (same hygiene law) |
| Real host identity in flake attrs | **Never** |

Deploy secrets stay host-only / operator channels. Scanner:
`nix run .#surmount-private-data`. Law: [hygiene.md](../hygiene.md),
[SECRETS.md](../SECRETS.md).

---

## Build evidence (2026-07-30; historical channel)

Impure `nix-build` of the package set against **nixos-25.05** (Surmount host
at that packaging day): **green**. Living Surmount host is **nixos-26.05**;
re-build on the living channel before claiming product readiness.

- Native: `libmceliece` (Debian orig 20260622), `libntruprime` (20260717)
- App: `pqconnect-1.2.3` with import checks and `--help` on main bins
- Vendored on that 25.05 build: `pysodium`, `securestring` (present on
  unstable only at the time)
- nixpkgs already had: `lib25519`, `libcpucycles`, `librandombytes`,
  `nftables` (+ python), `libnetfilter_queue`, common Python deps

Details and Surmount path-input instructions: sibling `README-NIX.md`.

---

## Still open (product)

- Wire flake input + NixOS module in surmount-server (**Q-PQC-4** packaging
  overlay now vs later). Default lean: after human commit.
- Day-1 vs Day-2 (**Q-PQC-1**), client audience (**Q-PQC-2**), mail path
  (**Q-PQC-3**). Default lean if unanswered: Day-2 after B1 + D1 host proof.
- Capabilities / systemd units / DNS announce checklist for production
- Lab peer handshake proof (host-gated; not CI)
