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
//! (branches and pulls), one commit page per branch that moved -- **and one
//! more for each of those walks**, the empty page it ends on, since nothing
//! here reads a page shorter than the `limit` it asked for as the end of a
//! listing (issue #81). Plus **one** per emitted pull request that has
//! comments: that endpoint is not paged, so however long the discussion there
//! is no second page to ask for (issue #131). Twenty repositories on the
//! default five-minute schedule is roughly eighty requests a run at 10 req/s.
//! The `owners`/`repos` allowlist is the lever when that is too much, and
//! `include_pr_comments` is the one that costs a discussion.
//!
//! **A refusal costs no extra request** (ADR-0004). It used to cost one
//! identity probe per skipped repository and per refused discussion, because
//! `knobas-http` collapsed 401 and 403 into one indistinguishable error and the
//! only way to tell a dead credential from a repository this token may not read
//! was to re-run `GET /user` and see. The status is carried now: 403 and 404
//! skip, 401 ends the run, and `client::is_repo_scoped` reads that off the
//! error.
//!
//! # Testing
//!
//! `cargo test -p knobas-source-gitea` needs no Docker: it runs against a
//! wiremock stand-in, contract battery included. **The stand-in is a
//! convenience, not the contract** -- this adapter's contract source is the
//! real pinned container (interfaces §4.2), and `tests/live_gitea.rs`
//! re-asserts every shape the fake encodes against it. If the two disagree, the
//! fake is what is wrong.
//!
//! Those tests are `#[ignore]`d so `just check` and CI stay docker-free
//! (roadmap §3). To run them:
//!
//! ```text
//! just gitea-live
//! ```
//!
//! or by hand, against an environment that is already up and seeded:
//!
//! ```text
//! cd testenv && docker compose up -d --wait gitea && ./seed-gitea.sh
//! eval "$(cd testenv && ./seed --env)"
//! cargo test -p knobas-source-gitea --test live_gitea -- --ignored --nocapture
//! ```

pub mod config;
pub mod keys;

mod client;
mod cursor;
mod map;
mod model;
mod source;
mod sync;
mod write;

pub use config::{GiteaConfig, config_schema};
pub use source::{GiteaSource, build};

use knobas_source::{
    AuthMethod, Capability, KindInfo, KindPaths, ListPath, PayloadPath, SourceDescriptor,
};

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
///
/// Each kind also declares whether a cursor-less run emits **its** complete
/// corpus, which is the engine's licence to tombstone what that kind stopped
/// returning (ADR-0003). This adapter splits two ways and is the reason the
/// declaration is per kind at all -- see
/// `tests::exactly_the_unbudgeted_kinds_are_exhaustive`, and `sync`'s module
/// docs for what makes the `true` half honest.
#[must_use]
pub fn entity_kinds() -> Vec<KindInfo> {
    vec![
        KindInfo {
            id: KIND_REPO.into(),
            label: "Repository".into(),
            plural: "Repositories".into(),
            monogram: "RE".into(),
            // The listing is walked to the end or the run fails, and no
            // configuration bounds it, so a cursor-less run emits every
            // repository in scope. This is what retires the row of a
            // repository that was deleted upstream.
            full_sync_exhaustive: true,
        },
        KindInfo {
            id: KIND_BRANCH.into(),
            label: "Branch".into(),
            plural: "Branches".into(),
            monogram: "BR".into(),
            // Same: every branch of every walked repository, unbounded. The
            // adapter's own tombstones need a previous cursor to diff against
            // and a full sync has none, so for the cursor-less case the
            // engine's sweep is the only thing that ever retires a deleted
            // branch -- and `branch` is precisely the kind links hang off
            // (spec §5a).
            full_sync_exhaustive: true,
        },
        KindInfo {
            id: KIND_PR.into(),
            label: "Pull request".into(),
            plural: "Pull requests".into(),
            monogram: "PR".into(),
            // `prs_per_repo` bounds what one run mirrors, cursor-less runs
            // included. A budgeted kind is non-exhaustive by definition.
            full_sync_exhaustive: false,
        },
        KindInfo {
            id: KIND_COMMIT.into(),
            label: "Commit".into(),
            plural: "Commits".into(),
            monogram: "CM".into(),
            // `commits_per_repo`, likewise.
            full_sync_exhaustive: false,
        },
    ]
}

/// The write ops this adapter declares, as `knobas_source::WriteOp`'s stable
/// identifiers. Named constants because the descriptor and the dispatch in
/// [`source`] must agree: an op listed and not dispatched is an action that
/// 404s, and one dispatched and not listed is an action nothing offers.
pub const WRITE_OP_CREATE_BRANCH: &str = "create_branch";
/// See [`WRITE_OP_CREATE_BRANCH`].
pub const WRITE_OP_CREATE_PULL_REQUEST: &str = "create_pull_request";
/// See [`WRITE_OP_CREATE_BRANCH`].
pub const WRITE_OP_COMMENT: &str = "comment";
/// See [`WRITE_OP_CREATE_BRANCH`].
pub const WRITE_OP_APPROVE: &str = "approve";

/// The descriptor `list_adapters` serves before any instance exists
/// (interfaces §2.2): `id == adapter_kind`, name = the product name.
#[must_use]
pub fn descriptor_template() -> SourceDescriptor {
    SourceDescriptor {
        id: ADAPTER_KIND.to_owned(),
        adapter_kind: ADAPTER_KIND.to_owned(),
        name: "Gitea".to_owned(),
        // M2 (issue #43): this adapter writes. `Capability::Search` stays
        // absent -- it is reserved for a server-side `Source::search` the SPI
        // does not have.
        capabilities: vec![Capability::Write],
        adapter_version: ADAPTER_VERSION.to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        // M2's ratified Gitea set (issue #43, ADR-0006), and the whole of what
        // the action bar offers. The battery holds this and `Capability::Write`
        // to each other in both directions.
        write_ops: vec![
            WRITE_OP_CREATE_BRANCH.to_owned(),
            WRITE_OP_CREATE_PULL_REQUEST.to_owned(),
            WRITE_OP_COMMENT.to_owned(),
            WRITE_OP_APPROVE.to_owned(),
        ],
        // Each kind carries its own `full_sync_exhaustive` -- see
        // `entity_kinds`, which is where the 2026-08-25 budget ruling and
        // ADR-0003 meet.
        entity_kinds: entity_kinds(),
        config_schema: config::config_schema(),
        payload_paths: payload_paths(),
    }
}

/// Where a Gitea record keeps what knobas reads (#277, ADR-0007).
///
/// A pull request only: a repository, a branch and a commit carry none of
/// these facts, and a kind that declares nothing is a miss for every reader.
///
/// * `reviewers` is Gitea's `requested_reviewers`, each element a user object
///   whose `login` is the account -- the same spelling `author` is filled from
///   (`map::pr_item`), which is what makes it comparable with the configured
///   identity. Gitea sends `null` there when nobody has been asked, and an
///   absent list is a rule that finds nothing rather than a rule that fails.
/// * `merged` is Gitea's boolean, which is what the start-work merge pass
///   follows. `merged_at` is the same fact as a timestamp and is deliberately
///   not a second candidate: a flag is a boolean here (see
///   [`knobas_source::KindPaths::merged`]).
///
/// **No project.** Gitea has no project grouping in knobas (ADR-0010 says so
/// in as many words: "Gitea has no such thing here and gets none"), and a
/// repository is an entity *kind*, not the axis a room is drawn along.
#[must_use]
fn payload_paths() -> Vec<KindPaths> {
    vec![KindPaths {
        kind: KIND_PR.to_owned(),
        reviewers: vec![ListPath {
            at: PayloadPath::of(["requested_reviewers"]),
            entry: PayloadPath::of(["login"]),
        }],
        merged: vec![PayloadPath::of(["merged"])],
        ..KindPaths::default()
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Add-source form, the launcher's chips and the action bar all read
    /// this one value.
    #[test]
    fn the_template_describes_a_gitea_that_writes() {
        let d = descriptor_template();
        assert_eq!(
            (d.id.as_str(), d.adapter_kind.as_str()),
            (ADAPTER_KIND, ADAPTER_KIND)
        );
        assert_eq!(d.name, "Gitea");
        assert_eq!(d.capabilities, vec![Capability::Write]);
        assert_eq!(
            d.write_ops,
            vec!["create_branch", "create_pull_request", "comment", "approve"],
            "the action bar is rendered from this list alone (issue #43)"
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

    /// **This adapter is why ADR-0003 exists**, and it carries both answers.
    ///
    /// `full_sync_exhaustive` means exactly "a cursor-less run emits the
    /// complete corpus of this kind". The repository listing and each walked
    /// repository's branch listing are walked to the end -- a reached page cap
    /// is fatal, and a repository skipped during a cursor-less run is fatal
    /// too (`sync`'s module docs), so a run that returns `Ok` without a cursor
    /// really did emit every repo and every branch. The other two kinds are
    /// bounded by `commits_per_repo` / `prs_per_repo`, and a budgeted kind is
    /// non-exhaustive by definition: `true` there would be a standing
    /// instruction to tombstone every commit past the cap on **every** full
    /// sync -- the same defect class as Jira's `MAX_PAGES`, reaching a
    /// different mechanism. That is the 2026-08-25 ruling, kept; what ADR-0003
    /// changed is that it no longer has to cost `repo` and `branch` too.
    ///
    /// This pins the *coupling* and not just four constants: the budgeted kinds
    /// are derived from the config schema, so renaming a budget fails here
    /// rather than passing vacuously, and a budget added for repositories or
    /// branches makes the two halves disagree instead of quietly licensing a
    /// sweep over a bounded walk.
    #[test]
    fn exactly_the_unbudgeted_kinds_are_exhaustive() {
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
        // `commits_per_repo` bounds `commit`, `prs_per_repo` bounds `pr`.
        let budgeted: Vec<&str> = budgets
            .iter()
            .map(|budget| budget.trim_end_matches("s_per_repo"))
            .collect();
        assert_eq!(budgeted, vec![KIND_COMMIT, KIND_PR]);

        let claims: Vec<(String, bool)> = descriptor_template()
            .entity_kinds
            .into_iter()
            .map(|kind| (kind.id, kind.full_sync_exhaustive))
            .collect();
        assert_eq!(
            claims,
            vec![
                (KIND_REPO.to_owned(), true),
                (KIND_BRANCH.to_owned(), true),
                (KIND_PR.to_owned(), false),
                (KIND_COMMIT.to_owned(), false),
            ]
        );
        for (kind, exhaustive) in &claims {
            assert_eq!(
                *exhaustive,
                !budgeted.contains(&kind.as_str()),
                "{kind:?} is exhaustive if and only if no {budgets:?} bounds it"
            );
        }
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
