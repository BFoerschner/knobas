//! `knobas-source-jira`'s integration tests, one binary (ADR-0017).
//!
//! Each module was a test binary of its own until #578 and keeps its file's
//! name, so a test's full name still says where it lives.
//!
//! `live_jira_seeded.rs` stays a binary of its own, because
//! `just atlassian-live` runs it by name (`test-layout-exceptions.txt`).

mod field_discovery;
mod mockd;
