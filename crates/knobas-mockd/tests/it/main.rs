//! `knobas-mockd`'s integration tests, one binary (ADR-0017).
//!
//! Each module was a test binary of its own until #577 and keeps its file's
//! name, so a test's full name still says where it lives. `common` holds the
//! two contract suites' golden-snapshot helpers; the snapshots themselves stay
//! in `tests/golden/`, found from the crate's manifest directory.
//!
//! `jira_harness.rs` stays a binary of its own, because its 5 s timeout could
//! not absorb a merged binary's extra load (`test-layout-exceptions.txt`).

mod common;

mod allowlist;
mod cluster;
mod jira_contract;
mod jira_issue;
mod jira_search;
mod specs_pinned;
mod state;
mod teamcity;
mod teamcity_contract;
mod violations;
