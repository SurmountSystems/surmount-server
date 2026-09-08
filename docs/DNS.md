# DNS checklist for mail (every domain we send or receive through)

Correct DNS matters as much as the MTA. Start with the
[Day-1 checklist](#day-1-dns-checklist-operator) (registrar managed DNS,
edge A/AAAA, then MX/PTR). Use the full earn-trust list before sending
real user mail. Validate send path with
[mail-tester.com](https://www.mail-tester.com/) and your provider's
blocklist checks.

**Last updated:** 2026-09-07 (intended production leaf is **20**
certificate hostnames on one Let's Encrypt PEM (`with_single_cert`): the
live 18 plus `cryptoquick.com` and `www.cryptoquick.com`. Live leaf
still has **18** names (CT). Validating A for cryptoquick apex/www
succeeds (AD true). Leftover parent DS key tag 2368 is gone. HTTPS
failure on those two names is certificate hostname mismatch, not
SERVFAIL. The old wait-for-SERVFAIL gate is closed as a live A-lookup
gate. SHA-1 parent DS digest type 1 remains standing DNSSEC quality debt
in operator-facts Monday leftover; it is not the HTTPS cause. Do not
invent leftover Namecheap clicks. Do not MX-flip Baxter. Esplora Hosts
stay off this leaf.) Prior 2026-08-24 (self-asserted BIMI TXT at
`default._bimi.surmount.systems`:
`v=BIMI1; l=https://surmount.systems/bimi.svg;`. Logo is
`https://surmount.systems/bimi.svg` (favicon-form Tiny-PS SVG, site repo
root, same serve as `/favicon.svg`). A Verified Mark Certificate is a
**separate leftover**. Gmail often still wants that cert before showing
the logo. Do not invent a VMC.) Prior 2026-08-20 (mail records on every domain we send or
receive through, same class as the primary, including DNSSEC. Do **not**
use `p=reject`. **Live 2026-08-20 public vs API:**
`surmount.systems` public MX `10 mail.surmount.systems`, EmailType **MX**,
SPF `v=spf1 a:mail.surmount.systems -all`, dual DKIM, TLS-RPT, CAA Let's
Encrypt; public `_dmarc` at `1.1.1.1` is **`p=quarantine`** (do **not**
use `p=reject`; unsigned, no DS). `cryptoquick.com` child zone
(`dig @1.1.1.1 +cd`): dual DKIM, TLS-RPT, CAA, SPF
`a:mail.cryptoquick.com -all`, MX `10 mail.cryptoquick.com`, DMARC
**`p=quarantine`** (leave as-is). That day's validating resolvers
**SERVFAIL**ed on leftover parent DS key tag **2368** (alg 13 digest
type 1 SHA-1, no child DNSKEY). That A-lookup gate is **closed** as of
2026-09-07. **Live 2026-08-21:** `mail.cryptoquick.com` **is** on the shared production
leaf (IMAP/SMTP identity). Extra MTA-STS wait (not on this leaf). `baxterartworks.com` EmailType still
**FWD**; public MX still eforward1-5 (do **not** claim MX flipped).
Mailbox `_dmarc.baxterartworks.com` **intended** is **`p=quarantine`**
(operator 2026-08-20; rua `admin@baxterartworks.com`). Do **not** use
`p=reject`. Do **not** restore getHosts `_dmarc` to `p=none` (a later
extra-mail slice wrongly stored `p=none` after getHosts already had
`p=quarantine`). Namecheap getHosts still has dual DKIM (`stalwart` +
`stalwart-rsa`, same `p=` as primary), TLS-RPT, CAA Let's Encrypt, SPF
`v=spf1 a:mail.surmount.systems include:spf.efwd.registrar-servers.com -all`.
Public recursive `1.1.1.1` 2026-08-20: DKIM / `_dmarc` / TLS-RPT
**NXDOMAIN**, CAA empty NOERROR, SPF still eforward-only `~all`, SOA
serial **1787245654** not bumped. Public `_dmarc.baxterartworks.com`
may stay **NXDOMAIN** while EmailType is **FWD** even when getHosts
has the TXT. Leftover `_acme-challenge` TXT still on getHosts. Extra
MTA-STS not published.
`--live set-mx` **fails closed** while FWD. `domain-audit` **FAIL**s
registrar eforward on a claimed mailbox. Extra domains do **not**
entirely lack dual DKIM / TLS-RPT / CAA: cryptoquick child has them;
Baxter getHosts has them; Baxter public NS has not republished.) Prior:
17 then 18 certificate hostnames; extra-vhost HTTPS live for six static
zones. Intended leaf is 20 names as of 2026-09-07.

Living companions: [EDGE_AND_TLS.md](EDGE_AND_TLS.md),
[research/tls-trust-and-acme.md](research/tls-trust-and-acme.md),
[operator-direction.md](operator-direction.md), [OPS.md](OPS.md),
[open-choices.md](open-choices.md) (DNS hosting options; open),
[deploy-host-local.md](deploy-host-local.md) (B6 mail legitimacy),
[SECURITY.md](SECURITY.md), [COMPACTION-PIN.md](COMPACTION-PIN.md),
[../AGENTS.md](../AGENTS.md) (mail records standing law).

---

## Mail records on every domain we send or receive through

Operator direction **2026-08-20**. Standing law. Dual-pin:
[../AGENTS.md](../AGENTS.md), [SECURITY.md](SECURITY.md),
[COMPACTION-PIN.md](COMPACTION-PIN.md).

Whenever this host **sends or receives mail** for a domain (primary
`surmount.systems`, extra local domains such as `cryptoquick.com` and
`baxterartworks.com`, or any later mailbox domain), that domain gets
the **same thoughtful mail-record hardening** we already treat as
required on the primary, **including primary-class DNSSEC**. **Do not**
harden only `surmount.systems` and leave extra mail domains on leftover
Cloudflare, a parking URL, leftover-unsigned DNSSEC, missing SPF,
missing dual DKIM TXT, missing TLS-RPT, or a leftover parent **DS**
that makes validating resolvers SERVFAIL.

Apply, for **each mail domain we actually use**:

| Record class | What "same as primary" means |
|--------------|------------------------------|
| Public DNS | Working nameservers we can edit. Namecheap hosted DNS when we are the registrar. Leftover Custom DNS / Cloudflare NS is not enough for records we must change. |
| DNSSEC honesty | Mail domains get the **same hosted DNSSEC** as `surmount.systems`: Namecheap **DNSSEC Status** **ON**. Leftover parent **DS** without a matching child DNSKEY SERVFAILs validating resolvers (`cryptoquick.com` was this). That is a **bug to clear then sign**, not a reason to stay unsigned. **DS digest type 1 (SHA-1) is also a fail**, even if a DNSKEY exists. Audit **INFO** for unsigned with no DS is not the product end state. Published Namecheap API has **no** DNSSEC/DS commands. Zone tool is A/AAAA/TXT/CAA/MX only. **Sequencing:** we asked cryptoquick **OFF** only because leftover DS key tag **2368** (algorithm 13, digest type 1) had no DNSKEY. **Live 2026-09-07:** leftover parent DS key tag 2368 is **gone**. Validating A for cryptoquick apex/www succeeds (AD true). That SERVFAIL wait is **not** the live A-lookup gate. SHA-1 parent DS digest type 1 remains standing DNSSEC quality debt in operator-facts Monday leftover; it is not the HTTPS cause. Do **not** re-add 2368 by hand. Do **not** invent leftover Namecheap clicks. Intended production leaf is **20** certificate hostnames on one PEM (`with_single_cert`). Live leaf still has **18** names and still omits `cryptoquick.com` / `www.cryptoquick.com`. **Live 2026-08-21:** `mail.cryptoquick.com` is on the shared production leaf. Path: [Best DNSSEC we can actually run](#best-dnssec-we-can-actually-run). Remaining work: [RESIDUAL.md](../RESIDUAL.md). |
| SPF | TXT that authorizes this host (and the current MX while forwarding is still live). |
| Dual DKIM TXT | Selectors we actually sign with: `stalwart` (Ed25519) **and** `stalwart-rsa` (RSA-4096). Not one selector. |
| DMARC | Operator-directed policy. Do **not** use `p=reject`. **Live 2026-08-20:** `surmount.systems` public `_dmarc` is **`p=quarantine`**; `cryptoquick.com` child (`+cd`) is **`p=quarantine`** (leave as-is); `baxterartworks.com` `_dmarc` **intended** is **`p=quarantine`**. Public recursive may stay NXDOMAIN while EmailType is FWD even when getHosts has the TXT. Do **not** restore getHosts `_dmarc` to `p=none`. Keep existing rua/ruf and aspf/adkim unless missing. |
| TLS-RPT, CAA, MTA-STS | Same classes as the primary, **on names this cert and this host actually serve**. Product MTA-STS HTTPS today is `mta-sts.<primaryDomain>` only; extra `_mta-sts` / `mta-sts` Hosts wait until those names are on the leaf **and** the edge serves them. |
| rDNS / PTR | When we control reverse DNS (SHC), for hosts we send through. Independent of extra-domain forward zones. |

**Mail domain vs static vhost:** extra static Hosts (apex/www files on this
box) are **not** automatically mail domains. Unowned domains are not
ours to publish records on.

**Local aliases vs public MX:** local Stalwart Domains and aliases can
exist before public MX points here. **Primary** `surmount.systems`
public MX is **live** (2026-08-20): `10 mail.surmount.systems` after
the operator Custom MX click and laptop `--live set-mx`.
`cryptoquick.com` public MX is already this host (`10 mail.cryptoquick.com`,
Custom MX). Validating A for cryptoquick apex/www succeeds as of
2026-09-07 (AD true). Leftover parent DS key tag 2368 is gone.
`baxterartworks.com` public MX stays **parked** (EmailType **FWD**,
eforward1-5) until the operator asks for Custom MX. `--live set-mx`
fails closed while FWD. Do not invent a Baxter MX flip.

**Laptop Namecheap custody:** ClientIp is **laptop egress**, not the VPS.
Zone tool and DNS-01 run from the laptop. Extra zones use
`--credentials` for that SLD.TLD env file. Audit an extra apex with
`just domain-audit -- cryptoquick.com` (default is primary only).
Leftover parent DS without DNSKEY is a **fail**. DS digest type 1
(SHA-1) is a **fail** even when a DNSKEY exists. Clearing that DS is
sequencing, then the same hosted **DNSSEC Status ON** as the primary.

---

## Best DNSSEC we can actually run

One setup. Not an IETF menu. Same algorithm path for mail zones and
static-site HTTPS. Static-site-only extra vhosts are **not**
automatically mail domains; DNSSEC is still useful for HTTPS when NS is
Namecheap hosted.

**Click path (hosted NS only):** Namecheap **Domain List**, **Manage**
the SLD, **Advanced DNS**, **DNSSEC Status**, toggle **ON**. Namecheap
generates keys, signs the zone, and submits matching DS. There is no
algorithm picker and no digest picker. Do **not** use the Custom-NS DS
form on hosted NS. Do **not** hand-add DS that does not match live
DNSKEY.

**Algorithm (we cannot pick; we verify):** operator accepts **ECDSA
P-256 SHA-256** (algorithm **13**) for now. Prefer a matching parent DS
with digest type **2** (SHA-256). Never digest type **1** (SHA-1) as the
intended end state. Never hand-add unmatched DS. Never re-add key tag
**2368**. After toggle, wait about **60 minutes**, then verify live
DNSKEY and a matching parent DS before claiming **Secure**. Prefer
digest type 2 on a new DS. Ed25519 algorithm 15 only if they actually
publish it (the UI does not offer a picker).

**Mail-domain enable is the goal (not leftover-unsigned):**

| Apex | Now | Why |
|------|-----|-----|
| `cryptoquick.com` | Leftover parent DS key tag **2368** is **gone** (live 2026-09-07). Validating A for apex/www succeeds (AD true). Same hosted **DNSSEC Status ON** as `surmount.systems` is still the goal. | Mail domain, not second-class. That leftover unmatched DS was a SERVFAIL **bug to clear then sign**, not a permanent off. The A-lookup SERVFAIL gate is **closed**. SHA-1 parent DS digest type 1 remains standing DNSSEC quality debt in operator-facts Monday leftover; it is not the HTTPS cause. Do **not** re-add 2368 by hand. Do **not** invent leftover Namecheap clicks. Intended production leaf includes cryptoquick apex/www on the same PEM (20 names). Live leaf still omits those two names (18 names). Prove **Secure** when claiming DNSSEC end state. |
| `surmount.systems` | Operator already clicked hosted **ON**. | **Live 2026-08-20:** not yet Secure. Parent DS NXRRSET. No apex DNSKEY. SOA unsigned. NS Namecheap registrar-servers. Not SERVFAIL. Waiting for Namecheap to publish matching DS+DNSKEY. Algorithm 13 is acceptable. |
| Other Namecheap-hosted mail zones (`baxterartworks.com` when it is a mailbox domain) | Same hosted **ON** as the primary. Do **not** enable during a broken mid-flip. | Same class, not leftover-unsigned. Static-site-only extra vhosts are **not** automatically mail domains. Operator click only. Agents cannot toggle via API. |

**Do not API.** Published Namecheap `domains.dns` has no DNSSEC method
(setDefault / setCustom / getList / getHosts / getEmailForwarding /
setEmailForwarding / setHosts only). The zone tool cannot toggle
DNSSEC. Leftover parent DS with no matching DNSKEY still SERVFAILs
until that DS drops, even after the hosted toggle is ON.

Namecheap hosted 2026 docs (accessed: 2026-08-20):
[Managing DNSSEC for domains pointed to PremiumDNS or BasicDNS](https://www.namecheap.com/support/knowledgebase/article.aspx/9723/2232/managing-dnssec-for-domains-pointed-to-premium-or-basicdns/),
[Nameservers and TLDs supported or unsupported by DNSSEC](https://www.namecheap.com/support/knowledgebase/article.aspx/9718/2232/nameservers-and-tlds-supportedunsupported-by-dnssec/).
`.systems` is not on their unsupported TLD list. Unsupported classes:
Shared/Reseller NS; PremiumDNS for domains not registered at Namecheap.

IANA DS digest type **1** (SHA-1) is **MUST NOT** for new delegations:
[DS RR Types](https://www.iana.org/assignments/ds-rr-types/ds-rr-types.xhtml)
(accessed: 2026-08-20). `domain-audit` **FAIL**s any DS RDATA whose
digest type is 1, even if it is the only DS, and even if a DNSKEY
exists. Unmatched DS (no DNSKEY) stays FAIL. Unsigned with no DS stays
INFO.

Dual-pin: [OPS.md](OPS.md), [../RESIDUAL.md](../RESIDUAL.md),
[../AGENTS.md](../AGENTS.md).

---

## Nameserver hosting (open; options, not product lock)

Surmount documents **which records** to publish. **Where** the zone is hosted
is an operator choice. Status in open-choices: **open**.

| Option | Summary | Recommended when |
|--------|---------|------------------|
| **A. Registrar managed DNS** | Namecheap BasicDNS or any equivalent managed DNS at the registrar | **Day-1 / B6** (default recommendation) |
| **B. Self-hosted authoritative** | BIND, PowerDNS, or similar on operator-owned NS (often later Track C) | After Day-1, when ranked; plan glue + secondaries |

**Day-1 lean (not locked):** managed DNS at the registrar. Matches B6
("Registrar/managed DNS; not self-host DNS required"). Product does not ship
a BIND/PowerDNS module as a Day-1 requirement.

**Cloudflare:** no CF product on the critical path (proxy, Access, Workers,
WAF, Tunnel). **DNS-only** at Cloudflare or any other registrar is allowed.

**rDNS / PTR:** the **VPS provider** owns the reverse zone. For Surmount on
**Sovereign Hybrid Compute (SHC)**, set PTR via the **SHC customer user-api**
when an operate-scoped API key is present (`kind=shc-api`, tool
`nix run .#surmount-shc` / `just rdns-shc`), or use the
provider **services management console**. You set the reverse hostname (the
PTR value others resolve with `dig -x`). Independent of Option A vs B for the
forward zone. **Not** registrar forward DNS (Namecheap is forward only).

**ACME DNS-01:** operator-owned **external-hook** against whatever DNS you
control; not a locked commercial DNS brand in product code.

**Namecheap BasicDNS (optional install-ready hook):** when the forward zone
lives at Namecheap managed DNS, the tree ships
`nix run .#acme-dns-hook-namecheap-bin`
for product `dnsProvider = "external-hook"`. Credentials only on the host
(durable default `/var/lib/surmount/secrets/acme/namecheap.env`; never in git).
Install via Domain A → B kind **`namecheap-api`**
(`nix run .#secrets-install-host`) or manual place.
Install steps, argv protocol, and lab vs production: [OPS.md](OPS.md)
(external-hook section). Hermetic self-test (no live API):
`just test-acme-dns-hook-namecheap`. Other registrars or self-hosted NS use
their own hook (or the lab sample for training). Product still does not lock a
commercial DNS SDK.

**Zone API residual vs Day-1 / ACME TXT (V0 honesty):**

| Work | Status | Notes |
|------|--------|--------|
| Day-1 A/AAAA/MX/PTR/SPF/DKIM/DMARC | **Operator checklist** (this file) | Registrar UI + VPS PTR; valid without automation |
| ACME DNS-01 `_acme-challenge` TXT | **Shipped** (external-hook + optional Namecheap hook) | Credentials Domain B `namecheap.env`; not Day-1 zone rewrite |
| Optional zone tool A/AAAA/TXT/CAA/MX/delete | **Shipped** (`nix run .#surmount-dns-zone`) | Same API credentials; dry-run default; not a DNS brand lock. CAA is real RecordType=CAA (not TXT). `delete-host HOST --type TYPE` drops URL/CNAME/AAAA and similar after getHosts merge (empty getHosts fail-closed). Primary MX live 2026-08-20; extra-domain MX stays L3-gated |
| PTR / rDNS | **VPS provider** (SHC API or console) | Not Namecheap forward API. Optional: `just rdns-shc` when `shc-api` key present |

Day-1 can stay fully manual. ACME TXT automation does **not** replace the
Day-1 edge A/AAAA checklist.

**Optional Namecheap zone tool (not a Day-1 requirement):**
`nix run .#surmount-dns-zone` merges
**A**, **AAAA**, **TXT** (SPF / DKIM / DMARC / MTA-STS / TLS-RPT and similar),
**CAA** (real CAA RRs), and **MX** for a host label via the same Namecheap
`getHosts` / `setHosts` API and the **same** Domain B credentials file as the
ACME hook. `delete-host HOST --type TYPE` drops one host+type (parking
`URL` at `@`, parking `CNAME` at `www`, leftover `AAAA`, and similar) and
re-applies the rest. Empty getHosts fails closed (does not wipe). A drop
that would leave an empty zone is refused. Default is **dry-run** (print
plan; no write). Pass `--live` to apply. Hermetic self-test (mock zone; no
network): `just test-dns-zone-namecheap`.

```bash
just dns-zone-namecheap -- list
just dns-zone-namecheap -- set-a services 203.0.113.10
just dns-zone-namecheap -- set-txt @ 'v=spf1 a:mail.example.test -all'
just dns-zone-namecheap -- set-txt _dmarc 'v=DMARC1; p=quarantine; rua=mailto:admin@example.test; pct=100'
just dns-zone-namecheap -- set-txt 'stalwart._domainkey' 'v=DKIM1; k=ed25519; p=...'
just dns-zone-namecheap -- set-txt 'stalwart-rsa._domainkey' 'v=DKIM1; k=rsa; p=...'
just dns-zone-namecheap -- set-txt _mta-sts 'v=STSv1; id=2026081101'
just dns-zone-namecheap -- set-txt 'default._bimi' 'v=BIMI1; l=https://surmount.systems/bimi.svg;'
just dns-zone-namecheap -- set-caa @ 0 issue letsencrypt.org
just dns-zone-namecheap -- set-caa @ 0 issuewild letsencrypt.org
just dns-zone-namecheap -- set-mx @ mail.example.test. --pref 10   # tooling only until L3
just dns-zone-namecheap -- delete-host @ --type URL
just dns-zone-namecheap -- delete-host www --type CNAME
just dns-zone-namecheap -- --live set-txt _dmarc 'v=DMARC1; p=quarantine; ...'
```

| Still manual / other path | Zone tool |
|---------------------------|-----------|
| Day-1 registrar UI for A/AAAA/MX/TXT (always valid) | Optional automation after credentials exist on Domain B |
| **PTR/rDNS** at **VPS provider** | Never this tool (forward zone only). Use SHC console or `just rdns-shc` |
| ACME `_acme-challenge` TXT | ACME external-hook only (not the zone tool) |
| Stalwart **signing** (DkimSignature object) | Engine admin token + apply/WebUI; DNS publish alone does not sign |
| Product **MX flip** to `mail` (L3 gate) | **Primary live 2026-08-20:** public MX `10 mail.surmount.systems`; EmailType **MX**; SPF `a:mail.surmount.systems -all`; public DMARC `p=quarantine`. Dual-sign + PTR already green. `list` prints EmailType. `--live set-mx` still **fails closed** while EmailType is FWD (not a public MX flip while Email Forwarding is on). `cryptoquick.com` public MX is already this host. `baxterartworks.com` stays FWD / eforward until the operator asks. No in-tree EmailType-set API |

**set-txt merge rules:** SPF (`v=spf1...`) and DMARC (`v=DMARC1...`) replace
only matching-shaped TXT at that HostName (other TXT kept). Other TXT values
(including DKIM, MTA-STS, TLS-RPT) replace all TXT at that HostName. Relative
multi-label names such as `stalwart._domainkey` and `_mta-sts` are valid
HostNames.

**set-caa merge rules:** writes real **RecordType=CAA** (Namecheap Address
shape `FLAGS TAG "VALUE"`, e.g. `0 issue "letsencrypt.org"`). Replaces CAA at
that HostName with the same tag only (call twice for `issue` + `issuewild`).
Do **not** use `set-txt` for CAA.

**set-mx merge rules:** writes real **RecordType=MX** with `MXPref`. Replaces
all MX at that HostName with one exchange + preference. `list` / getHosts
print **EmailType** (FWD vs MX). **`--live set-mx` is not a public MX flip
while Email Forwarding is on.** If EmailType is FWD, `--live set-mx` exits
non-zero and does not call setHosts. Fail text: Email Forwarding is still
on; change Mail Settings to Custom MX, then retry. UI: Domain List, Manage
the apex, Advanced DNS, Mail Settings, Email Forwarding -> Custom MX.
There is **no** in-tree EmailType-set API. Other merges still preserve
EmailType on setHosts so omitting it does not un-publish the zone. After
the Custom MX click, `--live set-mx` persists MX that public NS will
serve. **Primary `surmount.systems` is that state (2026-08-20):** EmailType
MX, public MX `mail.surmount.systems`, leftover eforward MX gone.

**Full-zone replace hazard:** Namecheap `setHosts` rewrites the **entire**
host list. Both the ACME hook and the zone tool always `getHosts` first and
re-apply other records (and preserve `EmailType`, e.g. FWD). **Do not** run
zone-tool `--live` while an ACME DNS-01 challenge `set`/`clear` is in flight
(both rewrite the full zone). Empty or malformed `getHosts` fails closed (no
wipe). After several sequential `--live` merges, if API `list` shows records
but registrar NS lag, one more full merge (or re-run the last `--live`) often
forces NS publication (same class of issue as Day-1 A records with EmailType).

### Optional SHC rDNS tool (PTR; not Namecheap)

When the VPS is on **Sovereign Hybrid Compute**, reverse DNS can be set through
the customer user-api (operate-scoped Bearer key). Provider console rDNS
remains valid. Namecheap is **forward zone only**.

| Item | Detail |
|------|--------|
| Secret kind | `shc-api` (Domain A intake: `just secrets-prompt -- shc-api --host surmount-1`) |
| Domain B path | Durable `/var/lib/surmount/secrets/rdns/shc.env` (KEY=value: `ApiKey`, `ApiBase`, optional `ServiceId`) |
| Tool | `nix run .#surmount-shc` / `just rdns-shc` (default **dry-run**) |
| Live apply | `--live` (API confirmation dance: 409 `confirmation_required` then `X-User-Api-Confirm`) |
| Hostname | **`mail.surmount.systems`** (locked plan for Surmount mail) |
| FCrDNS | Forward A for that hostname must already point at the IP (API returns 422 otherwise) |
| Hermetic test | `just test-rdns-shc` (mock API; no network; never live key as CI green) |

```bash
just secrets-prompt -- shc-api --host surmount-1
# optional install to Domain B:
just secrets-install-host -- --from-staging "$HOME/.local/share/surmount/staging" \
  --host-id surmount-1 --target root@YOUR_HOST --require-kind shc-api
just rdns-shc -- --list
just rdns-shc -- --list-vms
just rdns-shc -- --hostname mail.surmount.systems --ip YOUR_VPS_IP   # dry-run
just rdns-shc -- --live --hostname mail.surmount.systems --ip YOUR_VPS_IP
```

Never put production IPs or ApiKey values in public git. Credentials only on
Domain A/B paths or private env (`SURMOUNT_RDNS_SHC_ENV`).

**Citations (API facts):**

- SHC customer API / MCP knowledge base:
  [The Customer API and MCP](https://blesta.sovereignhybridcompute.com/plugin/support_manager/knowledgebase/view/16/the-customer-api-and-mcp/)
  (accessed: 2026-08-11)
- OpenAPI:
  [user-api openapi.json](https://blesta.sovereignhybridcompute.com/user-api/openapi.json)
  (accessed: 2026-08-11). Base `https://blesta.sovereignhybridcompute.com/user-api/v2`.
  rDNS: `GET|POST|DELETE /vm/{serviceId}/rdns`.

---

## Day-1 DNS checklist (operator)

Short path for first public edge and (when ready) inbound mail. Use
**registrar managed DNS** (Namecheap BasicDNS or any equivalent). Do **not**
stand up self-host NS / Track C for Day-1.

**Where you click:**

| Place | What you set there |
|-------|--------------------|
| **Registrar DNS UI** (forward zone) | A, AAAA, MX, TXT (SPF/DKIM/DMARC later), optional CNAME/SRV |
| **VPS provider rDNS** (SHC API and/or services console) | Reverse name for the box IPv4 and IPv6. Provider owns the reverse zone. On SHC: optional `just rdns-shc` with `shc-api` key, or console self-service; **you** enter the reverse hostname (the PTR value), typically `mail.<domain>`. Not Namecheap. Not a support ticket unless self-service is missing. |
| **Not Day-1** | Glue to your own NS, BIND/PowerDNS on the mail VPS, secondary NS plan |

Replace hostnames if your box uses different names. Examples use the scaffold
names (`mail` / `services` / apex). Use **documentation addresses only** in
docs; put real addresses only in host-local or the registrar UI.

### Order (relative to HTTPS and mail)

| Step | When | Records | Where | Unblocks |
|------|------|---------|-------|----------|
| **1. Edge names** | Before public HTTPS (B1) | A/AAAA for `services` (and apex/www if used) | Registrar | Certs + management UI HTTPS |
| **2. Mail host name** | Before MX or mail TLS on that name | A/AAAA for `mail` | Registrar | MX target resolves; mail cert name |
| **3. Inbound MX** | When ready to accept mail on this host | MX apex (and each extra domain) -> `mail` | Registrar | Inbound SMTP |
| **4. rDNS (PTR)** | Before or as you start **sending** | Reverse IPv4/IPv6 -> `mail.example` | **VPS provider** (SHC `just rdns-shc` or console) | Outbound reputation |
| **5. Send auth** | When **sending** real mail (not edge-only) | SPF, DKIM, DMARC TXT | Registrar | Deliverability (B6) |
| **6. Later depth** | After send path is green | MTA-STS, TLS-RPT, DNSSEC, DANE | Registrar (+ HTTPS policy host) | Earn-trust depth; not Day-1 |

**Edge-only Day-1:** steps **1** (and **2** if the certificate hostnames include `mail`).
Skip MX, PTR, SPF/DKIM/DMARC until you care about mail.

**Mail Day-1 (accept + send):** steps **1-4**, then **5** before or as you
send. Full ordered boxes: [Earn-trust checklist](#earn-trust-checklist-order-of-operations).

### Day-1 boxes (registrar)

- [ ] Zone uses **managed DNS at the registrar** (BasicDNS or equivalent).
      Nameservers still at registrar; no self-host NS for Day-1.
- [ ] **A** (and **AAAA** if you have v6) for `services.<domain>` -> VPS
- [ ] **A/AAAA** for apex and/or `www` if you serve them from this VPS
- [ ] **A/AAAA** for `mail.<domain>` -> same VPS (when mail or mail cert needs it)
- [ ] **MX** on apex -> `mail.<domain>` priority 10 (when accepting mail)
- [ ] No orange-cloud / required CDN proxy on MX or submission names
- [ ] Records match what host-local / sample config expects (direct to VPS)

### Day-1 boxes (VPS provider, not registrar)

- [ ] **PTR/rDNS** for primary IPv4 -> `mail.<domain>` (when sending)
- [ ] **PTR** for primary IPv6 if you send on v6
- [ ] Forward A/AAAA for `mail` and PTR **match** (same name both ways)

### Not a Day-1 blocker (edge-only or pre-send)

Do these when you **send** mail (B6), not as a gate for first HTTPS:

- [ ] **SPF** TXT on each sending domain (every mailbox domain, not only primary)
- [ ] **DKIM** dual-sign TXT (`stalwart._domainkey` Ed25519 + `stalwart-rsa._domainkey` RSA-4096)
- [ ] **DMARC** TXT `_dmarc` (do not use `p=reject`; live 2026-08-20: primary, cryptoquick, and Baxter intended `_dmarc` are `p=quarantine`; Baxter public recursive may NXDOMAIN while EmailType is FWD)
- [ ] MTA-STS, TLS-RPT, DNSSEC, DANE (see earn-trust sections below)

Smoke after publish (no secrets):

```text
nix run .#surmount-domain-audit -- <apex-domain>
# or: just domain-audit
# optional: just domain-audit -- --smtp --no-color <apex-domain>
```

Default DKIM selectors already include Surmount dual-sign (`stalwart`
Ed25519 + `stalwart-rsa` RSA-4096) plus common third-party names. Use
`--selector NAME` to add more. Hermetic smoke (no network):
`just test-domain-audit`. **Not** a flake check.

Interpretation notes:

- `nix run .#surmount-domain-audit` = richer PASS/WARN/FAIL/INFO; exit 0/1/2/64.
  Default apex is the primary. Pass another mailbox domain after `--`
  (`just domain-audit -- cryptoquick.com`). There is no in-tree loop
  over extra mail domains. For a claimed mailbox domain, registrar
  **eforward** public MX is a **FAIL** (not warn-only). If
  `SURMOUNT_DOMAIN_AUDIT_EMAIL_TYPE=FWD` (from a prior `list`), that is
  also FAIL. Static-site leftover eforward is not a mailbox FAIL. The
  audit does **not** call the Namecheap API (no Invalid-request-IP
  hammer).
- Missing **MTA-STS** (no `_mta-sts` marker) is INFO. Extra mailboxes
  do not require MTA-STS HTTPS until the cert covers `mta-sts.<apex>`.
  A marker whose HTTPS policy cannot be retrieved WARNs on extra mail
  and FAILs on the primary.
- Missing **TLS-RPT** is FAIL on a claimed mailbox domain and INFO on
  static-site-only vhosts.
- Missing/unsigned **DNSSEC** (no DS) is INFO, not WARN. Parent **DS**
  without a matching child DNSKEY is FAIL (leftover DS after leaving a
  signing NS). That is not "unsigned on purpose." DS digest type 1
  (SHA-1) is FAIL even if a DNSKEY exists.
- **DMARC** `p=none` is a normal monitor start; audit may still WARN on
  that policy. **Live 2026-08-20 mailbox policy:** `surmount.systems`
  public, `cryptoquick.com` child, and `baxterartworks.com` intended
  `_dmarc` are **`p=quarantine`**. Public `_dmarc.baxterartworks.com`
  may stay NXDOMAIN while EmailType is FWD even when getHosts has the
  TXT. Do **not** restore Baxter getHosts to `p=none`. Do **not** use
  `p=reject`.
- After send auth is live, use mail-tester (or equivalent) for end-to-end
  DKIM pass. Detail: sections below and [OPS.md](OPS.md) first-boot
  runbook.

---

## Earn trust (framing)

Internet mail does not trust you because the server boots. Receivers score
**identity, alignment, and transport hygiene**. Surmount's job is to **earn
trust** with boring, automatable DNS + TLS + auth signals, then keep them
green.

| Goal | What receivers look at |
|------|-------------------------|
| You own the domain | DNS control; stable apex/MX |
| You own the sending IP | **PTR/rDNS** matches forward name |
| Messages are authorized | **SPF** + **DKIM** alignment |
| Policy is explicit | **DMARC** (monitor then enforce) |
| Transport is authentic | Valid certs; prefer **MTA-STS** + **TLS-RPT**; later **DANE/TLSA + DNSSEC** |
| You are not a spam source | Content + reputation + greylist/spam path (Stalwart); clean IP |

This is **not** a third-party WAF or CDN product. Traffic stays **direct to
the VPS**. DNS registrar may be anywhere (including Cloudflare **DNS-only**);
orange-cloud is not required and must not become the only path.

Certificate automation should stay **affordable and automated** (public ACME
CAs are common). Custom/private CA and DANE paths stay **open** without
locking product law to one CA brand. See tls-trust research.

---

## Earn-trust checklist (order of operations)

Use this as the operator run-up before real user mail. Check boxes in ops
notes as you go.

### A. Identity and routing

- [ ] **A/AAAA** for `mail.surmount.systems` point at the VPS (direct)
- [ ] **A/AAAA** for services / apex / www as needed
- [ ] **MX** apex (and each additional domain) -> `mail.surmount.systems`
- [ ] **PTR/rDNS** on provider IP(s) -> `mail.surmount.systems` (forward/reverse match)
- [ ] No required CDN/proxy hop for MX or submission

### B. Message authentication (authorize what you send)

- [ ] **SPF** TXT on each sending domain (`-all` when ready; avoid softfail forever)
- [ ] **DKIM** dual-sign keys in Stalwart; both public TXT selectors published
      (`stalwart` + `stalwart-rsa`; not one selector)
- [ ] **DMARC** TXT `_dmarc` with rua; do **not** use `p=reject`.
      Live 2026-08-20: primary public and cryptoquick child
      **`p=quarantine`**; Baxter `_dmarc` **intended** **`p=quarantine`**.
      Public `_dmarc.baxterartworks.com` may stay NXDOMAIN while
      EmailType is FWD even when getHosts has the TXT.
- [ ] SPF + DKIM **align** with the visible From domain (DMARC alignment)
- [ ] Same auth stack for **every mailbox domain this host sends or
      receives through** (primary, live extra Stalwart Domains such as
      `cryptoquick.com` and `baxterartworks.com`, and each
      `surmount.additionalDomains` entry). Not only the Nix list. Not
      static-site-only vhosts.

### C. SMTP transport trust

- [ ] Mail certs valid on 465/993 and STARTTLS paths (shared ACME PEMs or chosen path)
- [ ] **TLS-RPT** TXT `_smtp._tls` so failures are visible
- [ ] **MTA-STS** TXT `_mta-sts` + HTTPS policy host (`mode: testing` then `enforce`)
- [ ] Prefer TLS 1.3 posture on related HTTPS policy host (edge)
- [ ] **DNSSEC** signed zone via the Namecheap hosted toggle on **every
      mail domain** (see
      [Best DNSSEC we can actually run](#best-dnssec-we-can-actually-run)).
      Leftover unmatched DS is sequencing (clear then sign), not a
      leftover-unsigned extra domain.
- [ ] **DANE/TLSA** for MX (when DNSSEC live): TLSA matches cert or SPKI; document
      rollover. Open timing: **Q-TLS-2** in tls-trust research

### D. Prove it

- [ ] mail-tester (or equivalent) high score on a realistic message
- [ ] Blocklist check on the sending IP; delist or swap dirty recycled IPs
- [ ] DMARC aggregate reports reviewed before `p=reject`
- [ ] Documented cutover TTL plan (lower before, raise after)

### E. Optional client hints

- [ ] SRV for submission / IMAPS / autodiscover as desired
- [ ] Autodiscover/autoconfig HTTPS if you support those clients
- [x] **BIMI** self-asserted TXT `default._bimi` (live 2026-08-24 on
      `surmount.systems`). A Verified Mark Certificate is a leftover.
      Gmail often still wants that cert before showing the logo.

---

## Records checklist

| Record | Name | Purpose | Trust role |
|--------|------|---------|------------|
| **A / AAAA** | `mail.surmount.systems` | Mail host address(es) | Where MX points |
| **A / AAAA** | `surmount.systems` | Apex (web / redirect) | Web + sometimes SPF `a` |
| **A / AAAA** | `services.surmount.systems` | Management UI | Admin HTTPS |
| **A / AAAA** | `www.surmount.systems` | Optional www | Web |
| **MX** | `surmount.systems` | Prefer `mail.surmount.systems` priority 10 | Inbound routing |
| **PTR / rDNS** | Provider reverse zone for the VPS IP | Must match `mail.surmount.systems` | IP reputation |
| **SPF** | TXT on apex (and each mailbox domain) | Authorize sending IPs | Auth |
| **DKIM** | TXT `stalwart._domainkey` and `stalwart-rsa._domainkey` | Dual-sign public keys from Stalwart | Auth |
| **DMARC** | TXT `_dmarc` | Do not use `p=reject`. Live 2026-08-20: primary public `p=quarantine`; cryptoquick child `p=quarantine`; Baxter intended `p=quarantine` (public recursive may NXDOMAIN while EmailType is FWD) | Policy |
| **BIMI** | TXT `default._bimi` | Self-asserted `v=BIMI1; l=https://surmount.systems/bimi.svg;` (live 2026-08-24). Logo is repo-root `/bimi.svg`. A Verified Mark Certificate is a leftover. Gmail often still wants that cert before showing the logo. | Inbox branding |
| **MTA-STS** | TXT `_mta-sts` + HTTPS policy host | SMTP STS policy | Transport |
| **TLS-RPT** | TXT `_smtp._tls` | TLS failure reports | Transport visibility |
| **DNSSEC** | Zone signing at DNS host | Chain of trust for DNS | Enables DANE |
| **TLSA** | `_25._tcp.mail` (and related) | DANE for SMTP | Transport auth without sole WebPKI |
| **SRV** | `_submission`, `_imaps`, `_autodiscover` | Client autoconfig (optional) | UX |
| **Autodiscover / autoconfig** | CNAME or A + HTTPS | Client setup (optional) | UX |

Also publish SPF, dual DKIM TXT, DMARC, TLS-RPT, and CAA for every
mailbox domain this host actually uses (live extra Stalwart Domains
included). `surmount.additionalDomains` is not the whole list. Do not
treat static-site-only extra vhosts as mail domains. Primary public MX
is live (`mail.surmount.systems`, 2026-08-20). `cryptoquick.com` public
MX is this host. `baxterartworks.com` public MX stays parked until the
operator asks. Extra domains are not "entirely missing" dual DKIM /
TLS-RPT / CAA when the child zone or getHosts already has them; public
NS lag is a separate remaining slice.

### DANE / TLSA note (mail plane)

DANE binds TLS authentication to **DNSSEC-secured** TLSA records so supporting
MTAs can authenticate your mail cert (or public key) without depending only on
public CAs. It complements, not replaces, SPF/DKIM/DMARC.

- Requires working **DNSSEC** end-to-end.
- TLSA must be updated on cert rollover (automation matters).
- Uneven receiver support; still worth planning for earn-trust depth.
- Detail and open timing: [research/tls-trust-and-acme.md](research/tls-trust-and-acme.md)
  (**Q-TLS-2**).

Do not block Day-1 MX on DANE if WebPKI certs + SPF/DKIM/DMARC/rDNS are green;
do not forget DANE as "never."

### BIMI (self-asserted TXT; VMC leftover)

Live 2026-08-24 on the primary zone (`surmount.systems`, EmailType **MX**):

```text
default._bimi.surmount.systems.  TXT  "v=BIMI1; l=https://surmount.systems/bimi.svg;"
```

The logo URL is `https://surmount.systems/bimi.svg`. That file is the
favicon-form plus mark (SVG Tiny 1.2 Portable/Secure, square 512 canvas,
white on black), at the site repository root, served the same way as
`/favicon.svg`. Do not add a flake copy-list for it; `*.svg` at repo root
already ships.

This TXT is **self-asserted**. A **Verified Mark Certificate** is a
**separate leftover**. Gmail often still wants that certificate before
showing the logo in the inbox. Do **not** invent a VMC. Do **not** treat
the self-asserted TXT as proof Gmail will display the mark.

Publish with the laptop Namecheap merge (ClientIp = laptop egress; do not
rewrite ClientIp). Dry-run then `--live set-txt`. Merge; do not wipe the
zone. EmailType must stay **MX**. Do not `--live set-mx`.

```bash
just dns-zone-namecheap -- --credentials ~/.local/share/surmount/issue-le-prod/namecheap.env -- set-txt 'default._bimi' 'v=BIMI1; l=https://surmount.systems/bimi.svg;'
just dns-zone-namecheap -- --credentials ~/.local/share/surmount/issue-le-prod/namecheap.env -- --live set-txt 'default._bimi' 'v=BIMI1; l=https://surmount.systems/bimi.svg;'
```

---

## Sample zone fragment (`surmount.systems`)

Replace `203.0.113.10` and `2001:db8::10` with the real VPS addresses.
Replace DKIM `p=` with the key Stalwart generates.
Examples are **documentation only** (RFC 5737 / 3849 documentation addresses).

```dns
; --- apex and hosts ---
$ORIGIN surmount.systems.
$TTL 3600

@       IN  A       203.0.113.10
@       IN  AAAA    2001:db8::10
@       IN  MX  10  mail.surmount.systems.

www     IN  CNAME   surmount.systems.
mail    IN  A       203.0.113.10
mail    IN  AAAA    2001:db8::10
services IN A       203.0.113.10
services IN AAAA    2001:db8::10

; --- SPF (adjust includes if you use a relay) ---
@       IN  TXT     "v=spf1 mx a:mail.surmount.systems ip4:203.0.113.10 ip6:2001:db8::10 -all"

; --- DMARC (live mailbox policy is p=quarantine; do not use p=reject) ---
_dmarc  IN  TXT     "v=DMARC1; p=quarantine; rua=mailto:dmarc@surmount.systems; ruf=mailto:dmarc@surmount.systems; fo=1; pct=100"

; --- DKIM dual-sign (selectors stalwart + stalwart-rsa; real p= from keys) ---
; stalwart._domainkey IN TXT "v=DKIM1; k=ed25519; p=..."
; stalwart-rsa._domainkey IN TXT "v=DKIM1; k=rsa; p=..."

; --- TLS reporting ---
_smtp._tls IN TXT   "v=TLSRPTv1; rua=mailto:tlsrpt@surmount.systems"

; --- BIMI (self-asserted; Verified Mark Certificate is a leftover) ---
; Gmail often still wants a VMC before showing the logo. Do not invent a VMC.
default._bimi IN TXT "v=BIMI1; l=https://surmount.systems/bimi.svg;"

; --- MTA-STS (optional; requires https://mta-sts.surmount.systems/.well-known/mta-sts.txt) ---
_mta-sts IN TXT     "v=STSv1; id=2026073001"
mta-sts  IN A       203.0.113.10
mta-sts  IN AAAA    2001:db8::10

; --- DANE (only with DNSSEC; example shape -- compute real TLSA from cert) ---
; _25._tcp.mail IN TLSA 3 1 1 <hex sha256 of SPKI>
; See RFC 6698 / 7672; automate on renew.

; --- SRV (optional client hints) ---
_submission._tcp IN SRV 0 1 587 mail.surmount.systems.
_imaps._tcp      IN SRV 0 1 993 mail.surmount.systems.
_autodiscover._tcp IN SRV 0 1 443 services.surmount.systems.
```

### MTA-STS policy body (served over HTTPS)

```text
version: STSv1
mode: testing
mx: mail.surmount.systems
max_age: 86400
```

Move `mode` to `enforce` only after testing.

**Product edge:** management-ui can serve `GET /.well-known/mta-sts.txt`
when `surmount.managementUi.mtaStsMode` is `testing` or `enforce` (env
`SURMOUNT_MTA_STS_MODE`; default **off** => 404). Host must be
`mta-sts.<primaryDomain>` and on the redirect allowlist. Body uses
`mailHostname` as `mx` and `mtaStsMaxAge` (default 86400). Receivers fetch
the policy over **HTTPS** on that host. Enable **testing** only after
`mta-sts` is on the production Let's Encrypt leaf (laptop DNS-01; host ACME
off). Primary public MX is this host (2026-08-20). Stay **testing**.
Do **not** `enforce` until the operator asks. Dual-pin: [EDGE_AND_TLS.md](EDGE_AND_TLS.md),
[OPS.md](OPS.md).

**Certificate hostnames (not a storage area network):** intended
production leaf is **one** Let's Encrypt PEM pair (`with_single_cert`)
covering **20** names. Live leaf as of 2026-09-07 (LE production YE2,
laptop DNS-01, host ACME off, public CT) is still **18** names:
`baxterartworks.com`, `www.baxterartworks.com`, `btcfur.com`,
`www.btcfur.com`, `exophiles.org`, `www.exophiles.org`,
`iantuckerstudios.com`, `www.iantuckerstudios.com`, `nostrfurs.com`,
`www.nostrfurs.com`, `yiffa.app`, `www.yiffa.app`, `surmount.systems`,
`www.surmount.systems`, `mail.surmount.systems`,
`services.surmount.systems`, `mta-sts.surmount.systems`,
`mail.cryptoquick.com`.
The two names still missing from the live leaf are `cryptoquick.com` and
`www.cryptoquick.com`. That hostname mismatch is why cryptoquick HTTPS
fails verify. Validating A for those names succeeds (AD true). Leftover
parent DS key tag 2368 is gone. `mail.surmount.systems` and
`mail.cryptoquick.com` **are** on the live leaf (IMAP/SMTPS). Six extra
static zones are **live** HTTPS 200. After the tree lists the two missing
names, laptop `--issue` is operator-gated (Namecheap custody). `--live`
does **not** detect missing certificate hostnames. Do **not**
re-issue just to add a name already on that leaf. Esplora Hosts stay off
this leaf. Renew path: private host-profile `acme_domains` +
`just laptop-renew-cert -- --issue --directory production`
(Namecheap ClientIp = laptop egress). Extra-zone names need
`--hook` from `nix run .#acme-dns-hook-namecheap-dispatch`. Mail-plane apply:
`just point-stalwart-mail-tls`. Then restart `stalwart-mail` so File
PEMs re-read.

## PTR / rDNS

On the operator-chosen VPS provider, set reverse DNS for the primary IPv4
(and IPv6) to **`mail.surmount.systems`**. Forward A/AAAA and PTR should
match. Many receivers treat mismatched rDNS as spammy. This is one of the
highest leverage earn-trust fixes on a new VPS IP.

**SHC path (optional automation):** intake an operate-scoped customer API key
with `just secrets-prompt -- shc-api --host surmount-1`, then
`just rdns-shc -- --live --hostname mail.surmount.systems --ip YOUR_VPS_IP`
(default is dry-run). Provider console remains valid if you prefer not to use
the API. Namecheap does **not** set PTR. See the optional SHC rDNS tool table
above.

## Stalwart DKIM

Stalwart **0.16+** keeps DKIM as **DkimSignature** objects in the datastore
(JMAP WebUI / `stalwart-cli`), not as TOML on disk. Private key is PEM
(inline secret, env, or **File** path). Public key is server-derived; DNS TXT
is `selector._domainkey.<domain>`. Multi-signature dual algorithm is supported
(Ed25519 + RSA on the same domain with **different selectors**).

### Day-1 dual-sign convention (operator direction 2026-08-11)

| Item | Ed25519 | RSA-4096 |
|------|---------|----------|
| Selector | `stalwart` | `stalwart-rsa` |
| Algorithm / object | Ed25519-SHA256 (`Dkim1Ed25519Sha256`) | RSA-SHA256 (`Dkim1RsaSha256`) |
| Domain B private key | `/var/lib/surmount/secrets/mail/dkim/stalwart-ed25519.pem` (0600, `stalwart-mail`; PKCS#8 from `openssl genpkey -algorithm ed25519`) | `/var/lib/surmount/secrets/mail/dkim/stalwart-rsa4096.pem` (0600, `stalwart-mail`; `openssl genrsa 4096`) |
| DNS HostName | `stalwart._domainkey` | `stalwart-rsa._domainkey` |
| DNS TXT shape | `v=DKIM1; k=ed25519; p=...` | `v=DKIM1; k=rsa; p=...` |
| Engine register | `stalwart-cli create DkimSignature/Dkim1Ed25519Sha256` after **valid** admin token | `.../Dkim1RsaSha256` after same token |

RSA floor is **4096** (not 2048). Do **not** use RSA-8192 for DKIM: the
verifier MUST range in [RFC 8301](https://www.rfc-editor.org/rfc/rfc8301)
(accessed: 2026-08-11) is through 4096 bits; larger keys often break
verification. Zone tool: `set-txt` for each selector (serialize `--live`).

### Hardness and quantum honesty (mail auth)

Surmount hardens against every practical threat class, including long-term
quantum risk, without lying about what classical DKIM can do:

| Layer | Day-1 posture | Honesty |
|-------|---------------|---------|
| Classical DKIM | **Ed25519 + RSA-4096 dual-sign** (best algorithms receivers will accept today) | Receiver compatibility is why RSA exists next to Ed25519 |
| Post-quantum | **Neither** RSA-4096 nor Ed25519 is post-quantum | Larger RSA does **not** buy meaningful quantum resistance (Shor). Do not claim PQ from RSA-4096 |
| Residual | When IETF and major receivers support PQ (or hybrid) mail auth, Surmount tracks it | Separate from **D1** TLS hybrid KEX (`prefer-post-quantum`) and **D2** PQConnect |

Living dual-pin: [SECURITY.md](SECURITY.md), [OPS.md](OPS.md),
[RESIDUAL.md](../RESIDUAL.md) B6, [operator-direction.md](operator-direction.md)
(2026-08-11).

Host runbook and example apply plans (placeholders only in git):

- `/etc/surmount/stalwart/README-dkim-surmount-systems.txt` (when module installed)
- [`nix/stalwart/README-dkim-surmount-systems.txt`](../nix/stalwart/README-dkim-surmount-systems.txt)
- [`nix/stalwart/dkim-signature-ed25519-stalwart.example.ndjson`](../nix/stalwart/dkim-signature-ed25519-stalwart.example.ndjson)
- [`nix/stalwart/dkim-signature-rsa-stalwart.example.ndjson`](../nix/stalwart/dkim-signature-rsa-stalwart.example.ndjson)

Key generation and DNS `p=` encoding (public docs; accessed 2026-08-11):
[Stalwart DKIM signing](https://stalw.art/docs/mta/authentication/dkim/sign/).
Ed25519: raw 32-byte public key base64 for `p=` via
`openssl asn1parse ... -offset 12`. RSA: base64 of DER SubjectPublicKeyInfo
via `openssl rsa -pubout -outform der | openssl base64 -A`.

After Stalwart is up and a **Stalwart-accepted** admin token is in Domain B
(`just add-stalwart-token`; random generate is not registration):

1. Confirm **both** Domain B private key paths exist and match published DNS
   (do not invent a second key for a selector already live in DNS).
2. Register both File-type objects with the host driver (loopback :8080;
   token never logged). Default dry-run; live create is `--live`.
   Idempotent if selectors already exist.
   `just register-dkim` / `just register-dkim -- --live`
   Hermetic: `just test-register-dkim`.
3. Confirm `query DkimSignature` shows both `stage=active`. That is
   **sign-ready**, not outbound signed mail.
4. Confirm each DNS TXT matches the public key for that private key
   (`dig` on registrar NS for both selectors).
5. Send a test to mail-tester and confirm DKIM pass (after MX/rDNS as needed).
   Some receivers will show two `DKIM-Signature` headers when dual-sign is live.

Exact CLI field names follow current `stalwart-cli describe DkimSignature`
for the package pin. Prefer WebUI **Management > Domains > DKIM Signatures**
if you do not want the CLI path.

## mail-tester notes

- Score 10/10 is the goal; SPF + DKIM + DMARC + rDNS + content usually get
  you most of the way.
- Blacklist hits: check provider IP reputation; request delisting if the IP
  is a dirty recycled address (common on cheap VPS). Consider swapping IP.
- HTML marketing templates are a separate problem from infra.
- Re-run after a DMARC policy change and after cert/TLSA changes.

## Propagation and cutover

1. Lower TTL to 300s a day before cutover.
2. Publish SPF/DKIM/DMARC **before** or **as** MX flips.
3. Keep old MX reachable briefly if you need a fallback window.
4. Raise TTL again after stability.

## Additional / legacy domains

Split **mail domains** from **static-site-only vhosts**. The Nix list
`surmount.additionalDomains` is not the whole mail list: extra Stalwart
Domains can already exist for local delivery (`cryptoquick.com`,
`baxterartworks.com`) while that Nix list stays empty.

### Mail domains (send or receive on this host)

Same record classes as the primary. Dual DKIM TXT, not one selector.
Do not invent MX flips. Documentation addresses only.

**Live extra-domain facts (2026-08-20 mail records; 2026-09-07 A and leaf):**

| Apex | Mail records now | Public MX | Still remaining |
|------|------------------|-----------|-----------------|
| `cryptoquick.com` | Dual DKIM, TLS-RPT, CAA, SPF `a:mail.cryptoquick.com -all`, `_dmarc` `p=quarantine` (leave as-is). **Live 2026-09-07:** validating A for apex/www succeeds (AD true). Leftover parent DS key tag 2368 is gone. Extra MTA-STS wait. | Already `10 mail.cryptoquick.com` (Custom MX). | Live leaf still omits apex/www (18 names). Intended leaf is 20 names on one PEM. Laptop `--issue` after the tree lists those names is operator-gated (Namecheap custody). SHA-1 parent DS digest type 1 remains standing DNSSEC quality debt in operator-facts Monday leftover; it is not the HTTPS cause. Do not re-add 2368. Do not invent leftover Namecheap clicks. |
| `baxterartworks.com` | getHosts (11 records): dual DKIM (same `p=` as primary), TLS-RPT, CAA Let's Encrypt, SPF `a:mail.surmount.systems` plus eforward include `-all`. `_dmarc` **intended** **`p=quarantine`**. Do **not** restore getHosts `_dmarc` to `p=none`. Leftover `_acme-challenge` TXT still in getHosts. Extra MTA-STS not published. | EmailType **FWD**; public MX eforward1-5. Do **not** claim MX flipped. `--live set-mx` fail-closed while FWD. | Public NS republish/lag: `1.1.1.1` still NXDOMAIN for DKIM/`_dmarc`/TLS-RPT even when getHosts has the TXT (EmailType FWD). Empty CAA, old eforward-only SPF `~all`, SOA serial **1787245654** not bumped. Custom MX only if the operator asks. |

```dns
; Mail domain D (example shape; Baxter public MX is still forwarding)
; NS: Namecheap hosted DNS when we are the registrar.
; No leftover parent DS without matching DNSKEY.
D.              IN TXT   "v=spf1 a:mail.surmount.systems include:spf.efwd.registrar-servers.com -all"
stalwart._domainkey.D. IN TXT "v=DKIM1; k=ed25519; p=..."
stalwart-rsa._domainkey.D. IN TXT "v=DKIM1; k=rsa; p=..."
; DMARC: cryptoquick child is p=quarantine (leave as-is).
; Baxter intended _dmarc is p=quarantine (do not restore p=none).
_dmarc.D.       IN TXT   "v=DMARC1; p=quarantine; rua=mailto:admin@D.; pct=100"
_smtp._tls.D.   IN TXT   "v=TLSRPTv1; rua=mailto:admin@D."
; CAA issue + issuewild letsencrypt.org at apex
; _mta-sts / mta-sts A only if this cert and this host actually serve that name
; leave Baxter public MX on eforward until the operator asks
```

Do **not** use `p=reject`. Keep existing rua/ruf unless missing.
Mailbox `_dmarc.baxterartworks.com` intended is `p=quarantine`.
Public `_dmarc` may stay NXDOMAIN while EmailType is FWD even when
getHosts has the TXT.

Register engine dual-sign for that Stalwart Domain after DNS TXT:

```text
just register-dkim -- --domain D
just register-dkim -- --live --domain D
```

`--domain D` binds the Domain object whose `name` matches `D` (not the first
`id` in the query) and only counts File selectors `stalwart` / `stalwart-rsa`
on that domainId. Hermetic: `just test-register-dkim`. This recipe talks to
loopback :8080 only. It does not publish public TXT. Extra-domain File objects
may already exist from a prior host create; a correct dry-run then says
already present. Public DKIM TXT stays a separate Namecheap path.

Zone tool for extra SLD.TLD (laptop ClientIp):

```text
just dns-zone-namecheap -- --credentials ~/.local/share/surmount/namecheap/D.env -- list
just domain-audit -- D
```

### Static-site-only extra vhosts

Apex web hosting for legacy domains is **Axum Host -> document root**,
not nginx (nginx stays transitional-to-delete). See
[EDGE_AND_TLS.md](EDGE_AND_TLS.md) extra static Hosts. **Live HTTPS
(2026-08-20):** yiffa.app, baxterartworks.com, btcfur.com,
iantuckerstudios.com, nostrfurs.com, exophiles.org (each apex + www)
use the same public mail-host A/AAAA as surmount.systems apex/www,
are on the production leaf, and return HTTPS **200** packaged site
content. Those A/AAAA records are **not** mail-record hardening. A
static Host does **not** automatically get SPF, dual DKIM, DMARC,
TLS-RPT, CAA, or MTA-STS. `baxterartworks.com` is also a local Stalwart
Domain (getHosts already has dual DKIM / TLS-RPT / CAA / SPF; `_dmarc`
intended **`p=quarantine`**; public `_dmarc` may stay NXDOMAIN while
EmailType is FWD; public MX still eforward). Do not invent
mail on yiffa.app or other static-only extras. **Not live in browsers as
trusted HTTPS (2026-09-07):** `cryptoquick.com` and `www.cryptoquick.com`
fail verify because the live 18-name leaf omits them. Validating A
succeeds (AD true). Leftover parent DS key tag 2368 is gone. Host tree
is populated. Intended leaf is 20 names on one PEM. SHA-1 parent DS
digest type 1 remains standing DNSSEC quality debt in operator-facts
Monday leftover; it is not the HTTPS cause. Do **not** invent leftover
Namecheap clicks. Unowned names are not ours. Do not
Host-serve btcdragonlord.com, btckitties.com, denver.space, or
justsaybits.org.
Point `@` and `www` at those existing addresses (`just dns-zone-namecheap --
list` on the surmount.systems credentials, then `--credentials` for
the other zone). Do not point these names at the NAS. Do not invent
new VPS IPs into git. Namecheap ClientIp is **laptop egress**. Skip
uncertain or non-static names (Ghost, Grav, mail/git/ipfs* labels).

## Optional Namecheap zone tool (A/AAAA)

Day-1 manual publish at the registrar UI remains the supported path. When the
forward zone is at Namecheap managed DNS and Domain B already has
`/var/lib/surmount/secrets/acme/namecheap.env` (same file as the ACME DNS-01
hook; durable Domain B), you may merge edge A/AAAA without pasting into the UI:

```bash
# Dry-run (default): print planned zone merge; no setHosts
just dns-zone-namecheap -- list
just dns-zone-namecheap -- set-a services 203.0.113.10
just dns-zone-namecheap -- set-host mail --a 203.0.113.10 --aaaa 2001:db8::10
just dns-zone-namecheap -- set-txt 'default._bimi' 'v=BIMI1; l=https://surmount.systems/bimi.svg;'

# Apply only after dry-run looks right (TEST-NET examples above; use real VPS addresses)
just dns-zone-namecheap -- --live set-a services 203.0.113.10

# Other registered zones (one SLD/TLD per credentials file; laptop ClientIp):
just dns-zone-namecheap -- --credentials ~/.local/share/surmount/namecheap/cryptoquick.com.env -- list
just dns-zone-namecheap -- --credentials ~/.local/share/surmount/namecheap/cryptoquick.com.env -- set-host @ --a 203.0.113.10 --aaaa 2001:db8::10
just dns-zone-namecheap -- --credentials ~/.local/share/surmount/namecheap/cryptoquick.com.env -- set-host www --a 203.0.113.10 --aaaa 2001:db8::10
just dns-zone-namecheap -- --credentials ~/.local/share/surmount/namecheap/example.test.env -- delete-host @ --type URL
just dns-zone-namecheap -- --credentials ~/.local/share/surmount/namecheap/example.test.env -- delete-host www --type CNAME
```

Documentation addresses only in docs (RFC 5737 / 3849). Never commit real
API keys or production IPs. Credentials path convention matches
[OPS.md](OPS.md) Namecheap external-hook install. Lock note vs concurrent ACME
TXT is in the Namecheap section above.

Hermetic gate: `just test-dns-zone-namecheap` (also GHA quality; never live API).

## Related

- [Day-1 checklist](#day-1-dns-checklist-operator) (this file)
- [OPS.md](OPS.md) first boot / DNS / host profile ACME render
- [SECRETS.md](SECRETS.md) profile vs Secret Service vs Vaultwarden
- [deploy-host-local.md](deploy-host-local.md) B6 / host-local-acme.nix
- [EDGE_AND_TLS.md](EDGE_AND_TLS.md)
- [research/tls-trust-and-acme.md](research/tls-trust-and-acme.md)
- [MIGRATION.md](MIGRATION.md)
- [operator-direction.md](operator-direction.md)
- [open-choices.md](open-choices.md) (DNS hosting; open)
