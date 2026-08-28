# Hash primitives in the production Rust lockfile (2026-08-26)

Best-effort inventory of **SHA-1** and **MD5** in the Surmount Server
**Cargo** production graph. This is not a pentest. It does not claim
Nixpkgs, Git, or third-party daemons have no SHA-1 inside C code.

**Question:** does this Cargo graph ship a SHA-1 or MD5 hasher we
selected for production crypto?

**Method:**

1. `rg` over `crates/Cargo.lock` for `sha1`, `SHA-1`, `md5`, `MD5`, `md-5`.
   Result: **no matches**.
2. Parse every `[[package]]` `name` in that lockfile (444 packages).
   Result: **zero** crates named `sha1`, `sha-1`, `md5`, `md-5`, or
   similar.
3. Reverse-map dependents of `ring`, `sha2`, `hmac`, `openssl`,
   `aws-lc-rs`, `rustls`.
4. Read workspace `crates/Cargo.toml` rustls features.
5. `rg` `Sha1` / `sha1::` in `crates/**/*.rs`: **no matches**.
6. `cargo metadata` was attempted; this environment did not emit JSON
   (no usable cargo index). Lockfile parse is the SoT used here.

**Workspace rustls pin:**
`rustls = { version = "0.23", default-features = false, features =
["aws-lc-rs", "prefer-post-quantum", "std", "logging"] }`.
Product `ServerConfig` is TLS 1.3 only (`crates/Cargo.toml` comment).
Outbound HTTP (reqwest / hyper-rustls) may still unify TLS 1.2
**protocol** support; that is not SHA-1 in our source, and it does not
change the edge ServerConfig list.

## Direct findings

| Item | In `crates/Cargo.lock`? | Role |
|------|-------------------------|------|
| Crate named sha1 / md5 | No | N/A |
| `sha2` 0.10.9 and 0.11.0 | Yes | SHA-256 family. Dependents include `age`, `age-core`, `surmount-management-ui`, `hmac`/`scrypt` path. |
| `hmac` | Yes | Used with sha2 (age, hkdf, pbkdf2, management-ui). |
| `digest` 0.10 and 0.11 | Yes | Trait crate, not SHA-1 by itself. |
| `aws-lc-rs` | Yes | Product TLS provider. Dependents: rustls, rustls-webpki, instant-acme, rcgen 0.14, x509-parser. |
| `ring` 0.17.14 | Yes | Listed as a **rustls 0.23.42** dependency in the lockfile (also quinn-proto, rcgen 0.13.2, rustls-webpki). Ring historically implements SHA-1 for TLS 1.2 compatibility. Our rustls features select **aws-lc-rs**, not ring, but Cargo still records ring because rustls lists it. We did **not** prove the linked `surmount-management-ui` binary contains ring SHA-1 symbols. |
| `openssl` crate | No | Not a Cargo dep. |

## What this does **not** cover

- **Git object IDs.** This git repo still uses Git's default SHA-1
  object names unless converted to SHA-256. That is Git, not a mail or
  TLS primitive we selected.
- **Nix store hashes.** NAR/store hashing is SHA-256.
- **Nixpkgs C libraries** on the guest (`openssl`, GnuTLS, Stalwart
  FODs). Those binaries are not in this Cargo.lock. OpenSSL still
  *implements* SHA-1 for compatibility; our product TLS path is rustls
  + aws-lc-rs, TLS 1.3.
- **Stalwart / Vandelay / etserver / qemu-ga.** Packaged, not this
  workspace graph.
- **Runtime proof** that rustls did not pull ring into the UI
  rlib (`nm` / `cargo tree -e features` not run here).

## Honest leftover for this inventory

1. Feature-unification proof (`cargo tree -e features -p rustls` on
   the builder) to confirm ring is unused in the management-ui link.
2. Guest OpenSSL/Stalwart SHA-1 *availability* vs *use* (separate from
   this lockfile).
3. Git SHA-256 object format (repo conversion) if we want Git itself
   off SHA-1 object names.

Accessed: 2026-08-26. Lockfile: `crates/Cargo.lock` as of this date.
