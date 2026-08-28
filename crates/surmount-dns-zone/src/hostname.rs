//! Namecheap HostName mapping from relative names or FQDNs under SLD.TLD.

use anyhow::Result;

use crate::cred::die;

pub fn to_hostname(raw: &str, sld: &str, tld: &str) -> Result<String> {
    let mut raw = raw.to_string();
    while raw.ends_with('.') {
        raw.pop();
    }
    if raw.is_empty() {
        return Err(die("empty host"));
    }
    if raw.chars().any(|c| c.is_control()) {
        return Err(die("host: control characters refused"));
    }
    if !raw
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '@' | '-'))
    {
        return Err(die(format!("host has invalid characters: {raw}")));
    }

    let suffix = format!("{sld}.{tld}");
    if raw == "@" || raw == suffix {
        return Ok("@".to_string());
    }
    if let Some(host) = raw.strip_suffix(&format!(".{suffix}")) {
        if host.is_empty() {
            return Ok("@".to_string());
        }
        return Ok(host.to_string());
    }
    if raw.contains('.') {
        let last = raw.rsplit('.').next().unwrap_or("");
        if last.len() >= 2 && last.len() <= 24 && last.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(die(format!(
                "host {raw} is not under {suffix} (check SLD/TLD in credentials)"
            )));
        }
        return Ok(raw);
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fqdn_maps_under_zone() {
        assert_eq!(
            to_hostname("services.example.test", "example", "test").unwrap(),
            "services"
        );
        assert_eq!(
            to_hostname("_dmarc.example.test", "example", "test").unwrap(),
            "_dmarc"
        );
    }

    #[test]
    fn foreign_fqdn_fails() {
        let err = to_hostname("other.invalid", "example", "test").unwrap_err();
        assert!(err.to_string().contains("not under"));
    }

    #[test]
    fn relative_dkim_kept() {
        assert_eq!(
            to_hostname("stalwart._domainkey", "example", "test").unwrap(),
            "stalwart._domainkey"
        );
    }
}
