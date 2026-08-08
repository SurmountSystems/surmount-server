# Impl report: soft-debt mop (no present-tense lies)

Date: 2026-08-07
Scope: docs-only mop after research 26.05 rewrite + `services.stalwart` rename.
No `.nix` product edits. No git commit. No `just ci` (docs only).

## Living facts (tree)

| Fact | Source |
|------|--------|
| Option path **`services.stalwart`** | `modules/stalwart-service.nix`, `modules/mail.nix` |
| Unit / state **`stalwart-mail*`** | same module (`systemd.services.stalwart-mail`, dataDir) |
| On-disk engine config | generated **config.json** DataStore (not TOML `settings`) |
| Host channel **nixos-26.05** | prior living scrub + tree |
| Engine **0.16.15** FOD | `nix/packages/stalwart-mail.nix` |
| spam-filter **3.0.0** FODs | `nix/packages/stalwart-spam-filter.nix`; installed under `/etc/surmount/stalwart/` |
| Stalwart HTTP management **:8080** | first-boot defaults; mail.nix helper `STALWART_URL` |
| Surmount UI default **:8090** | `modules/options.nix` |

## Must-fix items

### 1. `docs/research/stalwart-stores-evidence-2026-07-30.md`

Made the whole surface honest **historical** for the 0.11.8-era module:

- Option row labeled **at evidence date** (`services.stalwart-mail` stock); living
  option called out as **`services.stalwart`**.
- Section 1 header past tense ("what the evidence date built").
- Surmount wiring block past tense ("then enabled `services.stalwart-mail`")
  plus living pointer.
- Section 5 no longer "currently evaluated"; TOML role map is evidence-date only.
- Section 6 "packaging bump in flight" replaced with landed 0.16.15 history.
- Section 7 open items status-tagged (package bump / evidence file done; design
  still open).
- Section 8 re-verify: historical commands kept as historical; living eval shapes
  use `services.stalwart.storePath` / `storeType` (not TOML settings).

### 2. `docs/DATASTORES.md` section 5.2

Replaced present-tense "flake eval TOML" conceptual block with living reality:

- Surmount owns **0.16 config.json** via `services.stalwart` options.
- `settings` accepted-and-ignored; not written to disk.
- Table of what `modules/mail.nix` sets (`storeType`, `storePath`, ...).
- Conceptual JSON `@type` / `path` only (no invented multi-role TOML map as live).
- spam-filter **3.0.0** under `/etc/surmount/stalwart/` (not `v2.0.5` TOML settings).
- Section 5.4 and inventory row aligned to the same story.
- **Last updated:** 2026-08-07.

### 3. rg sweep (living surface)

| Pattern | Living product docs / modules result |
|---------|--------------------------------------|
| `option path stays services.stalwart-mail` | **none** |
| `Surmount keeps services.stalwart-mail` as option | **none** |
| present-tense "we are on 25.05" / "boots nixos-25.05" | **none** (only historical footnotes) |
| `services.stalwart-mail` as option | only **historical** research + contrast ("not ...") + **unit** name `systemd.services.stalwart-mail` |

Living modules consistently claim **`services.stalwart`**.

### 4. `docs/research/version-audit.md`

No "Intentionally leave 25.05 body" soft language. Living tables already match
tree (26.05 host, crane 1.95, option `services.stalwart`). No edit required
beyond prior research pass.

### 5. `.agents/reports/impl-research-26.05-truth.md`

Updated so the report cannot re-introduce debt:

- Living fact row: option is **`services.stalwart`** (rename landed).
- Scope line notes rename landed later.
- "Intentionally not done: rename" removed; replaced with follow-up pointing here.

## Adjacent living-doc lies mopped same pass

Operator asked for complete pre-deploy honesty; these still read as live truth:

| File | Fix |
|------|-----|
| `docs/OPS.md` | UI health **:8090**; Stalwart tunnel **:8080** (was 8081 / mixed) |
| `docs/MIGRATION.md` | `127.0.0.1:8080` (was 8081) |
| `docs/SEARCH_AND_UI.md` | diagram Stalwart **:8080** (was 8081) |
| `docs/architecture-review.md` | cleanup table marks mopped rows |
| `docs/research/scaffold-assumptions-inventory.md` | sections 13, 14, 27 lag claims updated |

## Verify

```text
# living option path
rg -n 'options\.services\.stalwart|config\.services\.stalwart' modules/*.nix
# should NOT show options.services.stalwart-mail

# no living "keeps old option path" prose
rg -n 'option path stays|keeps services\.stalwart-mail' docs README.md modules

# historical file may still quote services.stalwart-mail with "evidence date" /
# "then" language only
```

Docs only: **no** `just ci` / rebuild.

## Residual (honest, not soft-parked as living product)

1. **`.grok/joins/foundation.md`** may still lag ports/module story; trust modules
   + `stalwart-current.md` + COMPACTION-PIN.
2. **architecture-review** still lists join lag and fix-and-fixos "until packaging"
   as cleanup candidates where those surfaces remain historical or already landed.
3. **Upstream "latest" re-poll** for Stalwart FODs / Cargo.lock still dated
   2026-07-30/31 (not this pass).
4. Prior research reports under `.agents/reports/` that said "option path stays
   `services.stalwart-mail`" are **historical agent notes** except
   `impl-research-26.05-truth.md` (updated). Do not copy them into living docs.

## Done criteria

| Criterion | Status |
|-----------|--------|
| Historical 0.11.8 file past tense + living option callout | done |
| DATASTORES 5.2 config.json reality | done |
| Living option path clean | done |
| No living "we are on 25.05" | done |
| version-audit no soft leave-25.05 body | already clean |
| research report does not re-introduce option debt | done |
| Report path | this file |
