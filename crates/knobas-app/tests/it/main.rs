//! `knobas-app`'s integration tests, one binary (ADR-0017).
//!
//! Each module was a test binary of its own until #579 and keeps its file's
//! name, so a test's full name still says where it lives. Each module still
//! gets a database of its own: `knobas_db::test_util` keys the shared database
//! by the calling source file, not by the process (#572). `support/` holds
//! fixture files the modules embed with `include_str!`.
//!
//! The live suites (`alert_chain_live`, `atlassian_live`, `confluence_live`,
//! `estate_live`, `kuma_write_live`, `start_work_live`, `teamcity_seeded_live`)
//! and `share_exit` stay binaries of their own, because a recipe runs each by
//! name (`test-layout-exceptions.txt`). `live_digest/` is theirs.

mod adapter_to_mirror;
mod assets_ipc;
mod backup;
mod backup_ipc;
mod capture_ipc;
mod checkout_ipc;
mod config_schema_mirror;
mod contexts_ipc;
mod demo;
mod entity;
mod entity_mirror;
mod estate_exit;
mod inbox_ipc;
mod ipc;
mod protocol_ipc;
mod search_ipc;
mod sources_crud;
mod sources_mirror;
mod sources_registry;
mod standup_ipc;
mod start_work;
mod status_move;
mod suggestions_ipc;
mod time_ipc;
mod url_resolve;
mod week_ipc;
mod wiring;
mod worklog_ipc;
