//! `knobas-source-teamcity`'s integration tests, one binary (ADR-0017).
//!
//! Each module was a test binary of its own until #578 and keeps its file's
//! name, so a test's full name still says where it lives.
//!
//! `live_teamcity.rs` and `live_teamcity_seeded.rs` stay binaries of their own,
//! because `just teamcity-live` and `just teamcity-live-seeded` run them by
//! name (`test-layout-exceptions.txt`).

mod mockd;
