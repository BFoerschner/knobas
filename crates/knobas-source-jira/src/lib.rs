// The adapter is built bottom-up: every module below has its own tests, but
// nothing reaches `http`, `model`, `map`, `cursor` or `jql` from the crate's
// public surface until `source.rs` lands (task 6, the second stream-A PR), and
// `just check`'s `clippy --workspace --lib` pass deliberately excludes test
// targets. Task 6 deletes this line; the reviewer of that PR checks that it is
// gone.
#![allow(dead_code)]

//! The Jira **Data Center** adapter: `/rest/api/2/…`, `startAt` pagination.
//!
//! Read-only in M1 (interfaces doc §4.1): it declares no capabilities and no
//! write ops, and [`Source::write`](knobas_source::Source::write) will refuse
//! everything. Write-back (transition, comment, create) is M2.
//!
//! # Which Jira this speaks
//!
//! Data Center / Server, REST **v2**. Cloud's `/rest/api/3/search/jql` with
//! `nextPageToken` paging is a *different product surface*, and confusing the
//! two is the failure mode roadmap §4 gotcha 4 exists to prevent -- so the
//! dialect is an explicit configuration field ([`Flavor`]) that is refused by
//! name rather than guessed at.
//!
//! The endpoint truth is the vendored WADL, `testenv/specs/jira-dc-rest.wadl`
//! ("Jira 9.17.0", checksum-pinned): every path, verb and query parameter this
//! crate sends is declared there. `knobas-mockd` answers anything else with a
//! recorded violation, so an invented endpoint fails this crate's own suite.

mod config;
mod descriptor;
mod http;
mod map;
mod model;
mod time;

pub use config::{Flavor, JiraConfig};
pub use descriptor::descriptor_template;

/// The adapter kind: `SourceDescriptor::adapter_kind`, and the default
/// instance id offered by the Add-source form.
pub const ADAPTER_KIND: &str = "jira";

/// This adapter's own version, reported in the descriptor and in the
/// `User-Agent` (interfaces doc §4.1).
pub const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The one entity kind this adapter emits. Epics are Jira issues of type
/// `Epic`, so they arrive as tickets too (interfaces doc §4.2: kinds = `ticket`).
pub const KIND_TICKET: &str = "ticket";
