//! The light switch protocols. The devices and the web pages use this crate,
//! so both sides speak the same Rust types; irpc takes care of the wire format.
//!
//! Each version has its own ALPN, so a page for one version never talks to a
//! device running another.

pub mod v1;
pub mod v2;
pub mod v3;
