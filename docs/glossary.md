# Glossary (Surmount Server)

Plain American English terms used in this repo. No OSI-layer theater. If a
term is only jargon here, it belongs in this file.

**Last updated:** 2026-07-30

---

## A

**ACME**
Automated Certificate Management Environment. How we get TLS certificates
(usually Let's Encrypt) without manual cert files every 90 days.

**Activation (NixOS)**
The step when a built system generation is applied on the host
(`nixos-rebuild switch` and friends). Deploy secrets must be decryptable
here so services can start.

**Apply plan / apply (Stalwart)**
Day-2 change to Stalwart settings via `stalwart-cli apply` (or WebUI),
writing JMAP config objects into the datastore. Distinct from NixOS
activation. Stalwart 0.16+ keeps most runtime config in the store, not in
the small on-disk `config.json`.

**Arti**
Tor Project's **Rust** implementation of Tor protocols
(https://gitlab.torproject.org/tpo/core/arti/). Surmount uses Arti for
**required onion/hidden service** reachability of product services
(first-class next to clearnet). Not a clearnet HTTPS edge replacement. Not
Cloudflare. HS identity keys are deploy secrets (**never in git**). Research:
[research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md).
Pin: [COMPACTION-PIN.md](COMPACTION-PIN.md) section 7.

**Axum**
Rust HTTP framework used as the Surmount management / product shell.

---

## B

**Blob store (Stalwart)**
Where large binary objects live (message bodies, attachments), content-
addressed (BLAKE3). Can be the same physical engine as the data store
(Default) or separate (filesystem, S3, etc.).

**blobSize (Stalwart RocksDB knob)**
Minimum size in bytes for an object to go to the blob path rather than
inline with metadata. Upstream default **16834**. Larger values keep more
small objects in the data path; smaller values push more into blob files.

**bufferSize (Stalwart RocksDB knob)**
In-memory write buffer size in bytes for RocksDB. Upstream default
**134217728** (128 MiB). Larger can help write throughput; costs RAM.

---

## C

**config.json (Stalwart 0.16+)**
Tiny on-disk file that only describes the **DataStore** (type + path and
optional RocksDB knobs). Not a full mail config dump.

**Co-location (stores)**
Running data, blob, FTS, and in-memory/lookup roles on one physical engine
(here: one RocksDB directory). Fine for current Surmount phase per operator
direction 2026-07-30.

---

## D

**Data store (Stalwart)**
Metadata, folders, headers/refs, domains, most config objects.

**DataStore**
Stalwart 0.16 JSON object name for the configured data backend
(`@type` = `RocksDb`, `Sqlite`, etc.).

**Day-1**
First install and bring-up: disk layout, OS install, flake first switch,
DNS, first certs, empty or imported mail, basic hardening. "Get it
running."

**Day-2**
Operations after first install: rebuilds, secret rotation, backups,
account changes, Stalwart apply plans, upgrades, incident response.
"Keep it running and change it safely."

**Deploy secrets**
Secrets that must exist on disk **before/during** `nixos-rebuild switch`
so systemd can start services (mail admin seed, restic password,
Vaultwarden admin token, session keys, host age/sops key material).
Bucket 1 in [SECRETS.md](SECRETS.md). Scaffold tool today: **sops-nix**
(need is real; tool can change). **Not** Vaultwarden and **not** LUKS.
Material lives **on the host** / operator channels; **never in public git**
(plain or ciphertext). Prefer this phrase over vague "seal" jargon.
sops-nix decrypts at **activation** after root is up; it does **not**
unlock LUKS2 in initrd. See [hygiene.md](hygiene.md) top rule and
[research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md).

---

## E

**Edge**
The process that terminates public HTTPS (and usually handles cert obtain
or loads cert files) and routes to local apps. Prefer **first-party Axum**
(in-process or small same-workspace crate). Must not be nginx long-term
(operator direction). See [EDGE_AND_TLS.md](EDGE_AND_TLS.md).

**External FTS**
Full-text search engine outside Stalwart's internal index (Elasticsearch,
Meilisearch, etc.). Not required now; Surmount may build its own search
product later.

---

## F

**Facta Non Verba**
Latin: "deeds, not words." Cultural root of the Fix / FixOS names. Prefer
shipping working hosts over ceremony.

**FDE**
Full Disk Encryption. On this project, prefer **LUKS2** on the VPS when
install allows.

**Fix**
Surmount's take on, or fork lineage of, **upstream Nix** (package manager /
evaluator story). Open working direction; see [fix-and-fixos.md](fix-and-fixos.md).

**FixOS**
Surmount's take on, or fork lineage of, **upstream NixOS** (modules,
images, installer defaults).

**fixpkgs**
Future shared Surmount package channel/set (name for "our packages," not a
shipping product today). Today we use a **Surmount package overlay** inside
this repo instead.

**FOD (Fixed-Output Derivation)**
**Nix term only.** A derivation whose output hash is known in advance, so
Nix can fetch or build with a fixed result (release tarballs, spam-filter
rules zip, etc.). **Not** "foreign object debris." Hermetic flakes use FODs
so builds do not depend on a live unhashed network at eval time beyond the
fixed hash.

**FTS**
Full-text search. **Internal FTS** means Stalwart's built-in search index
in the search store role (Default = same RocksDB as data for us now).

---

## H

**Hermetic**
Build/eval does not depend on ambient laptop state, unpinned checkouts, or
unhashed network. Locked inputs + FODs + host-side secret decrypt.

**Hidden service / onion service (HS)**
Tor service published so clients reach it as a `*.onion` address without
needing a public clearnet listener for that path. Surmount **requires** Arti
HS reachability for services (alongside clearnet where clearnet applies). See
**Arti**.

---

## I

**In-memory store (Stalwart)**
Soft KV used for rate limits, greylist, locks, short-lived tokens. Can sit
on the same RocksDB (Default). Operator direction: fine on RocksDB for now.

**Internal directory (Stalwart)**
Built-in principal/credential store (mail accounts), not a separate SQL
directory product for v1.

**Internal FTS**
See FTS. Good enough for now; Surmount owns a future search product later.

---

## J

**JMAP**
JSON Meta Application Protocol. How modern clients and our product should
talk to Stalwart for mail (preferred over scraping RocksDB).

---

## L

**Leptos SSR**
Rust UI framework with server-side rendering. Preferred Surmount web UI
path (admin first, then webmail).

**LUKS2**
Linux block-device full disk encryption format. Disk-at-rest layer (not
deploy secrets, not Vaultwarden). Unlock happens in **initrd** (passphrase,
initrd SSH, TPM, Tang/Clevis; host-local keyfile only if weaker unattended
path is accepted) **before** deploy-secret activation. **Never** put unlock
material in git. sops-nix does not configure or unlock LUKS2. Research:
[research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md).

---

## N

**nixpkgs**
Community Nix package set. We say **upstream nixpkgs** when contrasting
with Surmount-owned packages.

**Nostr**
Simple public-key identity network. Surmount product auth uses Nostr keys
(npub public, nsec private on client). Keys managed via host OS / other
Surmount tools; not a manual password farm for users.

**npub / nsec**
Nostr public / private key encodings. **nsec never stored on the mail
server** as a convenience login secret.

---

## O

**Operator direction**
What the operator stated as working product direction on a date. Dated in
[operator-direction.md](operator-direction.md). Not automatically eternal
law; still stronger than scaffold defaults when they conflict.

**Onion service**
See **Hidden service / onion service (HS)**.

**Overlay (Nix)**
Function that modifies or adds packages on top of upstream nixpkgs. Our
Surmount packages are exposed this way.

---

## P

**poolWorkers (Stalwart RocksDB knob)**
Worker threads for database operations. Upstream default: **number of
logical CPUs** on the host. Leave default unless measured need to pin.
Do not invent a public core count for the VPS.

**PTR / rDNS**
Reverse DNS for the sending IP. Needed for decent mail deliverability.

---

## R

**RocksDB**
Embeddable LSM key-value store. Stalwart's default single-node engine.
Surmount: **fine for now** for all store roles.

**RPO / RTO**
Recovery Point Objective / Recovery Time Objective. How much data you can
lose and how fast you must be back. Clever backup design around these is
**later, not now** (operator direction 2026-07-30).

**restic**
Backup tool wired as an opt-in module. Not a substitute for offline age
key and LUKS recovery material.

---

## S

**Scaffold default**
Wired so the host boots or evals green; provisional until operator chooses
or directs otherwise.

**Search store (Stalwart)**
Holds full-text indexes. Default = internal FTS on the data engine.

**sops-nix**
NixOS integration for [sops](https://github.com/getsops/sops): decrypt
deploy secrets **on the host** at activation with age (or pgp). Current
scaffold tool for **deploy secrets**. Surmount rule: secret material is
**not** stored in the public git tree (plain or ciphertext); host-local /
out-of-band only. **Not** a human password manager. **Not** LUKS unlock
(no full initrd secrets support; activation is after root). The activation
*need* remains if the tool changes. [SECRETS.md](SECRETS.md),
[hygiene.md](hygiene.md).

**PQConnect**
Application-independent post-quantum path encryption between hosts that
both run it (https://www.pqconnect.net). Complements TLS; does not replace
Axum HTTPS or Stalwart mail TLS by itself. Not in nixpkgs as of 2026-07-30
check. Research: [research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md).

**Stalwart**
Mail and collaboration server (SMTP/IMAP/JMAP/Sieve, spam path, stores).
Surmount may consume a **SurmountSystems/stalwart** fork when patches are
needed. See [packages-and-forks.md](packages-and-forks.md).

**Surmount package overlay**
Packages owned in this repo (`nix/packages/`) applied over upstream
nixpkgs so we can pin current engines without waiting on the channel.

---

## U

**UDS (Unix domain socket)**
Filesystem socket path for local IPC (for example `/run/surmount/ui.sock`).
Preferred between local services instead of TCP localhost.

**upstream Nix / upstream NixOS / upstream nixpkgs**
Community projects, as opposed to Fix / FixOS / Surmount packages.

---

## V

**Vaultwarden**
Unofficial Bitwarden-compatible server
(https://github.com/dani-garcia/vaultwarden/). Human/org secret vault
(passwords, TOTP, notes, mail-related secrets operators manage in the
vault product). Bucket 2 in SECRETS.md. Does **not** replace encrypted
deploy secrets for NixOS activation.

**VPS**
Virtual private server. Surmount mail host is an **operator-chosen VPS**
(size/plan open; **Q-HOST-1**; do not invent RAM/disk/core/SKU numbers). Not a
default "must be Hetzner" preference.

---

## Question ids

Open questions in docs use **unique global ids** so they never collide with
section numbers or other lists. Prefer namespaced forms when helpful
(**Q-HOST-1**, **Q-TLS-1**, **Q-EDGE-1**) or a single **Q1, Q2, ...**
sequence **per document** without recycling bare "1." in mixed replies.
