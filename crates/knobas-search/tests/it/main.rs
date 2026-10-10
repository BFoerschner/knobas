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
mod corpus_seam;
mod help_card;
mod home;
mod lists;
mod lists_no_identity;
mod search;
mod sql_containment;
mod sql_shape;
mod vocab;
