use anyhow::Result;
use reqwest::Method;

use crate::cred::Credentials;
use crate::die;
use crate::jsonutil::{extract_confirmation_id, is_confirmation_required};
use crate::mock::MockHttp;
use crate::redact;

pub struct HttpOutcome {
    pub code: u16,
    pub body: String,
}

pub enum Transport {
    Mock(MockHttp),
    Live,
}

impl Transport {
    pub fn request(
        &self,
        creds: &Credentials,
        method: &str,
        rel_path: &str,
        body: &str,
        confirm: Option<&str>,
        verbose: bool,
    ) -> Result<HttpOutcome> {
        match self {
            Transport::Mock(mock) => {
                let (code, resp_body) = mock.request(method, rel_path, body, confirm)?;
                verbose_http(verbose, code, method, rel_path);
                Ok(HttpOutcome {
                    code,
                    body: resp_body,
                })
            }
            Transport::Live => live_request(creds, method, rel_path, body, confirm, verbose),
        }
    }
}

fn verbose_http(verbose: bool, code: u16, method: &str, rel_path: &str) {
    if verbose {
        eprintln!("rdns-shc: HTTP {code} {method} {rel_path}");
    }
}

fn live_request(
    creds: &Credentials,
    method: &str,
    rel_path: &str,
    body: &str,
    confirm: Option<&str>,
    verbose: bool,
) -> Result<HttpOutcome> {
    let url = format!("{}{rel_path}", creds.api_base);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| die(format!("runtime: {e}")))?;
    let api_key = creds.api_key.clone();
    let method_owned = method.to_string();
    let body_owned = body.to_string();
    let confirm_owned = confirm.map(str::to_string);
    rt.block_on(async move {
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .build()
            .map_err(|e| die(redact(&format!("http client: {e}"), &api_key)))?;
        let m = Method::from_bytes(method_owned.as_bytes())
            .map_err(|_| die(format!("invalid method {method_owned}")))?;
        let mut req = client
            .request(m, &url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Accept", "application/json")
            .header("Content-Type", "application/json");
        if let Some(cid) = confirm_owned.as_deref() {
            req = req.header("X-User-Api-Confirm", cid);
        }
        if !body_owned.is_empty() {
            req = req.body(body_owned);
        }
        let resp = req.send().await.map_err(|e| {
            die(redact(
                &format!(
                    "API call failed (no HTTP status) method={method_owned} path={rel_path}: {e}"
                ),
                &api_key,
            ))
        })?;
        let code = resp.status().as_u16();
        let resp_body = resp
            .text()
            .await
            .map_err(|e| die(redact(&format!("response body: {e}"), &api_key)))?;
        verbose_http(verbose, code, &method_owned, rel_path);
        Ok(HttpOutcome {
            code,
            body: resp_body,
        })
    })
}

pub fn write_with_confirm(
    transport: &Transport,
    creds: &Credentials,
    method: &str,
    rel_path: &str,
    body: &str,
    verbose: bool,
) -> Result<HttpOutcome> {
    let first = transport.request(creds, method, rel_path, body, None, verbose)?;
    if matches!(first.code, 200 | 201 | 202) {
        return Ok(first);
    }
    if first.code == 409 && is_confirmation_required(&first.body) {
        let cid = extract_confirmation_id(&first.body).ok_or_else(|| {
            die(format!(
                "409 confirmation_required but no confirmation_id in body (path={rel_path})"
            ))
        })?;
        if verbose {
            eprintln!(
                "rdns-shc: re-sending with X-User-Api-Confirm (id length={})",
                cid.len()
            );
        }
        let second = transport.request(creds, method, rel_path, body, Some(&cid), verbose)?;
        if matches!(second.code, 200 | 201 | 202) {
            return Ok(second);
        }
        return Err(die(format!(
            "confirmed {method} {rel_path} failed HTTP {} (body redacted; check API)",
            second.code
        )));
    }
    Err(map_write_error(method, rel_path, first.code))
}

fn map_write_error(method: &str, rel_path: &str, code: u16) -> anyhow::Error {
    match code {
        409 => die(format!(
            "HTTP 409 conflict on {method} {rel_path} (update in progress or confirmation failed)"
        )),
        422 => die(format!(
            "HTTP 422 validation failed on {method} {rel_path} (rDNS: FCrDNS / IP / hostname; tickets: required fields)"
        )),
        401 | 403 => die(format!(
            "HTTP {code} auth failed (check operate-scoped ApiKey; never print key)"
        )),
        _ => die(format!("HTTP {code} on {method} {rel_path}")),
    }
}
