//! The Jira **Data Center** adapter: `/rest/api/2/…`, `startAt` pagination.
//!
//! M2's ratified write-back set is here (issue #43): a ticket can be moved to
//! another status, replied to, and created. Everything else
//! [`Source::write`](knobas_source::Source::write) is handed is refused, which
//! is the SPI's rule and what the contract battery checks. **No write reaches
//! this adapter except through the write queue** -- `knobas_sync::write_queue`
//! is knobas' one outbound write path (issue #42).
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

mod api;
mod config;
mod cursor;
mod descriptor;
mod discover;
mod fingerprint;
mod http;
mod jql;
mod map;
mod model;
mod source;
mod sync;
mod time;
mod write;

pub use config::{Flavor, JiraConfig};
pub use descriptor::descriptor_template;
pub use source::{JiraSource, build};

/// The adapter kind: `SourceDescriptor::adapter_kind`, and the default
/// instance id offered by the Add-source form.
pub const ADAPTER_KIND: &str = "jira";

/// This adapter's own version, reported in the descriptor and in the
/// `User-Agent` (interfaces doc §4.1).
pub const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The write ops this adapter declares, as `knobas_source::WriteOp`'s stable
/// identifiers. Named constants rather than literals because the descriptor
/// and the dispatch in [`source`] must agree, and a typo in either is an
/// action the UI offers and the adapter refuses.
pub const WRITE_OP_COMMENT: &str = "comment";
/// See [`WRITE_OP_COMMENT`].
pub const WRITE_OP_TRANSITION: &str = "transition";
/// See [`WRITE_OP_COMMENT`].
pub const WRITE_OP_CREATE_TICKET: &str = "create_ticket";
/// See [`WRITE_OP_COMMENT`]. M3.1's growth (issue #280): the only adapter that
/// declares it, because it is the only source knobas mirrors that holds
/// worklogs.
pub const WRITE_OP_LOG_WORK: &str = "log_work";

/// The one entity kind this adapter emits. Epics are Jira issues of type
/// `Epic`, so they arrive as tickets too (interfaces doc §4.2: kinds = `ticket`).
pub const KIND_TICKET: &str = "ticket";
