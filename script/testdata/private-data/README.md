# Private-data detector fixtures (synthetic only)

These files exist **only** to prove `script/check-private-data.sh` detects
pattern classes. Every sample is **obviously fake** and must never look like
production material.

| Path | Role |
|------|------|
| `good-host-sample.txt` | Allowlisted TEST-NET + short SSH placeholder (must pass) |
| `bad-*.txt` | One class each (must fail under `--paths`) |

`script/check-private-data.sh --tree` and `--staged` **exclude** this
directory so synthetic hits do not fail product scans. Tests use `--paths`.

**Never** replace these with real keys, tokens, host IPs, or screenshot text.
