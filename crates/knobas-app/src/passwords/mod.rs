//! The Passwords role in the app (ADR-0056): codes made on this machine, the
//! hold a shown secret lives in, the clipboard a copied one goes to, and the
//! page's reads.
//!
//! **House rule:** a secret ([`knobas_source::passwords::SecretText`]) is
//! named in this module, in `commands/passwords.rs` and in the adapters, and
//! nowhere else in the app. It is never put in a table, a file, a log line, a
//! `tracing` field, the read cache (ADR-0032) or the held reads (ADR-0042).

pub mod otp;

/// How long a shown secret stays shown, and a copied one stays on the
/// clipboard. Mirrored in the window's `tf/passwords/hold.ts`.
pub const HOLD_SECS: u64 = knobas_source::passwords::HOLD_SECS;
