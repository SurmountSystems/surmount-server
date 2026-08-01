# DNS checklist for surmount.systems mail

Correct DNS matters as much as the MTA. Work top to bottom before sending
real user mail. Validate with [mail-tester.com](https://www.mail-tester.com/)
and your provider's blocklist checks.

**Last updated:** 2026-07-30

Living companions: [EDGE_AND_TLS.md](EDGE_AND_TLS.md),
[research/tls-trust-and-acme.md](research/tls-trust-and-acme.md),
[operator-direction.md](operator-direction.md), [OPS.md](OPS.md).

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
- [ ] **DKIM** key in Stalwart; public TXT `selector._domainkey` published
- [ ] **DMARC** TXT `_dmarc` with rua; start `p=none` or `quarantine`, move to
      `reject` after clean reports
- [ ] SPF + DKIM **align** with the visible From domain (DMARC alignment)
- [ ] Same auth stack for every `surmount.additionalDomains` entry

### C. SMTP transport trust

- [ ] Mail certs valid on 465/993 and STARTTLS paths (shared ACME PEMs or chosen path)
- [ ] **TLS-RPT** TXT `_smtp._tls` so failures are visible
- [ ] **MTA-STS** TXT `_mta-sts` + HTTPS policy host (`mode: testing` then `enforce`)
- [ ] Prefer TLS 1.3 posture on related HTTPS policy host (edge)
- [ ] **DNSSEC** signed zone when registrar/DNS host allows (prerequisite for DANE)
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
| **SPF** | TXT on apex (and each additional domain) | Authorize sending IPs | Auth |
| **DKIM** | TXT `selector._domainkey` | Public key from Stalwart | Auth |
| **DMARC** | TXT `_dmarc` | Start at `p=none` or `quarantine`, then reject | Policy |
| **MTA-STS** | TXT `_mta-sts` + HTTPS policy host | SMTP STS policy | Transport |
| **TLS-RPT** | TXT `_smtp._tls` | TLS failure reports | Transport visibility |
| **DNSSEC** | Zone signing at DNS host | Chain of trust for DNS | Enables DANE |
| **TLSA** | `_25._tcp.mail` (and related) | DANE for SMTP | Transport auth without sole WebPKI |
| **SRV** | `_submission`, `_imaps`, `_autodiscover` | Client autoconfig (optional) | UX |
| **Autodiscover / autoconfig** | CNAME or A + HTTPS | Client setup (optional) | UX |

Also publish MX/SPF/DKIM/DMARC for every entry in
`surmount.additionalDomains`.

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

; --- DMARC (start monitoring, then tighten) ---
_dmarc  IN  TXT     "v=DMARC1; p=quarantine; rua=mailto:dmarc@surmount.systems; ruf=mailto:dmarc@surmount.systems; fo=1; pct=100"

; --- DKIM (example selector "stalwart"; get real record from Stalwart) ---
; stalwart._domainkey IN TXT "v=DKIM1; k=rsa; p=MIIBIjANBg..."

; --- TLS reporting ---
_smtp._tls IN TXT   "v=TLSRPTv1; rua=mailto:tlsrpt@surmount.systems"

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

## PTR / rDNS

On the operator-chosen VPS provider, set reverse DNS for the primary IPv4
(and IPv6) to **`mail.surmount.systems`**. Forward A/AAAA and PTR should
match. Many receivers treat mismatched rDNS as spammy. This is one of the
highest leverage earn-trust fixes on a new VPS IP.

## Stalwart DKIM

After Stalwart is up:

1. Generate or enable a DKIM key for `surmount.systems` in admin / config.
2. Publish the TXT record Stalwart shows.
3. Send a test to mail-tester and confirm DKIM pass.

Exact CLI varies by Stalwart version; prefer the admin UI or current
`stalwart-cli` docs for your package.

## mail-tester notes

- Score 10/10 is the goal; SPF + DKIM + DMARC + rDNS + content usually get
  you most of the way.
- Blacklist hits: check provider IP reputation; request delisting if the IP
  is a dirty recycled address (common on cheap VPS). Consider swapping IP.
- HTML marketing templates are a separate problem from infra.
- Re-run after DMARC policy tighten and after cert/TLSA changes.

## Propagation and cutover

1. Lower TTL to 300s a day before cutover.
2. Publish SPF/DKIM/DMARC **before** or **as** MX flips.
3. Keep old MX reachable briefly if you need a fallback window.
4. Raise TTL again after stability.

## Additional / legacy domains

For each domain `D` in `surmount.additionalDomains`:

```dns
D.              IN MX 10 mail.surmount.systems.
D.              IN TXT   "v=spf1 mx a:mail.surmount.systems -all"
_dmarc.D.       IN TXT   "v=DMARC1; p=quarantine; rua=mailto:dmarc@surmount.systems"
; + DKIM selector for D
; + TLS-RPT / MTA-STS / DANE as you roll them out per domain policy
```

Apex web hosting for legacy domains can be separate edge vhosts
(nginx transitional-to-delete today; Axum-first edge target; see EDGE_AND_TLS.md)
via `surmount.web.extraVhosts`.

## Related

- [EDGE_AND_TLS.md](EDGE_AND_TLS.md)
- [research/tls-trust-and-acme.md](research/tls-trust-and-acme.md)
- [OPS.md](OPS.md)
- [MIGRATION.md](MIGRATION.md)
- [operator-direction.md](operator-direction.md)
