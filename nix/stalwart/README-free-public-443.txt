Surmount P1: free public :443 for Axum product edge
=====================================================

Product rule (operator lock):
  - management-ui (Axum) owns clearnet public :80 and :443
  - Stalwart is backend mail (25/465/587/993/4190 as used)
  - Stalwart must NOT permanently own product clearnet HTTPS on :443

First-boot honesty:
  Empty Stalwart store may insert upstream defaults including HTTPS :443
  and HTTP :8080. That is engine bootstrap, not Surmount product edge.
  Free :443 before enabling managementUi listenMode=https on public bind.

Preferred driver (Domain B token; never logs values):
  script/free-stalwart-public-443.sh --dry-run
  script/free-stalwart-public-443.sh --live --restart
  Token default: /var/lib/surmount/secrets/ui/stalwart-api-token
  Installing the token file is not engine registration (first-boot admin may
  still be required once).

Manual steps (operator; host credentials never in git):
  1. export STALWART_URL=http://127.0.0.1:8080
     (or SSH tunnel to the first-boot management port)
  2. stalwart-cli query NetworkListener --fields id,name,protocol,bind --json
  3. Confirm first-boot HTTPS listener name (template uses name=https).
     If names differ, copy this plan to a host-local file and adjust filters.
  4. stalwart-cli apply --file /etc/surmount/stalwart/free-public-443-for-axum-edge.ndjson --dry-run
  5. stalwart-cli apply --file /etc/surmount/stalwart/free-public-443-for-axum-edge.ndjson
  6. Restart stalwart-mail if binds persist until process restart
  7. ss -lntp | check :443 free for product edge
  8. Enable managementUi https + PEMs (or ACME) per hosts/mail-vps comments

Firewall note:
  modules/networking.nix opens :80/:443 for product edge (Axum or dual-run
  nginx). services.stalwart.openFirewall does not open 443. Prefer leave
  openFirewall false and keep Surmount networking.nix as the port SoT.

Mail TLS note:
  IMAPS/SMTPS cert files on mail ports are separate from browser HTTPS.
  Do not re-bind Stalwart as the product public HTTPS frontend.

Docs: docs/EDGE_AND_TLS.md, docs/OPS.md, nix/stalwart/README.md
