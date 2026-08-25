//! Namecheap DNS-01 ACME helper crate (hook, dispatch, lab, laptop renew, render).
//! Never live Namecheap as CI green. Never log ApiKey.

pub mod cred;
pub mod dispatch;
pub mod hook;
pub mod lab;
pub mod laptop;
pub mod render;
pub mod zone;
