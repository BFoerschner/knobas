//! `knobas-search`'s integration tests, one binary (ADR-0017).
//!
//! Each module was a test binary of its own until #574 and keeps its file's
//! name, so a test's full name still says where it lives. Each module still
//! gets a database of its own: `knobas_db::test_util` keys the shared database
//! by the calling source file, not by the process (#572).
//!
//! `perf.rs` and `coverage.rs` stay binaries of their own, because
//! `just search-perf` runs them by name (`test-layout-exceptions.txt`).

mod ancestor_path;
mod bare_name;
mod cased_identity;
mod corpus_seam;
mod declaring;
mod help_card;
mod home;
mod item_parent;
mod lists;
mod lists_no_identity;
mod name_glob;
mod parent_key_pin;
mod path_line;
mod ranking;
mod reach;
mod readings;
mod search;
mod sql_containment;
mod sql_shape;
mod state_chip;
mod suggest;
mod ticket_by_key;
mod ticket_keys;
mod under;
mod under_confluence;
mod under_coverage;
mod under_jira;
mod under_teamcity;
mod vocab;
