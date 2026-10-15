//! Pointer only. Not a product module.
//!
//! The compiled Splora Unix-socket proxy is nested `pub(crate) mod splora`
//! in `proxy_vaultwarden.rs` (`crate::proxy_vaultwarden::splora`). That
//! module owns the portal Host, GET `/` HTML, extra onion/redirect Hosts,
//! and the named tests. `main.rs` does not `mod proxy_splora`. Do not add
//! that line: a later edit of this path would miss the portal Host.

compile_error!(
    "proxy_splora.rs is a pointer only; use crate::proxy_vaultwarden::splora"
);
