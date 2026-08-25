//! Parent DNSSEC record (Delegation Signer, not DiskStation) digest checks.
//!
//! SHA-1 digest type 1 must fail closed. Digest type 2 is acceptable to keep
//! on merge. See [IANA DS Digest Types](https://www.iana.org/assignments/ds-rr-types/ds-rr-types.xhtml)
//! (accessed: 2026-08-25).

use crate::cred::die;
use crate::zone::Record;

/// Digest type 1 is SHA-1 (IANA MUST NOT for new delegations).
pub const SHA1_DIGEST_TYPE: u8 = 1;

pub fn ds_digest_type(address: &str) -> Option<u8> {
    // Namecheap Address / presentation: KEYTAG ALGORITHM DIGESTTYPE HEX
    let mut parts = address.split_whitespace();
    let _keytag = parts.next()?;
    let _alg = parts.next()?;
    let dt = parts.next()?;
    dt.parse().ok()
}

pub fn refuse_sha1_ds(records: &[Record]) -> anyhow::Result<()> {
    for rec in records {
        if !rec.r#type.eq_ignore_ascii_case("DS") {
            continue;
        }
        if ds_digest_type(&rec.address) == Some(SHA1_DIGEST_TYPE) {
            return Err(die(
                "parent DNSSEC record uses digest type 1 (SHA-1); leftover SHA-1 is not acceptable (refuse setHosts)",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_type_1_fails_closed() {
        let recs = vec![Record {
            name: "@".into(),
            r#type: "DS".into(),
            address: "2368 13 1 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            mx_pref: "10".into(),
            ttl: "1800".into(),
        }];
        assert!(refuse_sha1_ds(&recs).is_err());
    }

    #[test]
    fn digest_type_2_ok() {
        let recs = vec![Record {
            name: "@".into(),
            r#type: "DS".into(),
            address: "370 13 2 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            mx_pref: "10".into(),
            ttl: "1800".into(),
        }];
        assert!(refuse_sha1_ds(&recs).is_ok());
    }
}
