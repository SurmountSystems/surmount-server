# Project git hooks

| Path | Role |
|------|------|
| `pre-commit` | Private-data pattern scan on **staged** files |

## How it runs

Operator machines with `core.hooksPath=~/.git-hooks` already chain into this
path after GPG checks:

```text
~/.git-hooks/pre-commit  ->  $repo/script/git-hooks/pre-commit
  -> PATH bin, or crates/target, or nix run .#surmount-private-data -- --staged
```

No per-repo `git config core.hooksPath` is required when that global chain is
set. Optional local-only: `git config core.hooksPath script/git-hooks` (skips
global GPG unless you compose both).

## Manual runs

```bash
nix run .#surmount-private-data -- --staged
nix run .#surmount-private-data -- --tree
just check-private-data -- --staged
```

## Rules

- **Patterns only.** Never put real IPs, keys, tokens, or screenshot contents
  into hooks, fixtures, docs, residual, or reports.
- On hit the scanner prints **class + file path** only (not the secret line).
- Fix: remove material from the index; keep PEMs, age identities, SSH material,
  and real public host addresses on the **host** only.
- Synthetic detector fixtures live under `script/testdata/private-data/` and are
  excluded from `--tree` / `--staged`.

Detail: [docs/hygiene.md](../../docs/hygiene.md) (pre-commit private-data scan).
