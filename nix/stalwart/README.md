# Stalwart apply plans (Surmount-owned)

Hermetic templates and notes for day-2 Stalwart config. Stalwart 0.16+ keeps
listeners, accounts, spam, and TLS objects in the **datastore** (JMAP). The
on-disk `config.json` is DataStore only. Surmount does **not** claim product
clearnet HTTPS on Stalwart.

## P1 product edge (operator direction)

| Port / plane | Owner |
|--------------|--------|
| Public **:80** and **:443** (browser clearnet HTTPS) | **management-ui** (Axum rustls) when `web.enable` is false |
| Mail SMTP / submission / IMAPS / ManageSieve (25/465/587/993/4190) | **Stalwart** |
| Stalwart HTTP management / JMAP bootstrap | **Loopback only** (first-boot default often `[::]:8080`; rebind to `127.0.0.1:8080`) |
| Stalwart HTTPS listener on public **:443** | **Not product clearnet edge.** Free this port for Axum before public B1 cutover |

Detail: [docs/EDGE_AND_TLS.md](../../docs/EDGE_AND_TLS.md),
[docs/OPS.md](../../docs/OPS.md).

## Files installed on host

When `surmount.enable` is true, `modules/mail.nix` installs under
`/etc/surmount/stalwart/`:

| Path | Role |
|------|------|
| `free-public-443-for-axum-edge.ndjson` | Operator `stalwart-cli apply` template to drop Stalwart public HTTPS |
| `README-free-public-443.txt` | Short host-local runbook (query, dry-run, apply) |
| `dkim-signature-ed25519-stalwart.example.ndjson` | Example `DkimSignature` create (Ed25519; selector `stalwart`; edit `DOMAIN_ID_PLACEHOLDER`) |
| `dkim-signature-rsa-stalwart.example.ndjson` | Example `DkimSignature` create (RSA-4096; selector `stalwart-rsa`; edit `DOMAIN_ID_PLACEHOLDER`) |
| `README-dkim-surmount-systems.txt` | Dual-sign DKIM Domain B paths, selectors, DNS + register steps |
| `mail-plane-tls-le-pems.example.ndjson` | Example `Certificate` create (File paths = Axum durable PEMs) |
| `README-mail-plane-tls.txt` | IMAP/SMTP TLS: shared Let's Encrypt PEMs, grant, apply, proof |
| spam-filter / webui FODs (when packaged) | Hermetic resource paths; wire via apply/WebUI |
| `README-mailbox-password-argon2id.txt` | Installed from `modules/mail.nix` (inline text). Stalwart hashes mailbox passwords (default Argon2id); optional CLI pin |
| `README-imap-timeout-anonymous.txt` | Installed from `modules/mail.nix` (inline text). Raise Imap.timeoutAnonymous to 1800000 so Evolution first-open is not cut by `* BYE Connection timed out.` |

## Apply order (after first boot, before public Axum :443)

Prefer the V1 driver (Domain B `stalwart-token`; default dry-run; never logs
token values):

```bash
just free-stalwart-public-443 -- --dry-run
just free-stalwart-public-443 -- --live --restart   # operator host only
```

Token path default: `/var/lib/surmount/secrets/ui/stalwart-api-token` (kind
`stalwart-token`). Installing that file is **not** the same as Stalwart engine
registration; first-boot admin may still be required once. Hermetic self-test:
`just test-free-stalwart-public-443`. Docs: [docs/OPS.md](../../docs/OPS.md),
[docs/SECRETS.md](../../docs/SECRETS.md).

Manual path:

1. Confirm Stalwart is up and HTTP management answers on the first-boot port
   (often `http://127.0.0.1:8080` if already rebound, else tunnel to the host).
2. Query live listeners (names vary by first-boot / upgrades):

   ```bash
   export STALWART_URL=http://127.0.0.1:8080
   # Token or admin creds: host-only; never in git.
   stalwart-cli query NetworkListener --fields id,name,protocol,bind --json
   ```

3. Dry-run the template, then apply only after names match your store:

   ```bash
   stalwart-cli apply --file /etc/surmount/stalwart/free-public-443-for-axum-edge.ndjson --dry-run
   # Edit a host-local copy if first-boot names differ; then:
   stalwart-cli apply --file /etc/surmount/stalwart/free-public-443-for-axum-edge.ndjson
   ```

4. Restart or reload Stalwart if binds do not drop until process restart
   (`systemctl restart stalwart-mail` after a maintenance window).
5. Confirm nothing listens on public :443 except the intended product edge
   (ss/sshd/journal). Then enable `managementUi.listenMode=https` + PEMs
   (or in-process ACME) per host sample comments.

## DKIM signing (send auth)

Stalwart 0.16 stores DKIM as **DkimSignature** objects (WebUI or
`stalwart-cli`). Private keys may be inline secret text or a **File** path
readable by `stalwart-mail`. Multi-signature dual algorithm (Ed25519 + RSA,
different selectors) is supported. Surmount Day-1 convention for this host is
**dual-sign** (both objects when engine token is accepted):

| Item | Ed25519 | RSA-4096 |
|------|---------|----------|
| Selector | `stalwart` | `stalwart-rsa` |
| Algorithm / object | Ed25519-SHA256 (`Dkim1Ed25519Sha256`) | RSA-SHA256 (`Dkim1RsaSha256`) |
| Domain B private key | `/var/lib/surmount/secrets/mail/dkim/stalwart-ed25519.pem` | `/var/lib/surmount/secrets/mail/dkim/stalwart-rsa4096.pem` |
| DNS TXT HostName | `stalwart._domainkey` (`k=ed25519`) | `stalwart-rsa._domainkey` (`k=rsa`) |
| SPF / DMARC | Apex + `_dmarc` TXT (same zone tool); not Stalwart-managed | same |

RSA floor is **4096** (not 2048). Do not use 8192 for DKIM (verifier MUST range
through 4096 per RFC 8301; larger often breaks verification). Neither algorithm
is post-quantum; see [docs/DNS.md](../../docs/DNS.md) hardness note.

Register both signatures only after Domain B has an **engine-accepted** admin
token (same gate as free-443). Random generate is not registration. Host
runbook: `README-dkim-surmount-systems.txt`. Public earn-trust checklist:
[docs/DNS.md](../../docs/DNS.md).
See [Stalwart DKIM signing](https://stalw.art/docs/mta/authentication/dkim/sign/)
(accessed: 2026-08-11).

## Honesty

- This is **not** live B1 cutover. PEMs, DNS, and `just e2e-host` remain
  operator host gates (RESIDUAL.md).
- First boot with an empty store still inserts upstream safe defaults, which
  may include HTTPS :443 until this plan (or WebUI equivalent) runs.
- Do not invent live listener ids in git. Prefer `query` then destroy by
  name filter, or WebUI Settings > Network > Listeners.
- Publishing DKIM **DNS** is not the same as Stalwart **signing** until the
  DkimSignature object exists and uses the matching private key.
- Mail protocol TLS (IMAPS/SMTPS on 465/993) uses the **same durable
  Let's Encrypt PEM files** as product browser HTTPS on :443 after
  `just point-stalwart-mail-tls -- --live`. Nix `settings` still cannot
  pin those files. The leaf must name `mail` on the certificate hostname
  list. Host ACME stays off. See Q-EDGE-1.

See also: [stalwart apply docs](https://stalw.art/docs/management/cli/apply/)
(accessed: 2026-08-10).
