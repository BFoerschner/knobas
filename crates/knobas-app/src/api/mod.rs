//! The API explorer's way out (ux31, ADR-0057): a request a person composed,
//! sent once from the app process and never from the web view.
//!
//! - [`send`]: the request, its limits and guards (nothing changing is sent to
//!   a host that is not this computer without a yes; a secret goes only to its
//!   origin; redirects only on the same origin, for `GET` and `HEAD`).
//! - [`held`]: secrets typed on the explorer's page, held in the app process
//!   until knobas quits, keyed by origin and scheme. Björn (2026-11-22): "I'd
//!   just type it into the openapi viewer page itself. not using stored
//!   credentials." Nothing is stored: no keychain item, no settings row.
//! - [`kept`]: what was sent and answered, in memory only, bounded.
//! - [`servers`]: servers a person saved, a name and an address, no secret.
//! - [`image`]: an image at an address in a document, loaded on a press (the
//!   XML view, section 14), by the same client.
//! - [`schemas`]: the schema files a person pointed to, and whether XML files
//!   are checked against theirs (ADR-0058).

pub mod found;
pub mod held;
pub mod image;
pub mod kept;
pub mod open;
pub mod schemas;
pub mod scratch;
pub mod send;
pub mod servers;
