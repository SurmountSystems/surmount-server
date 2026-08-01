//! JSON API routes (health, domain/account stubs, JMAP proxy placeholder).

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "surmount-management-ui",
        version: env!("CARGO_PKG_VERSION"),
    })
}

pub async fn list_domains(State(state): State<Arc<AppState>>) -> Json<Value> {
    // Skeleton: surfaces configured primary domain. Real data will come from
    // Stalwart directory / declarative Nix options mirrored at deploy time.
    Json(json!({
        "domains": [
            {
                "name": state.config.primary_domain,
                "role": "primary"
            },
            {
                "name": state.config.mail_hostname,
                "role": "mail_hostname"
            }
        ],
        "note": "stub: wire to Stalwart directory or rendered Nix inventory"
    }))
}

pub async fn list_accounts(State(state): State<Arc<AppState>>) -> Json<Value> {
    let domain = &state.config.primary_domain;
    Json(json!({
        "accounts": [
            {
                "id": "example-admin",
                "address": format!("admin@{domain}"),
                "status": "placeholder"
            }
        ],
        "note": "stub: no real directory query yet; create accounts via stalwart-cli or /admin"
    }))
}

#[derive(Debug, Deserialize)]
pub struct JmapRequest {
    /// Opaque JSON body placeholder for future JMAP method calls.
    #[serde(flatten)]
    #[allow(dead_code)]
    pub body: Value,
}

/// Placeholder JMAP proxy. Returns 501 until auth + upstream forwarding land.
pub async fn jmap_proxy_placeholder(
    State(state): State<Arc<AppState>>,
    Json(_req): Json<JmapRequest>,
) -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": "jmap_proxy_not_implemented",
            "message": "JMAP proxy skeleton only. Point clients at Stalwart HTTP or implement proxy.",
            "stalwart_url": state.config.stalwart_url,
        })),
    )
}

/// Best-effort reachability probe against local Stalwart HTTP.
pub async fn stalwart_status(State(state): State<Arc<AppState>>) -> Json<Value> {
    let url = state.config.stalwart_url.trim_end_matches('/').to_string();
    let probe = state.http.get(format!("{url}/")).send().await;
    match probe {
        Ok(resp) => Json(json!({
            "reachable": true,
            "status": resp.status().as_u16(),
            "url": url,
        })),
        Err(err) => Json(json!({
            "reachable": false,
            "error": err.to_string(),
            "url": url,
        })),
    }
}
