//! Namecheap forward-zone helper. Default dry-run. Hermetic mock zone.
//!
//! SHA-1 parent DNSSEC digest type 1 fails closed on setHosts.

pub mod cred;
pub mod dnssec;
pub mod hostname;
pub mod run;
pub mod zone;

pub use run::{USAGE, run};
