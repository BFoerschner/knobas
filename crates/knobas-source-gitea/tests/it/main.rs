//! `knobas-source-gitea`'s integration tests, one binary (ADR-0017).
//!
//! Each module was a test binary of its own until #578 and keeps its file's
//! name, so a test's full name still says where it lives.
//!
//! `litter_guard.rs` (a 5 s bound), `live_gitea.rs` and `live_gitea_capped.rs`
//! (run by name by `just gitea-live` and `just gitea-live-capped`) stay
//! binaries of their own (`test-layout-exceptions.txt`).

mod support;

mod client;
mod contract;
mod sync;
mod write;
