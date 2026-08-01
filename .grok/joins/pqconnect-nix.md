# Join: PQConnect Nix flake packaging

**Date:** 2026-07-30
**Worker:** pqconnect-nix packaging
**Repos:** `/home/hunter/Projects/surmount/pqconnect` (branch `feat/nix-flake`, **uncommitted**)
**Surmount note:** `docs/research/pqconnect-local-packaging.md`

## Result

**Green package build** (impure `nix-build` of flake package expressions on
`nixos-25.05`). Flake files written; **not** git-added/committed (operator
owns stage + signed commit).

## Important correction

Upstream PQConnect is **Python** (flit, `pyproject.toml` 1.2.3), **not Rust**.
Crane / rustPlatform are not used. Packaging is `buildPythonApplication` +
native DJB microlibs.

## What landed (pqconnect repo, dirty tree)

| Path | Role |
|------|------|
| `flake.nix` | packages, checks, devShell; nixpkgs `nixos-25.05` |
| `flake.lock` | locks nixpkgs rev `ac62194c3917...` (same family as surmount-server) |
| `README-NIX.md` | Surmount path-input how-to + build status |
| `nix/packages/libmceliece.nix` | native Classic McEliece (Debian orig FOD) |
| `nix/packages/libntruprime.nix` | native NTRU Prime (Debian orig FOD) |
| `nix/packages/python-lib25519.nix` | PyPI ctypes + nixpkgs lib25519 |
| `nix/packages/pymceliece.nix` | PyPI + libmceliece |
| `nix/packages/pyntruprime.nix` | PyPI + libntruprime |
| `nix/packages/netfilterqueue.nix` | NetfilterQueue + cython |
| `nix/packages/pysodium.nix` | vendored (missing on 25.05) |
| `nix/packages/securestring.nix` | vendored (missing on 25.05) |
| `nix/packages/pqconnect.nix` | app; bins below |
| `nix/patches/djb-configure-cc.patch` | DJB configure CC for Nix |
| `nix/patches/djb-staticlib-ar.patch` | DJB staticlib AR/RANLIB |

**Bins:** `pqconnect`, `pqconnect-server`, `pqconnect-keygen`,
`pqconnect-server-check`.

## Build proof

```text
nix-build /tmp/pqconnect-build-full.nix -A pqconnect
# -> /nix/store/...-pqconnect-1.2.3
# pythonImportsCheck: pqconnect, pqconnect.common.crypto
# pqconnect --help / keygen --help / server-check --help: OK
```

Native libs alone also green: `libmceliece-20260622`, `libntruprime-20260717`
(from Debian `orig.tar.xz` because DJB hosts timed out from this network).

## Flake UX caveat (operator action)

Nix flakes hide **untracked** files. After operator review:

```bash
cd /home/hunter/Projects/surmount/pqconnect
git add flake.nix flake.lock nix README-NIX.md
# human: git commit -S ...
nix build
```

Agents did **not** `git add` / commit / push.

## Surmount consumption sketch

```nix
pqconnect.url = "path:/home/hunter/Projects/surmount/pqconnect";
# packages: inputs.pqconnect.packages.${system}.default
```

Not wired into surmount-server flake yet (product **Q-PQC-*** still open).

## Surmount-server file written

- `docs/research/pqconnect-local-packaging.md`
