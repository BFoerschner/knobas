//! The TeamCity adapter: build configurations and builds, read-only (M1).
//!
//! Five endpoints, all JSON, all trimmed with an explicit `fields=`:
//! `/app/rest/server` and `/app/rest/users/current` (test connection),
//! `/app/rest/buildTypes` (the scope and the `build_config` items),
//! `/app/rest/builds` with three locator shapes -- one page of any state to
//! open the run, finished-since-watermark, and an unconditional
//! `state:(queued:true,running:true)` poll, because a running build mutates in
//! place without ever getting a new id -- and `/app/rest/builds/id:{id}`,
//! asked once in the rare run whose opening pages do not reach the watermark
//! and answered by its **status**, where a 404 is the one thing that proves
//! the server is not the one this source's cursor came from.
//!
//! Every request sends `Accept: application/json`; without it real TeamCity
//! answers XML, and `knobas-mockd` answers 406 + `X-Mockd-Hint` (interfaces
//! §5, P11).
//!
//! # What this adapter does *not* claim
//!
//! [`KindInfo::full_sync_exhaustive`](knobas_source::KindInfo::full_sync_exhaustive)
//! is `false` on **both** kinds this adapter emits. A full sync fetches the
//! newest `builds_per_config` finished builds per configuration, which is a
//! window over the build history rather than the whole of it, so the engine's
//! tombstone sweep must not run after one. This adapter is the reason the flag
//! exists; ADR-0003 moved it onto the kind, for adapters that need to answer
//! both ways at once, and left TeamCity's answer where it was.

mod client;
mod config;
mod cursor;
mod descriptor;
mod http;
mod map;
mod rest;
mod source;
mod sync;

pub use config::{TeamCityConfig, config_schema};
pub use descriptor::descriptor_template;
pub use source::{TeamCitySource, build};

/// This adapter's kind, the default instance id offered by the Add-source
/// form, and therefore the default
/// [`EntityRef`](knobas_core::entity::EntityRef) namespace of everything it
/// emits.
pub const ADAPTER_KIND: &str = "teamcity";

/// This adapter's own version, reported in the descriptor and in the
/// `User-Agent` (interfaces §4.1).
pub const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// `SyncItem::kind` for one build.
pub(crate) const KIND_BUILD: &str = "build";
/// `SyncItem::kind` for one build configuration.
pub(crate) const KIND_BUILD_CONFIG: &str = "build_config";
