Stalwart mail-plane TLS (IMAP :993 / SMTPS :465)

Product truth (Stalwart 0.16+):
  - Nix cannot pin IMAP/SMTP cert files. services.stalwart.settings is
    accepted and ignored. Listeners and TLS live as JMAP objects.
  - First boot with an empty store inserts an engine self-signed leaf
    (rcgen identity). Evolution will warn until a Certificate object
    points at real files.
  - Surmount points Stalwart at copies of the durable Let's Encrypt PEMs
    under secrets/mail/tls. Axum keeps secrets/tls with key mode 0600.
    Host in-process ACME stays off. Laptop DNS-01 issues the leaf
    (Namecheap; ClientIp = laptop egress).
  - The leaf must name mail.<apex> on the certificate hostname list
    (private host-profile acme_domains + just laptop-renew-cert
    -- --directory production). Adding a File object without that name
    still fails Evolution hostname checks.

Durable Domain B paths:
  Axum cert:     /var/lib/surmount/secrets/tls/cert.pem
                 0640  owner surmount-ui  group surmount-tls
  Axum key:      /var/lib/surmount/secrets/tls/key.pem
                 0600  owner surmount-ui (owner-only)
  Mail cert:     /var/lib/surmount/secrets/mail/tls/cert.pem
                 0640  owner stalwart-mail
  Mail key:      /var/lib/surmount/secrets/mail/tls/key.pem
                 0600  owner stalwart-mail (owner-only)
  Dir (mail):    0750  stalwart-mail:stalwart-mail
  Do not chmod world-readable. Do not put PEM bodies in Nix.

Sandbox:
  ProtectSystem=strict. modules/mail.nix grants ReadOnlyPaths
  /var/lib/surmount/secrets/mail/tls (and still grants secrets/tls).

Apply (requires engine-accepted admin token; loopback :8080 only):

  just point-stalwart-mail-tls
  just point-stalwart-mail-tls -- --live --restart

  # Manual equivalent after query:
  #   stalwart-cli query Certificate --json
  #   stalwart-cli create Certificate \
  #     --field 'certificate={"@type":"File","filePath":"/var/lib/surmount/secrets/mail/tls/cert.pem"}' \
  #     --field 'privateKey={"@type":"File","filePath":"/var/lib/surmount/secrets/mail/tls/key.pem"}'
  #   Stalwart 0.16.15 query JSON is certificate hostnames plus id, not
  #   filePath. Use the Let's Encrypt File object id (not first-boot rcgen).
  #   stalwart-cli update SystemSettings --field defaultCertificateId=<file-id>
  #   systemctl restart stalwart-mail

Default dry-run. Hermetic: just test-point-stalwart-mail-tls
Do not auto-apply at Nix activation (same as free-443 / DKIM).

Proof after live apply (names only; no IPs, no PEM bodies):
  ./scripts/check-tls.sh mail.surmount.systems:993 mail.surmount.systems
  openssl s_client -connect mail.surmount.systems:993 -servername mail.surmount.systems
  Subject / issuer / notAfter / certificate hostnames. Issuer must not be
  rcgen self signed cert. Hostname list must include mail.surmount.systems.

Upstream:
  https://stalw.art/docs/server/tls/certificates/ (accessed: 2026-08-17)
  https://stalw.art/docs/ref/object/certificate (accessed: 2026-08-17)
  https://stalw.art/docs/ref/object/system-settings (accessed: 2026-08-17)
Docs: docs/EDGE_AND_TLS.md, docs/OPS.md, docs/DNS.md
