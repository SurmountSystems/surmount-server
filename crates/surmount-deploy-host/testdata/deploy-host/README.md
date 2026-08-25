# Deploy-host driver fixtures (synthetic only)

Copied from `script/testdata/deploy-host/` for hermetic crate tests.

| Path | Role |
|------|------|
| `good-host-local/` | Minimal host-local with a **synthetic** long SSH public key line (not a real operator key) |
| `empty-keys-host-local/` | authorized_keys with comments only (must fail lockout check) |
| `short-placeholder-keys-host-local/` | Non-comment short `AAAA...` placeholder (must fail usable-key check) |

**Never** replace these with real host keys, IPs, PEMs, or production identity.
