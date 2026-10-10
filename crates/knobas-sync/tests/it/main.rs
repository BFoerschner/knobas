//! `knobas-sync`'s integration tests, one binary (ADR-0017).
//!
//! Each module was a test binary of its own until #576 and keeps its file's
//! name, so a test's full name still says where it lives. Each module still
//! gets a database of its own: `knobas_db::test_util` keys the shared database
//! by the calling source file, not by the process (#572).
//!
//! `scheduler_loop.rs` stays a binary of its own, because its 5 s timeout
//! could not absorb a merged binary's extra load (`test-layout-exceptions.txt`).

mod alerts;
mod attach;
mod backfill;
mod config;
mod cursor;
mod dedicated;
mod progress;
mod run_log_recovery;
mod run_log;
mod run;
mod samples;
mod scheduler_run;
mod stats;
mod sweep;
mod write_activity;
mod write_choke_point;
mod write_queue;
