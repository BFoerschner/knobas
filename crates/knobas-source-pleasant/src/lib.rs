//! Pleasant Password Server as a source of the Passwords role (ADR-0056).
//!
//! It mirrors entries and folders, never a password. A password is read live,
//! one request a press; a one-time-code key goes to the sink's side channel
//! and from there to the OS keychain. It never calls `GET entries/{id}`.

/// The API's shapes and the mapping, open to the tests that replay the real
/// server's recordings; nothing else names them.
#[doc(hidden)]
pub mod api;
mod config;
mod descriptor;
#[doc(hidden)]
pub mod map;
mod otp;
mod source;

pub use config::{PleasantConfig, Walk};
pub use descriptor::descriptor_template;
pub use source::{PleasantSource, build};

/// The adapter kind: `pleasant`.
pub const ADAPTER_KIND: &str = "pleasant";
/// This crate's version, the adapter's.
pub const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");
