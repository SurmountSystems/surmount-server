# Research: PQConnect local Nix packaging

**Status:** research finding / packaging scaffold. **Not** operator acceptance
of Day-1 PQConnect deploy.
**Date:** 2026-07-30

Related: [pqconnect-and-pqc.md](pqconnect-and-pqc.md),
Surmount open **Q-PQC-*** in [open-choices.md](../open-choices.md).

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
apply. Version in tree: **1.2.3**.

---

## How Surmount should consume (until published)

Path flake input (local only):

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
  overlay now vs later)
- Day-1 vs Day-2 (**Q-PQC-1**), client audience (**Q-PQC-2**), mail path
  (**Q-PQC-3**)
- Capabilities / systemd units / DNS announce checklist for production
