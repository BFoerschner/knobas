//! `knobas-app`'s integration tests, one binary (ADR-0017).
//!
//! Each module was a test binary of its own until #579 and keeps its file's
//! name, so a test's full name still says where it lives. Each module still
//! gets a database of its own: `knobas_db::test_util` keys the shared database
//! by the calling source file, not by the process (#572). `support/` holds
//! the fixture helpers the modules share (`support::source_config`,
//! `support::role_miss`) and the fixture files they embed with `include_str!`.
//!
//! The live suites (`alert_chain_live`, `atlassian_live`, `confluence_live`,
//! `estate_live`, `kuma_write_live`, `start_work_live`, `teamcity_seeded_live`)
//! and `share_exit` stay binaries of their own, because a recipe runs each by
//! name (`test-layout-exceptions.txt`). `live_digest/` is theirs.

mod adapter_to_mirror;
mod after_landing;
mod assets_ipc;
mod background_write;
mod backup;
mod backup_ipc;
mod bring_up;
mod builds_ipc;
mod builds_list_plan;
mod builds_of_a_commit;
mod builds_of_a_commit_perf;
mod capture_ipc;
mod checkout_ipc;
mod clone_ipc;
mod code_facts_ipc;
mod command_statements;
mod config_schema_mirror;
mod console_choke_point;
mod contexts_ipc;
mod conversation_ipc;
mod day_offers_ipc;
mod demo;
mod demo_delete_during_load;
mod dependencies_ipc;
mod docs_ipc;
mod entity;
mod entity_role;
mod entity_view;
mod estate_exit;
mod external_adapter;
mod external_adapter_install;
mod external_adapter_release;
mod inbox_ipc;
mod ipc;
mod item_field;
mod jira_views_ipc;
mod job_ipc;
mod job_lifecycle_ipc;
mod job_pipelines_ipc;
mod keychain_write;
mod link_on_arrival_ipc;
mod link_set_patterns_ipc;
mod link_sets_ipc;
mod live_reads;
mod monitor_on_its_way_ipc;
mod notes_export_ipc;
mod notes_search_ipc;
mod pages_kept_open;
mod pr_viewed_ipc;
mod probe_snapshot;
mod project_census;
mod protocol_ipc;
mod pull_request_worktree;
mod read_plans;
mod refresh;
mod refused_writes;
mod registry_reads_ipc;
mod renderer_commands;
mod responsiveness_probe;
mod run_pipeline;
mod run_presets_ipc;
mod run_reads_ipc;
mod run_ticket_ipc;
mod search_ipc;
mod signals_ipc;
mod sources_crud;
mod sources_registry;
mod standup_ipc;
mod start_work;
mod status_move;
mod suggestion_pass;
mod suggestions_ipc;
mod support;
mod ticket_keys_ipc;
mod ticket_views_ipc;
mod time_ipc;
mod ui_pool_probe;
mod ui_statement_limit;
mod unauthorized_write;
mod uncommitted_diff;
mod url_resolve;
mod week_ipc;
mod wiring;
mod working_tree;
mod worklog_ipc;
