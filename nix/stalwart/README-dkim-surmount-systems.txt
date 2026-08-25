Stalwart DKIM signing for surmount.systems (Day-1 dual-sign mail auth)

Product truth (Stalwart 0.16+):
  - DKIM private keys live as DkimSignature objects in the RocksDB datastore
    (JMAP), OR as a SecretText File path that the service can read.
  - Public key is server-set on the object (publicKey PEM) after create.
  - DNS TXT is operator-published at selector._domainkey.<domain>.
  - This tree does not auto-apply DKIM into Stalwart without a valid admin
    token (stalwart-cli create / apply). generate != engine registration.
  - Day-1: **dual-sign** Ed25519-SHA256 + RSA-SHA256 (4096-bit RSA floor).
    Two selectors, two DkimSignature objects, both stage=active when token ok.
  - RSA-2048 is not enough for Surmount hardness. RSA-8192 is out of scope
    (RFC 8301 verifier MUST range is through 4096; larger often breaks).
  - Neither Ed25519 nor RSA-4096 is post-quantum. Larger RSA does not buy
    meaningful quantum resistance (Shor). See docs/DNS.md hardness note.

Selectors and Domain B durable private keys:

  Selector:   stalwart
  Algorithm:  Ed25519-SHA256 (Dkim1Ed25519Sha256)
  Path:       /var/lib/surmount/secrets/mail/dkim/stalwart-ed25519.pem
  owner/mode: stalwart-mail:stalwart-mail  0600
  PEM:        PKCS#8 from `openssl genpkey -algorithm ed25519`

  Selector:   stalwart-rsa
  Algorithm:  RSA-SHA256 (Dkim1RsaSha256), RSA-4096 private key
  Path:       /var/lib/surmount/secrets/mail/dkim/stalwart-rsa4096.pem
  owner/mode: stalwart-mail:stalwart-mail  0600
  PEM:        `openssl genrsa 4096` (PKCS#1 RSA PRIVATE KEY is fine)

Generate Ed25519 (laptop or host; private key never in git):

  openssl genpkey -algorithm ed25519 -out stalwart-ed25519.pem
  openssl pkey -in stalwart-ed25519.pem -pubout -out stalwart-ed25519.pub.pem
  # DKIM DNS p= is the raw 32-byte public key, base64 (Stalwart docs):
  openssl asn1parse -in stalwart-ed25519.pub.pem -offset 12 -noout -out /dev/stdout \
    | openssl base64 -A
  # TXT: v=DKIM1; k=ed25519; p=<that base64>

Generate RSA-4096 (laptop or host; private key never in git):

  openssl genrsa -out stalwart-rsa4096.pem 4096
  openssl rsa -in stalwart-rsa4096.pem -pubout -out stalwart-rsa4096.pub.pem
  # DKIM DNS p= is base64 of DER SubjectPublicKeyInfo:
  openssl rsa -in stalwart-rsa4096.pem -pubout -outform der 2>/dev/null \
    | openssl base64 -A
  # TXT: v=DKIM1; k=rsa; p=<that base64>

Public DNS (Namecheap forward zone; not this file):

  HostName: stalwart._domainkey
  Type: TXT
  Value: v=DKIM1; k=ed25519; p=<base64 raw Ed25519 public>

  HostName: stalwart-rsa._domainkey
  Type: TXT
  Value: v=DKIM1; k=rsa; p=<base64 RSA SPKI>

  Tool: script/dns-zone-namecheap.sh set-txt (dry-run default; --live apply)
  Same credentials as ACME DNS-01 (kind namecheap-api).
  Serialize --live setHosts (do not wipe zone with concurrent ACME).

SPF / DMARC (also Namecheap forward TXT; rDNS stays VPS provider):
  SPF apex:  v=spf1 a:mail.surmount.systems include:spf.efwd.registrar-servers.com -all
             (a:mail covers this VPS; include keeps Namecheap email-forward
              while EmailType=FWD / eforward MX still in use)
  DMARC:     _dmarc TXT  v=DMARC1; p=quarantine; rua=mailto:admin@surmount.systems; pct=100
             (live mailbox policy is p=quarantine; do not use p=reject)

Register BOTH signatures in Stalwart (requires engine-accepted admin token):

  # Preferred: host driver (token from Domain B file; loopback :8080 only).
  # Default dry-run. Live create: just register-dkim -- --live
  # Preflight: token, both PEMs, Domain name, sandbox ReadOnlyPaths grant.
  # Idempotent if selectors already exist. Hermetic: just test-register-dkim
  just register-dkim
  just register-dkim -- --live

  export STALWART_URL=http://127.0.0.1:8080
  # STALWART_TOKEN or basic admin from Domain B; never invent a random token.
  # free-443 401 means Domain B value is wrong or unknown.

  # 1) Find Domain id for surmount.systems
  stalwart-cli query Domain --json

  # 2) Create Ed25519 signature (edit domainId):
  stalwart-cli create DkimSignature/Dkim1Ed25519Sha256 \
    --field 'domainId=<Domain-id-from-query>' \
    --field selector=stalwart \
    --field 'privateKey={"@type":"File","filePath":"/var/lib/surmount/secrets/mail/dkim/stalwart-ed25519.pem"}' \
    --field stage=active

  # 3) Create RSA-4096 signature (separate selector):
  stalwart-cli create DkimSignature/Dkim1RsaSha256 \
    --field 'domainId=<Domain-id-from-query>' \
    --field selector=stalwart-rsa \
    --field 'privateKey={"@type":"File","filePath":"/var/lib/surmount/secrets/mail/dkim/stalwart-rsa4096.pem"}' \
    --field stage=active

  # Or apply edited host-local copies of:
  #   dkim-signature-ed25519-stalwart.example.ndjson
  #   dkim-signature-rsa-stalwart.example.ndjson
  # (replace DOMAIN_ID_PLACEHOLDER). Never commit real domain ids if sensitive.

  # 4) Confirm both objects have publicKey; DNS p= must match each key
  stalwart-cli query DkimSignature --json

  # 5) Send a test (mail-tester) after MX/rDNS are ready; confirm DKIM pass
  #    (receivers that accept both may show two DKIM-Signature headers)

Do not regenerate a private key after its DNS is published unless you rotate
the selector or update the TXT in the same window.

Hardness / quantum honesty (short):
  Classical Day-1 best available under receiver DKIM algorithm acceptance:
  Ed25519 + RSA-4096 dual-sign. Not post-quantum. When IETF/receivers support
  PQ mail auth (or hybrid), Surmount tracks it separately from TLS hybrid KEX
  and PQConnect. Living: docs/DNS.md, docs/SECURITY.md, RESIDUAL.md B6.

Upstream: https://stalw.art/docs/mta/authentication/dkim/sign/ (accessed: 2026-08-11)
RFC 8301 (DKIM crypto update; RSA key sizes): https://www.rfc-editor.org/rfc/rfc8301
  (accessed: 2026-08-11)
Docs: docs/DNS.md (Stalwart DKIM + zone tool set-txt), docs/OPS.md, docs/SECRETS.md
