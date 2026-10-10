//! `knobas-core`'s integration tests, one binary (ADR-0017).
//!
//! Each module was a test binary of its own until #575 and keeps its file's
//! name, so a test's full name still says where it lives. Each module still
//! gets a database of its own: `knobas_db::test_util` keys the shared database
//! by the calling source file, not by the process (#572).

mod ancestor_path;
mod checkout_scan;
mod contexts;
mod estate_file;
mod inbox;
mod link_reads;
mod mini_board;
mod notes;
mod payload_paths;
mod projects;
mod stores;
mod suggestions;
mod web_url;
mod write_queue;
