Laptop Let's Encrypt renew timer (not a VPS ACME timer)

Host in-process ACME stays off. Namecheap ClientIp is laptop egress.
Do not copy Namecheap credentials to the VPS.

Preferred install (writes filled units with this tree's script path):

  just laptop-renew-cert -- --install-timer --directory production \
    --host-profile ~/.local/share/surmount/host-profile.toml \
    --target root@YOUR_HOST

That writes to ~/.config/systemd/user/ unless you pass --timer-dir.
--install-timer without --target is BLOCKED (weekly --live cannot install
PEMs without a destination). Then on the laptop:

  systemctl --user daemon-reload
  systemctl --user enable --now surmount-laptop-renew-cert.timer

The timer runs --live --directory production --target ... (and
--host-profile when you passed one). --live issues only when
the staged leaf is missing or inside the early-renew window (default 30
days). Otherwise it restages the matching pair, installs tls-cert (0640)
and tls-key (0600 owner-only; also copies mail/tls for Stalwart),
restarts the UI, and proves health plus IMAP/SMTPS.

Safe anytime:

  just laptop-renew-cert -- --check --directory production

These example units in this directory are comments plus a just wrapper.
Prefer --install-timer over copying them by hand.
