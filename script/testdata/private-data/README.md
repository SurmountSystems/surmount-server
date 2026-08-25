# Private-data detector fixtures (synthetic only)

These files exist **only** to prove `surmount-private-data` detects
pattern classes. Every sample is **obviously fake** and must never look like
production material. Crate tests also keep a hermetic copy under
`crates/surmount-private-data/testdata/`.

| Path | Role |
|------|------|
| `good-host-sample.txt` | Allowlisted TEST-NET + short SSH placeholder (must pass) |
| `bad-*.txt` | One class each (must fail under `--paths`) |

`surmount-private-data --tree` and `--staged` **exclude** this
directory so synthetic hits do not fail product scans. Tests use `--paths`.

**Never** replace these with real keys, tokens, host IPs, or screenshot text.
