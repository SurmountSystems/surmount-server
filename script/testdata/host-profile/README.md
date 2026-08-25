# Host-profile fixtures (synthetic only)

| Path | Role |
|------|------|
| `sample-host-profile.toml` | Public sample profile shape (`example.invalid` only). Multi-SAN (H5): services + mail + apex + www. Staging LE directory (H6: production only with explicit flag or profile field). |
| `expected-host-local-acme.nix` | Expected ACME fragment shape for hermetic render test (space-separated Nix `domains` list; durable H-PEM TLS paths; staging directory). |

Real operator profiles live **outside** public git (e.g.
`~/.local/share/surmount/host-profile.toml`). Rendered
`host-local-acme.nix` is private host-local only. Never put API keys, PEMs,
tokens, or production IPs in fixtures.

Hermetic test: crate tests for `surmount-render-host-profile-acme`.

Docs: `docs/OPS.md`, `docs/SECRETS.md`, `docs/EDGE_AND_TLS.md`,
`docs/deploy-host-local.md`.
