# Surmount Server - agent notes

Short process rules for anyone (human or agent) editing this repo.
Product detail lives under `docs/`. This file is standing law only.

## Language

- **No ADR jargon.** Do not write "ADR", "ADR-001", or "architecture decision
  record." Use plain American English: open choices, working notes, proposed,
  scaffold default, research finding, operator direction YYYY-MM-DD.
- **Fix / FixOS naming:** when meaning Surmount-owned Nix or NixOS lineage,
  say **Fix** (Nix take/fork) or **FixOS** (NixOS take/fork). When meaning
  community projects, say **upstream Nix**, **upstream NixOS**, or **nixpkgs**.
  Plain packaging names: **upstream nixpkgs**, **Surmount package overlay**,
  **future fixpkgs channel**. Ladder: [docs/fix-and-fixos.md](docs/fix-and-fixos.md).
  Not operator-accepted product law unless they say so in writing.
- **FOD** means Nix Fixed-Output Derivation only (see
  [docs/glossary.md](docs/glossary.md)). Not "foreign object debris."
- Open questions in docs: global **Q1, Q2, ...** (never collide with section
  numbers).
- Undecided design: [docs/open-choices.md](docs/open-choices.md).
- Dated operator direction: [docs/operator-direction.md](docs/operator-direction.md).
- **After compaction:** [docs/COMPACTION-PIN.md](docs/COMPACTION-PIN.md) first.
- Living maps: [docs/STACK.md](docs/STACK.md), [docs/hygiene.md](docs/hygiene.md),
  [docs/principles.md](docs/principles.md), [docs/fix-and-fixos.md](docs/fix-and-fixos.md).
- Deep notes: [docs/research/](docs/research/). Parent docs link down to
  research; research does not replace living docs.
- ASCII only in docs we own; no em dashes.
- **DNSSEC parent record, not bare "DS".** Operator pin 2026-08-20: in this
  house **DS** means **DiskStation** (DS1513, DS3018xs). Do **not** use bare
  **DS** for DNSSEC. Say **parent DNSSEC record**, or write it out once:
  Delegation Signer (the DNSSEC parent record, not DiskStation), then use
  plain words. Same collision class as bare SAN vs storage area network.
- **Certificate hostnames, not bare "SAN".** Operator pin 2026-08-11:
  do **not** use bare **SAN** / multi-SAN for TLS certificate name lists.
  In datacenters **SAN** means storage area network; that collision confuses
  humans. Say **names on the certificate**, **certificate hostnames**, or
  **which hostnames this cert covers**. If the wire term is needed once,
  write it out: Subject Alternative Name (the cert hostname list), then use
  plain words.
- **Production public HTTPS = Let's Encrypt production.** Operator has been
  clear: production uses **Let's Encrypt** (the real, browser-trusted
  directory). Do **not** talk as if production certs might be some other CA
  by default. **Let's Encrypt staging** is only a temporary test directory
  (not trusted by normal browsers). Live residual is: move from LE **staging**
  (test) to LE **production** (real), and cover the hostnames we need, not
  "maybe invent a non-LE path."
- **Namecheap ClientIp = who calls the API, not where PEMs live.** Operator
  pin 2026-08-11: Domain A / laptop custody of Namecheap API credentials means
  DNS-01 and zone tools run from the **laptop**. Namecheap must whitelist the
  **laptop egress**, not the VPS. Do **not** demand VPS whitelist as a default
  gate when the operator already put the API token local and issuance is
  laptop-driven. Host PEMs are installed to the VPS after issue. VPS whitelist
  is only required if **host in-process ACME** (or any process on the VPS) hits
  Namecheap. Do not mix: laptop ClientIp on the VPS env file, or host ACME with
  laptop-only whitelist.
- **Operator is the admin.** Operator pin 2026-08-14: do not tell the
  operator they are "not the admin" when explaining Stalwart CLI. The verb
  `update AccountPassword` updates the principal bound to `STALWART_TOKEN`
  (the host API token), not a mailbox account. Say **API token principal**
  vs **mailbox account**. Never "you are not the admin." Mailbox password
  setup belongs in the services console, not an SSH ritual.
- **Enumerate operator residual every status (operator 2026-08-22).** When
  you report what is done, also list leftover operator work in numbered
  complete sentences (password still owed, client still empty, parked
  Monday work). Do not hide leftovers in prose. Living mailbox maps stay
  in `~/.agents/surmount-server/operator-facts.md`, not this file.
  One-time "remind me next time we talk" leftovers go in that facts file
  and the remaining-work pointer. Do **not** start a recurring scheduler
  loop for that.
- **Recommendations need reasons and sources (operator 2026-08-24).** When
  you tell the operator to tap, type, or change a setting, say **why** in
  complete sentences and cite a public page or a live measurement from
  this host (IMAP NAMESPACE, cert names, JMAP counts). Do **not** default
  to "delete the account." If iOS says the IMAP server named in quotes is
  not responding, they **did** enter that host. Do **not** invent a cause
  without matching the screen they showed.
- **Public repo is process law (Kerckhoffs, operator 2026-08-21).** This
  tree is meant to be public. Put **process** here: how agents work, what
  must never land in git, stack rules, Kerckhoffs-style design that stays
  safe when the design is known. Put **project-specific operator facts**
  (living mailbox maps, who is which MailPlus uid, provisioned host names)
  in global `~/.agents/` on this machine, not in `AGENTS.md` or living
  `docs/`. Pointer: `~/.agents/surmount-server/operator-facts.md`. Process
  rule that may stay public: the same local-part on another domain is
  **not** an alias of an existing mailbox. Distinct MailPlus accounts are
  separate User mailboxes until the operator says otherwise. IMAP username
  is that mailbox's own address.
- **Complete sentences for leftover clicks (operator 2026-08-20).** When you
  ask the operator to do something in Namecheap, write full American English
  sentences. One leftover click is one paragraph. Say which domain, which
  page, which control, what the click does, what they must not do, and how
  we will know it worked. Do **not** stack a key tag, a TTL, SERVFAIL, and
  a UI toggle into one numbered fragment. The operator knows DNS. Unclear
  shorthand is the failure, not their knowledge.
- **Do not invent leftover clicks this turn.** Operator 2026-08-20: they
  did not ask about `baxterartworks.com` in the DNSSEC leftover. Do **not**
  add Baxter Custom MX, Baxter `_dmarc`, or any other domain they did not
  name as a click they owe right now. Mail-record law still applies when
  that domain is in scope. It is not a license to dump every extra zone
  into the current reply.

## Mail records on every domain we send or receive through (operator 2026-08-20)

This is standing law. Do not forget it after compaction.

Whenever this host **sends or receives mail** for a domain (primary
`surmount.systems`, extra local domains such as `cryptoquick.com` and
`baxterartworks.com`, or any later mailbox domain), that domain gets the
same thoughtful mail-record hardening we already treat as required on
the primary, **including primary-class DNSSEC**. **Do not** harden only
`surmount.systems` and leave extra mail domains on leftover Cloudflare,
parking URL, leftover-unsigned DNSSEC, missing SPF, missing dual DKIM
TXT, missing TLS-RPT, or a leftover parent DS that makes validating
resolvers SERVFAIL.

Apply, for each mail domain we actually use:

- Working public DNS (Namecheap hosted DNS when we are the registrar;
  leftover Custom DNS / Cloudflare NS is not "good enough" for records
  we must edit).
- Primary-class **DNSSEC**: Namecheap hosted **DNSSEC Status** toggle
  **ON** (operator click; published API has no DNSSEC method). Same
  security and deliverability class as `surmount.systems`. ECDSA P-256
  SHA-256 (algorithm 13) is acceptable for now. Do **not** leave a mail
  domain leftover-unsigned.
- No leftover parent **DS** without a matching DNSKEY (that breaks the
  zone for validating resolvers; cryptoquick.com was this class).
  DS digest type 1 (SHA-1) must FAIL even if a DNSKEY exists. A leftover
  unmatched DS is a SERVFAIL **bug to clear, then sign** with the same
  hosted toggle. It is **not** a reason to leave the zone unsigned.
  We asked the operator to toggle cryptoquick **OFF** only because
  leftover parent DS key tag **2368** (algorithm 13, digest type 1
  SHA-1) had no matching DNSKEY. That was sequencing. It is **not**
  "cryptoquick stays unsigned." Do **not** re-add key tag 2368 by hand.
- SPF, dual DKIM TXT (Ed25519 + RSA-4096 selectors we actually sign
  with), DMARC at the operator-directed policy (today live mailbox
  policy is `p=quarantine`; do **not** use `p=reject`).
- TLS-RPT, CAA, MTA-STS as on the primary, on names this cert and this
  host actually serve.
- rDNS/PTR when we control it (SHC), for hosts we send through.

Static-site-only extra vhosts are **not** automatically mail domains.
Unowned domains are not ours to publish records on. Public **MX flip**
stays parked until dual-sign + PTR are green **and** the operator asks
for that flip. Local aliases can exist before public MX points here.
Laptop Namecheap custody still applies (ClientIp = laptop egress).

Living maps: [docs/DNS.md](docs/DNS.md), [docs/SECURITY.md](docs/SECURITY.md),
[docs/COMPACTION-PIN.md](docs/COMPACTION-PIN.md).

## Greenfield versions

- Prefer **current major versions** of engines we choose (mail, edge, UI deps).
- **Always validate** pins are still latest when touching them.
- **nixpkgs lag is not a reason to stay old.** Surmount package overlay,
  overrides, or vendored FODs are fine when upstream is ahead of the channel.
- Stalwart is pinned in `nix/packages/stalwart-mail.nix` (not channel 0.11.8).
  Compatibility with old Stalwart is **not** a goal.
- On source builds, prefer system libs with fixed ABI (for example
  `pkgs.rocksdb`) over crate-bundled native trees when practical. Binary FOD
  embeds RocksDB today; that gap is documented in packages-and-forks.

## NEVER secrets in git (absolute; public repo)

- **Never** commit secrets: plaintext, ciphertext, age/sops private keys,
  `.env`, LUKS keyfiles or recovery material, or anything that unlocks
  production.
- **Never suggest** committing secrets "because encrypted," including
  sops-encrypted LUKS keyfiles. Entirely out of the question for this
  public tree.
- Secrets stay **off git**. Deploy secrets live on the host via
  operator-controlled channels. LUKS unlock: passphrase / initrd SSH / TPM.
- Detail: [docs/hygiene.md](docs/hygiene.md) (top rule),
  [docs/SECRETS.md](docs/SECRETS.md),
  [docs/research/luks2-and-deploy-secrets.md](docs/research/luks2-and-deploy-secrets.md).
- **Private-data pre-commit:** `script/git-hooks/pre-commit` runs
  `surmount-private-data --staged` (patterns only). Never paste
  provisioned-host IPs, keys, tokens, or screenshot contents into the tree,
  residual, reports, or fixtures. Synthetic detector samples only under
  `script/testdata/private-data/` (crate copy under
  `crates/surmount-private-data/testdata/`). Run `surmount-private-data --tree`
  before proposing commits that touch hosts/ or secrets layout.

## Stack language (operator direction 2026-07-30)

- Prefer **NixOS + Nix + Rust** for product and ops gaps.
- **No Python** in the product/ops stack (fail2ban Python is
  transitional-at-most; replace with Rust + nft lean).
- **No NPM** ecosystem.
- Short form why: [docs/principles.md](docs/principles.md).

## Edge and security (operator direction 2026-07-30)

- **No nginx** as product edge. In-tree nginx is **transitional-to-delete**.
  Prefer **first-party Axum** HTTPS edge (TLS, certs, rate limit); cert path
  not locked ACME-only. See [docs/EDGE_AND_TLS.md](docs/EDGE_AND_TLS.md).
- Prefer **Unix domain sockets** between local services over TCP localhost.
- **Arti onion/hidden services REQUIRED** (Tor Project Rust Arti); first-class
  next to clearnet; HS keys never in git; not optional. See
  [docs/COMPACTION-PIN.md](docs/COMPACTION-PIN.md) section 7 and
  [docs/research/arti-and-secrets-manager.md](docs/research/arti-and-secrets-manager.md).
- First-class spam detection, lock-down, integrity.
- Host: **operator-chosen VPS**. Size/plan open (Q-HOST-1); do **not** invent
  or publish provider names, RAM/disk/core counts, or plan SKUs.
- No Cloudflare products as critical path (research cites OK).

## Evidence before "unsafe"

- Do **not** claim something is unsafe, broken, or "version skew is dangerous"
  without evidence (failed build, broken import, measured issue, failed restore).
- Prefer: measured fact, open question, or historical note.
- Current pin (0.16.15) store evidence:
  [docs/research/stalwart-0.16.15-stores-evidence.md](docs/research/stalwart-0.16.15-stores-evidence.md).
- Historical 0.11.8 only:
  [docs/research/stalwart-stores-evidence-2026-07-30.md](docs/research/stalwart-stores-evidence-2026-07-30.md).

## No assumed operator acceptance

- Scaffold, research, and agent prose are **not** operator acceptance.
- Prefer **proposed**, **scaffold default**, **research finding**, **open**,
  **operator-deferred**, or **operator direction YYYY-MM-DD** until the
  operator explicitly approves something further in chat or writing.
- Do not say "we decided" or "locked" without that approval.
- Operator direction files are dated working direction, not eternal law.

## Packages and forks

- Detail: [docs/packages-and-forks.md](docs/packages-and-forks.md).
- Stalwart patches: Surmount fork `SurmountSystems/stalwart` when needed.
- Consume via flake input URL+rev (operator bumps) or operator-managed path
  submodule. **Agents never `git commit` / `git push` the fork** unless the
  operator explicitly instructs that work.
- Do not clone the fork if network/git policy blocks it; document the pattern.

## How agents work here

- Multi-file research, diagnosis, or non-trivial implementation: use
  **hierarchical subagents** (coordinator holds goals; workers own greps/edits;
  write short **reports** under `~/.agents/reports/` on this machine). Parent
  thread stays thin. Call on-disk handoffs **reports**, never "joins".
- Update living docs in the same turn as design changes.
- New durable stores: update [docs/DATASTORES.md](docs/DATASTORES.md) inventory.
- Agents never `git commit` (human-signed only). No bulk find-and-replace.
- ASCII only in docs we own; no em dashes.
- **Name the UI the operator showed (operator 2026-08-17).** Do not assume
  Thunderbird, or any other client, from an older mail thread. Read the
  screenshot: Evolution has Mail / Contacts / Calendar / Tasks / Memos.
  Say Evolution when that is what they opened.

## Agent plans and reports live on the host (2026-08-18)

Leftover project `.agents/plans/` and `.agents/reports/` were moved
2026-08-18 to the host homes. Leftover project `.grok/joins/` reports were
moved the same day to `~/.agents/reports/` on this machine. Do **not**
recreate repo `.agents/plans/`, `.agents/reports/`, or `.agents/joins/` as
a live home. Do **not** create project-root `.grok/` for reports, plans, or
scratch. Call on-disk handoffs **reports**, never "joins".

- New **plans**: `~/.agents/plans/` (use `~/.agents/plans/surmount-server/`
  when a filename would collide).
- New **reports**: `~/.agents/reports/` on this machine. They are not part
  of the git tree.
- **Project-specific operator facts** (living mailboxes, uid maps, host
  identity): `~/.agents/surmount-server/` on this machine, starting with
  `operator-facts.md`. The public repo points at that directory. It does
  not copy the facts in.
- Product skill roots under `.agents/skills` (if any) stay. `.gitignore`
  already ignores `.agents/`; keep that ignore.
- Do not write new handoffs under `.grok/joins/` or recreate that directory.
- Product scripts and tests must not mkdir those leftover homes. Use
  `mktemp` / `$TMPDIR`, or host `~/.agents/reports/` for agent notes.

## Git is operator-owned (operator 2026-08-17)

Git is operator-owned. The index and the working tree are **how the
operator keeps track of what agents did**. Agents never `git add`,
`git commit`, `git push`, stage, or **touch the index at all**
(`git restore --staged`, `git reset`, `git rm --cached`, anything that
writes the index). Finding a staged tree is **not** a reason to unstage.
Do not "fix" or "clean up" Git. That erases their tracker. Tell them if
they asked. Do not nag about commits. Do not mention uncommitted trees
in reports unless they asked about Git that turn. Operator 2026-08-28:
stop touching Git. Unstaging is still touching Git.

## "Always remember" is dual-pin law (operator 2026-08-27)

When the operator says **always remember**, that is not a chat promise.
Write it in **both** this file **and** [docs/COMPACTION-PIN.md](docs/COMPACTION-PIN.md)
in the same turn (standing rule + compaction reload table). One file is
attention dilution. Chat-only is compaction loss. Nix needing an untracked
file is still not a reason to stage.

## Mention is in scope; remote builder and niceness (operator 2026-08-17)

If the operator mentioned work, that work is in scope. Implement it. Do **not**
park mentioned residual as "need hostname" or "operator-gated" when access
already exists.

**After a guest incident, finish the named process (operator 2026-08-27).**
Do not tell the operator they need do nothing, or to wait, when leftover is
`just check-remote`, then `just deploy-host -- --dry-run`, then the real
switch. A reboot does not cancel that slice. Agents never reboot. Keep the
Eternal Terminal window. Continue test, dry-run, and switch unless they said
stop.

Comprehensive residual means finish the **named leftover slices**. Do **not**
invent unlocked tracks. Do not start an MX flip, a DMARC `p=reject`
change, live Vaultwarden / ban drop, a Q-AUTH-1 redesign, or a reboot
unless the operator asked for that slice.

**surmount-1** is the allowed remote Nix builder (ssh-ng). Mail and
builder work share that host. That is why niceness and a hard memory
cap matter. **Lake is optional and default off** (`surmount.lake` in
`modules/lake.nix`; operator 2026-08-25 supersedes the 2026-08-18
"never a lake unit" line). Do **not** start Lake on the live guest
from an agent. When host-local enables it, the unit must be niced,
`MemoryMax`-capped on `surmount-lake.service`, and journaled. Do not
Nice= mail.

Dev builds (remote Nix store builds, flake check, hygiene compiles) are
always maximally nice: `nice -n 19` and idle ionice when practical.
Use the real CPUs. Do **not** leave the builder idle on 1-2 jobs while
cores sit unused. Do **not** apply systemd `CPUQuota=95%` as 95 percent
of **one** CPU (that starves the ssh-ng protocol and parks cores).
`cpuQuota = "auto"` means 95 percent times online CPUs on the niced
wrapper, and no one-CPU quota on the slices.

Mail and other critical services (`stalwart`, `management-ui`, `sshd`,
`arti`, networking) are **not** nice. They stay at normal priority. Do
**not** set `Nice=` on those units. Do **not** globally starve mail by
nicing the whole machine. Prefer a niced builder user and a niced
remote-build path.

**Hard memory cap (SHC ticket 261, closed 2026-08-18; console evidence
2026-08-18).** **surmount-1** is mail plus the allowed remote Nix
builder. Mail and builder work share that host. That is why niceness
**and** a hard memory cap both matter. SHC said our RAM use at the tail end
cannibalized the hypervisor host, and Proxmox automatically OOM-shutdown
the guest as a protection step. They asked for safety thresholding on CPU,
RAM, and storage so torture tests stay manageable (about 95 percent
guards). They added NVMe swap as a cushion. Do **not** publish provider
plan SKUs, RAM/core/disk counts, swap size, or provisioned IPs.

Niceness (`nice -n 19`, idle ionice) is **not** a memory cap. Uncapped
Lean/Lake OOM killed systemd and took inbound networking with it
(console evidence 2026-08-18). Builder work **must** have a hard memory
cap (systemd `MemoryMax` / cgroup `memory.max`) plus the about-95-percent
CPU/RAM/storage guards SHC asked for. Those 95 percent guards are a
**total** ceiling, not "give the builder 95 percent of guest RAM."
Mail and other critical services must **not** be starved. In-tree:
`surmount.remoteBuilder` (default off) owns that cap for **ssh-ng /
nixbuilder only**; enable from host-local. Trusted ssh-ng forwards
rustc to **system `nix-daemon.service`**. `MemoryMax` and `Nice=19`
must apply to **that** unit, not only `user-<uid>.slice`. A
nixbuilder-only cap is a miss. Job slots (`maxJobs`, laptop machines
`max-jobs`) must stay under that memory cap. Do **not** advertise a
fake high slot count that lets Nix spawn more parallel rustc than the
guest can hold. Speed factor is not a substitute for `MemoryMax`.
Scaffold `memoryMax = "4G"` is a builder budget, not a published guest
RAM size. Scaffold `maxJobs = 8` is a memory-safe default, not a
published core count. The builder is the preferred in-guest OOM
victim (`OOMScoreAdjust`). Enable `surmount.hardening.qemuGuestAgent`
from host-local so SHC can inject a command when SSH is stuck. Do **not**
start Lake from an agent. `surmount.lake.enable` stays false until
host-local turns it on after this generation is live.

Operator correction 2026-08-18: Nix is **misconfigured** if builds do
not make maximally nice use of cores **without also OOMing the box**.
That is the question. Do not treat "why was the static site slow" as
the problem. Mention is in scope. Fix the live builder path.

We were wrong to treat the first outage as an unexplained Stopped and to
ask SHC to restore reachability without a reboot. **Agents never reboot.**
Do **not** open another ticket to re-argue the Stopped badge.

Ops dual-pin: [docs/OPS.md](docs/OPS.md) section **Remote Nix builder and niced
Lake**.

## Measure hardware before Nix job knobs (operator 2026-08-18)

The operator laptop is **local**. It is not the remote builder. Never
call the laptop remote. **surmount-1** (the mail host) is the remote
builder. Measure each machine on that machine: `inxi -C` on the laptop
for laptop-local Nix only; `ssh surmount-1 'inxi -C -c0'` (or `lscpu` /
`nproc` if `inxi` is missing) for guest slots. Never treat laptop
`inxi` as the mail host. Whose fault we skip remote `inxi` first is
ours. Builder `max-jobs` / physical cores in this conversation means
the mail host.

`/etc/nix/machines` lives on the laptop (the Nix **client**) and
describes the remote (surmount-1). Installing that file on the laptop
does not make the laptop the builder.

Assume nothing about core counts or RAM. Do **not** copy laptop `nproc`
onto `modules/remote-builder.nix`.

| Knob | Machine | Source of truth |
|------|---------|-----------------|
| Laptop local `max-jobs` / cores | Operator laptop (this session host) | Live `inxi` on the laptop. 8 physical / 16 threads. If setting laptop system `max-jobs`, use 16 threads. Separate from machines-file slots. Guest down: `BUILD_LOCAL=true just ...` (empty builders, local `max-jobs = auto`). |
| machines-file `max-jobs` (jobs sent to the builder) | Mail host surmount-1 | Live guest `nix.settings.max-jobs` after measuring that box with `ssh surmount-1 inxi` / `lscpu`. Not laptop inxi. |
| Guest `MemoryMax` / `CPUQuota` | Mail host | Live guest facts. Do not change unless those remote facts prove they are still wrong. |

System Nix does **not** read `~/.config/nix/machines`. Root and the
laptop `nix-daemon` need `/etc/nix/machines` plus
`builders = @/etc/nix/machines`. A sudo password on that install is a
local laptop prompt, not proof the laptop is remote. Do **not** publish
guest RAM, disk, SKU, or IP numbers in git. CPU model plus physical vs
logical may go in the local implement report only. Dual-pin:
[docs/OPS.md](docs/OPS.md) **Remote Nix builder**.

Assume nothing. Run `inxi` (or `lscpu` / `nproc`) **on the machine you
mean**. Remote mail-host specs go over SSH as `nixbuilder` **without
sudo** (`just host-inxi`, or `nix shell nixpkgs#inxi -c inxi ...`
until `pkgs.inxi` is on the guest PATH after deploy). Never treat the
laptop as the remote. Do **not** set guest `max-jobs` to raw guest
`nproc`. Dual-pin: [docs/OPS.md](docs/OPS.md) **Host hardware probe**.
