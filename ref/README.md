# Reference materials (study only)

Put **git submodules** or downloaded references here for reading while you
design modules. Examples:

- Upstream Stalwart docs / example configs
- Community NixOS mail server flakes (for ideas, not copy-paste)
- Old Synology export notes

## Rules

1. **Study only.** Implement our own modules under `modules/` and `nix/`.
2. **No large unattributed copy-paste** into production modules.
3. Prefer linking and short excerpts with clear attribution in comments.
4. Submodules should stay optional; the flake must evaluate without them.

## Adding a submodule

```bash
git submodule add <url> ref/<name>
git submodule update --init --recursive
```

Do not make flake inputs depend on `ref/` unless the input is a proper
locked flake with a clear license and pin.
