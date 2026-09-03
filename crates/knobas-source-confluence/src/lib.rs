//! The Confluence **Data Center** adapter: `/rest/api/…`, CQL search,
//! `_links.next` pagination.
//!
//! Read-only (issue #284, M3.2 first half): pages arrive as items of kind
//! [`KIND_PAGE`], carrying the storage-format body verbatim, their comments,
//! their ancestors and their space. The write ops spec #272 lists for
//! Confluence -- `CreatePage`, `UpdatePage`, `Comment` -- are the next
//! ticket's, so [`Source::write`](knobas_source::Source::write) refuses
//! everything, which is the SPI's rule and what the contract battery checks.
//!
//! # Which Confluence this speaks
//!
//! Data Center / Server, REST **v1** (`/rest/api/...`). Cloud's v2 API
//! (`/wiki/api/v2/pages`, cursor paging, `atlassian.net` account ids) is a
//! different product surface, so the dialect is an explicit configuration
//! field ([`Flavor`]) refused by name rather than guessed at -- the same
//! device the Jira adapter uses for the same reason (roadmap §4 gotcha 4).
//!
//! # Where the endpoint truth comes from
//!
//! Nowhere but the server. Atlassian publishes no machine-readable Confluence
//! DC specification, which is exactly why ADR-0013 refused a mockd half for
//! it: a mock would have been this crate's assumptions checked against
//! themselves. **The witness is `tests/live_confluence_seeded.rs` against the
//! seeded container**, and every claim this crate's unit tests make about a
//! response shape is a claim that suite re-makes against Confluence itself.

mod api;
mod config;
mod cql;
mod cursor;
mod descriptor;
mod http;
mod map;
mod model;
mod source;
mod storage;
mod sync;
mod time;

pub use config::{ConfluenceConfig, Flavor};
pub use descriptor::descriptor_template;
pub use source::{ConfluenceSource, build};

/// The adapter kind: `SourceDescriptor::adapter_kind`, and the default
/// instance id offered by the Add-source form.
pub const ADAPTER_KIND: &str = "confluence";

/// This adapter's own version, reported in the descriptor and in the
/// `User-Agent` (contract §4.1).
pub const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The one entity kind this adapter emits.
///
/// `page` is already knobas' own word (migration `0001`'s kind enumeration and
/// the shell's *Docs* tile), so a Confluence page needs no new vocabulary
/// anywhere downstream. Blog posts, attachments and comments-as-items are
/// deliberately out of scope (spec #272, "Not in scope"): a comment belongs to
/// its page's payload, the way a Jira comment belongs to its issue's.
pub const KIND_PAGE: &str = "page";
