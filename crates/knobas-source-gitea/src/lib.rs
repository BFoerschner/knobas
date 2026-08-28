//! The Gitea adapter: repositories, branches, pull requests and commits, read
//! only (interfaces §4.1: M1 is read-only toward every source).
//!
//! # Why the client is hand-rolled
//!
//! The roadmap permits generating this client from the instance's OpenAPI
//! document. It is hand-rolled instead. Six endpoints are read here and about
//! six more will be written in M2 -- a generated client is 340 paths of surface
//! for twelve of use, and it fights three things this adapter does not get to
//! choose: the shared reqwest + retry + rate-limit stack every adapter routes
//! through (interfaces §4.1, ruling P8), the exact status-to-`SourceError`
//! classification the contract battery checks in both directions, and the
//! promise that `SyncItem::payload` is the source's record *verbatim* (spec
//! §3a) -- a generated model is a lossy projection, so the payload would have
//! to be re-serialised from types instead of kept. A generated client also
//! pins its own transitive reqwest, which is the version split the roadmap
//! rejects `tauri-plugin-http` for. The `gitea-sdk` crate is a third-party 0.x
//! wrapper with the same transitive-reqwest problem and no bearing on the
//! contract. Spec fidelity is bought the stronger way instead:
//! `tests/live_gitea.rs` runs against the real pinned container, which is a
//! stricter check than conformance to a document.
//!
//! # What one run costs
//!
//! Gitea's branch listing has no incremental filter, so a run costs one
//! identity request, `ceil(repos / 50)` listing requests, two per repository
//! (branches and pulls), one commit page per branch that moved, and one more
//! per emitted pull request that has comments. Twenty repositories on the
//! default five-minute schedule is roughly forty requests a run at 10 req/s.
//! The `owners`/`repos` allowlist is the lever when that is too much.
//!
//! **Plus one identity request per repository this run *skips*.** A refusal is
//! believed only while the credential is provably still good, because
//! `knobas-http` collapses 401 and 403 into one error
//! (`sync::credential_still_good`). A healthy run pays nothing for this; a
//! source whose token is scoped away from one of its repositories pays one
//! extra request on every incremental run, for as long as that stays true.

pub mod config;
pub mod keys;

mod client;
mod cursor;
mod map;
mod model;
mod source;
mod sync;

pub use config::{GiteaConfig, config_schema};
pub use source::{GiteaSource, build};

use knobas_source::{AuthMethod, KindInfo, SourceDescriptor};

/// The adapter kind, and the default instance id (interfaces §4.2).
pub const ADAPTER_KIND: &str = "gitea";

/// This adapter's own version, reported in the descriptor and in the
/// `User-Agent` (interfaces §4.1).
pub const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const KIND_REPO: &str = "repo";
pub const KIND_BRANCH: &str = "branch";
pub const KIND_PR: &str = "pr";
pub const KIND_COMMIT: &str = "commit";

/// The kinds this adapter emits, with the display metadata the launcher renders
/// groups, chips and monograms from (spec §3a -- nothing downstream carries a
/// per-adapter table). Monograms are fixed by interfaces §4.2.
#[must_use]
pub fn entity_kinds() -> Vec<KindInfo> {
    vec![
        KindInfo {
            id: KIND_REPO.into(),
            label: "Repository".into(),
            plural: "Repositories".into(),
            monogram: "RE".into(),
            full_sync_exhaustive: false,
        },
        KindInfo {
            id: KIND_BRANCH.into(),
            label: "Branch".into(),
            plural: "Branches".into(),
            monogram: "BR".into(),
            full_sync_exhaustive: false,
        },
        KindInfo {
            id: KIND_PR.into(),
            label: "Pull request".into(),
            plural: "Pull requests".into(),
            monogram: "PR".into(),
            full_sync_exhaustive: false,
        },
        KindInfo {
            id: KIND_COMMIT.into(),
            label: "Commit".into(),
            plural: "Commits".into(),
            monogram: "CM".into(),
            full_sync_exhaustive: false,
        },
    ]
}

/// The descriptor `list_adapters` serves before any instance exists
/// (interfaces §2.2): `id == adapter_kind`, name = the product name.
#[must_use]
pub fn descriptor_template() -> SourceDescriptor {
    SourceDescriptor {
        id: ADAPTER_KIND.to_owned(),
        adapter_kind: ADAPTER_KIND.to_owned(),
        name: "Gitea".to_owned(),
        // Ruling P12: M1 adapters declare none. `Capability::Write` in
        // particular must stay absent while `write_ops` is empty -- the battery
        // enforces both directions.
        capabilities: Vec::new(),
        adapter_version: ADAPTER_VERSION.to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        write_ops: Vec::new(),
        // Each kind carries its own `full_sync_exhaustive` -- see
        // `entity_kinds`, which is where the 2026-08-25 budget ruling and
        // ADR-0003 meet.
        entity_kinds: entity_kinds(),
        config_schema: config::config_schema(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Add-source form, the launcher's chips and the read-only promise all
    /// read this one value.
    #[test]
    fn the_template_describes_a_read_only_gitea() {
        let d = descriptor_template();
        assert_eq!(
            (d.id.as_str(), d.adapter_kind.as_str()),
            (ADAPTER_KIND, ADAPTER_KIND)
        );
        assert_eq!(d.name, "Gitea");
        assert!(
            d.capabilities.is_empty(),
            "ruling P12: M1 adapters declare no capabilities"
        );
        assert!(
            d.write_ops.is_empty(),
            "interfaces §4.1: M1 is read-only toward every source"
        );
        assert_eq!(d.auth_methods, vec![AuthMethod::Pat]);
        let kinds: Vec<&str> = d.entity_kinds.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(kinds, vec![KIND_REPO, KIND_BRANCH, KIND_PR, KIND_COMMIT]);
        for k in &d.entity_kinds {
            assert_eq!(
                k.monogram.chars().count(),
                2,
                "{:?} needs a two-character monogram",
                k.id
            );
        }
        assert_eq!(d.config_schema["type"], "object");
    }

    /// **The 2026-08-25 ruling** (carry-overs, *"BLOCKING before stream B's
    /// adapter merges"*), which supersedes interfaces §4.2's `true` for Gitea.
    ///
    /// `full_sync_exhaustive` means exactly "a cursor-less run emits the
    /// complete corpus", and this adapter's configuration bounds what one run
    /// mirrors per repository. The engine sweeps on
    /// `full_sync && exhaustive && upserted > 0`
    /// (`knobas_sync::run_inner` → `sweep`), so `true` alongside a budget is a
    /// standing instruction to tombstone every commit past the cap on **every**
    /// full sync -- the same defect class as Jira's `MAX_PAGES`, reaching a
    /// different mechanism.
    ///
    /// This pins the coupling and not just the constant: it names the budgets
    /// the ruling is about, so renaming one fails here rather than passing
    /// vacuously, and if every budget is ever removed this test is what has to
    /// be revisited before `true` can come back.
    #[test]
    fn a_budgeted_full_sync_is_not_exhaustive() {
        let schema = config::config_schema();
        let budgets: Vec<&str> = schema["properties"]
            .as_object()
            .expect("config_schema has an object of properties")
            .keys()
            .filter(|key| key.ends_with("_per_repo"))
            .map(String::as_str)
            .collect();
        assert_eq!(
            budgets,
            vec!["commits_per_repo", "prs_per_repo"],
            "the per-repository budgets the ruling is about"
        );
        assert!(
            !descriptor_template()
                .entity_kinds
                .iter()
                .any(|k| k.full_sync_exhaustive),
            "{budgets:?} bound what a cursor-less run emits, so the run is not \
             exhaustive and the engine must not sweep after it"
        );
    }

    /// The whole descriptor crosses the IPC bridge as plain data (spec §3a).
    #[test]
    fn the_template_round_trips_as_json() {
        let json = serde_json::to_value(descriptor_template()).unwrap();
        let back: SourceDescriptor = serde_json::from_value(json).unwrap();
        assert_eq!(back.entity_kinds.len(), 4);
        assert_eq!(back.config_schema, config::config_schema());
    }
}
