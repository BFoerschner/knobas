//! The Uptime Kuma adapter: `/metrics` with an API key, read-only.
//!
//! Monitors arrive as items of kind [`KIND_MONITOR`], each carrying Kuma's own
//! monitor id, name, type, address, current state, response time, uptime ratios
//! and certificate countdown (issue #442). Uptime Kuma is a source like Jira or
//! Gitea and gets no special treatment anywhere: it has a room in the switcher
//! because it is a source, and it has no project rooms because it declares no
//! projects (`CONTEXT.md`, **Monitor**; spec #427, *The Kuma room and tiles*).
//!
//! # Which Kuma this speaks, and through which door
//!
//! **Uptime Kuma v2, through `/metrics`.** There is no other door: v2 has no
//! REST API for reading monitors -- the dashboard is socket.io, and an API key
//! authenticates exactly one HTTP endpoint. So the read path is a Prometheus
//! scrape ([`metrics`]), and what a monitor *is* here is what four gauge
//! families with the same `monitor_id` add up to ([`model`]).
//!
//! Three consequences a reader should have in hand before the code:
//!
//! * **There is no clock in the document.** No timestamps, no paging, no
//!   "changed since". Every run reads the whole roster, every item is undated,
//!   and the cursor is a digest of the last corpus rather than a position
//!   ([`cursor`]).
//! * **A paused monitor is not in `/metrics` at all**, so through this channel
//!   *paused* and *deleted* are one observation. Measured, not assumed; see
//!   [`cursor`].
//! * **Writing is a different channel, and a different credential.** Pause and
//!   resume are socket.io, which an API key cannot log in to at all -- so a
//!   source configured with only a key declares no write ops and refuses every
//!   one by name, and a source whose keychain item also carries an *account*
//!   declares `pause_monitor`, `resume_monitor` and `create_monitor` and
//!   performs them over [`socket`] (issues #452 and #453).
//!
//! # Where the endpoint truth comes from
//!
//! The running container, and nothing else. Every shape this crate encodes --
//! the four families, the label set, `"null"` for an absent field, `-1` for a
//! response time that did not happen, the `/dashboard/:id` link, the `401` a
//! wrong key gets -- was read off the pinned image (2.5.3) on 2026-09-06 and is
//! re-asserted against the real server by `tests/live_kuma.rs`, which
//! `just kuma-live` runs (ADR-0013: the real container is the witness). The
//! wiremock stand-in in `tests/contract.rs` exists only so `just check` stays
//! docker-free; **if the two disagree, the stand-in is what is wrong.**

mod config;
mod create;
mod cursor;
mod descriptor;
mod http;
mod map;
mod metrics;
mod model;
mod socket;
mod source;

pub use config::KumaConfig;
pub use descriptor::descriptor_template;
pub use source::{KumaSource, build};

/// The adapter kind: `SourceDescriptor::adapter_kind`, and the default instance
/// id offered by the Add-source form.
///
/// `kuma` and not `uptime-kuma`: the instance id is the namespace of every
/// entity this source ever emits (`kuma:8`), it is typed by a human into the
/// Add-source form, and it is what the whole codebase already calls this source
/// -- the spec's *Kuma room*, the recipe `just kuma-live`, and the fixtures in
/// `crates/knobas-app/tests` that were written against a source id of `kuma`
/// before this crate existed. Both spellings are legal instance ids; this is
/// the one already in use.
pub const ADAPTER_KIND: &str = "kuma";

/// This adapter's own version, reported in the descriptor and in the
/// `User-Agent` (contract §4.1).
pub const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The one entity kind this adapter emits.
///
/// `monitor` is already knobas' own word -- `0001_init.sql`'s kind enumeration
/// names it, `CONTEXT.md` defines it, and migration `0020` keeps the monitor
/// *names* an estate file gives against the day this crate started emitting the
/// kind. So a Kuma monitor needs no new vocabulary anywhere downstream. Alerts
/// and samples are not kinds: an alert is a knobas-owned row with no entity at
/// all, and a sample is a row of the knobas-owned timeseries (spec #427).
pub const KIND_MONITOR: &str = "monitor";
