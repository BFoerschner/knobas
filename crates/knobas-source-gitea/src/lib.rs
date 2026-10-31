//! The Gitea adapter: repositories, branches, pull requests and commits, and
//! since #1206 its issues, as tickets. It fills two roles (ADR-0029): the code
//! host and the ticket system.
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
//! identity request, `ceil(repos / 50)` listing requests, and per repository
//! five on an idle run: branches, pulls and issues (#1206), and the empty page the
//! branch and issue walks end on, since nothing here reads a page shorter
//! than the `limit` it asked for as the end of a listing (issue #81). On top
//! of that: the repository's teams when its record changed (#966), one commit
//! page per branch that moved and the empty page after it, one more page per
//! further page of pulls or issues that changed, and **one** per emitted pull
//! request that has comments: that endpoint is not paged, so however long the
//! discussion there is no second page to ask for (issue #131). An idle run
//! over 300 repositories is therefore about 1,500 requests.
//!
//! **What bounds the time is the rate, then the round trip.** Every request
//! goes through the per-origin limiter (`rate_limit_per_sec`, 10 req/s by
//! default), so those 1,500 take two and a half minutes at the default. Inside
//! one repository the walk is one request after another, and up to
//! `concurrent_repos` repositories (default 8) are walked side by side
//! (#2159), so a raised rate is not capped by one round trip at a time; at 1
//! the run walks them one after another. The `owners`/`repos` allowlist is the
//! lever when the request count is too much, and `include_pr_comments` is the
//! one that costs a discussion.
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
//! cd testenv && docker compose --env-file images.env up -d --wait gitea && ./seed-gitea.sh
//! eval "$(cd testenv && ./seed --env)"
//! cargo test -p knobas-source-gitea --test live_gitea -- --ignored --nocapture
//! ```

pub mod config;
pub mod keys;

mod client;
mod cursor;
mod fingerprint;
mod grants;
mod map;
mod model;
mod refresh;
mod source;
mod sync;
mod view;
mod write;

pub use config::{GiteaConfig, config_schema};
pub use source::{GiteaSource, build};

use knobas_source::{
    AuthMethod, Capability, KindInfo, KindPaths, ListPath, ParentPath, PayloadPath,
    SourceDescriptor, StatePath, StateValue, Tone,
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
/// An issue (#1206): the ticket-system role's kind, spelled as Jira's is, so
/// Tickets draws it beside Jira's.
pub const KIND_TICKET: &str = "ticket";

/// How the tree of a kind's view names its sections by path (#2298): the
/// repository's, a branch's and a commit's *Files* tab, `None` for a kind
/// that draws no tree. What the app carries on the entity's detail, so a deep
/// link's reads start before any listing has named them (#2244).
#[must_use]
pub fn tree_sections(kind: &str) -> Option<knobas_source::view::TreeSections> {
    [KIND_REPO, KIND_BRANCH, KIND_COMMIT]
        .contains(&kind)
        .then(view::tree_sections)
}

/// The kinds this adapter emits, with the display metadata the launcher renders
/// groups, chips and monograms from (spec §3a -- nothing downstream carries a
/// per-adapter table). Monograms are fixed by interfaces §4.2.
///
/// Each kind also declares whether a cursor-less run emits **its** complete
/// corpus, which is the engine's licence to tombstone what that kind stopped
/// returning (ADR-0003). This adapter splits two ways and is the reason the
/// declaration is per kind at all -- see
/// `tests::exactly_the_unbudgeted_code_host_kinds_are_exhaustive_and_the_ticket_is_not`, and `sync`'s module
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
            // `crate::refresh` reads a pull request and an issue by id, and
            // answers a repository, branch or commit not offered (#2146).
            refreshes: false,
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
            refreshes: false,
        },
        KindInfo {
            id: KIND_PR.into(),
            label: "Pull request".into(),
            plural: "Pull requests".into(),
            monogram: "PR".into(),
            // `prs_per_repo` bounds what one run mirrors, cursor-less runs
            // included. A budgeted kind is non-exhaustive by definition.
            full_sync_exhaustive: false,
            refreshes: true,
        },
        KindInfo {
            id: KIND_COMMIT.into(),
            label: "Commit".into(),
            plural: "Commits".into(),
            monogram: "CM".into(),
            // `commits_per_repo`, likewise.
            full_sync_exhaustive: false,
            refreshes: false,
        },
        KindInfo {
            id: KIND_TICKET.into(),
            label: "Issue".into(),
            plural: "Issues".into(),
            monogram: "IS".into(),
            // No budget bounds it, and still not exhaustive (#1206): a
            // repository whose issue listing is refused -- a token without
            // issue scope, the unit switched off -- is skipped rather than
            // failing the run, as its discussions are, so a cursor-less run
            // may not have seen every issue, and a sweep would retire tickets
            // nobody deleted. The cost is the budgeted kinds': an issue an
            // admin deleted stays in the mirror. A closed one is still listed
            // (`state=all`), so closing moves its state and never tombstones it.
            full_sync_exhaustive: false,
            refreshes: true,
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
/// See [`WRITE_OP_CREATE_BRANCH`]. Offered on a pull request's view only while
/// Gitea says it can be merged (issue #667).
pub const WRITE_OP_MERGE_PULL_REQUEST: &str = "merge_pull_request";
/// See [`WRITE_OP_CREATE_BRANCH`]. A review written from a pull request's
/// toolbar: *Approve…* and *Request changes…* (issue #705).
pub const WRITE_OP_SUBMIT_REVIEW: &str = "submit_review";
/// See [`WRITE_OP_CREATE_BRANCH`]. Offered on a branch's view: absent on the
/// default branch, disabled on a protected one (issue #707).
pub const WRITE_OP_DELETE_BRANCH: &str = "delete_branch";
/// See [`WRITE_OP_CREATE_BRANCH`]. A pull request's *Edit…*, *Close* and
/// *Reopen*, one `PATCH` (issue #706).
pub const WRITE_OP_EDIT_PULL_REQUEST: &str = "edit_pull_request";
/// See [`WRITE_OP_CREATE_BRANCH`]. *Update branch…* on a pull request whose
/// head is behind its base (issue #708).
pub const WRITE_OP_UPDATE_PULL_REQUEST_BRANCH: &str = "update_pull_request_branch";
/// See [`WRITE_OP_CREATE_BRANCH`]. *Commit change…* on a file, through the
/// contents API and never the checkout (spec #808 D4, issue #822).
pub const WRITE_OP_COMMIT_FILE: &str = "commit_file";
/// See [`WRITE_OP_CREATE_BRANCH`]. An issue's *Move to…*: `open` or
/// `closed`, the two states Gitea has (#1206).
pub const WRITE_OP_TRANSITION: &str = "transition";
/// See [`WRITE_OP_CREATE_BRANCH`]. A new issue in a repository (#1206).
pub const WRITE_OP_CREATE_TICKET: &str = "create_ticket";
/// See [`WRITE_OP_CREATE_BRANCH`]. *Reply…* on a review thread of a pull
/// request's diff (#2277): a comment-only review with one comment on the
/// thread's line, which Gitea joins to the conversation there.
pub const WRITE_OP_REPLY_TO_THREAD: &str = "reply_to_thread";
/// See [`WRITE_OP_CREATE_BRANCH`]. *Resolve* and *Unresolve* on a review
/// thread (#2277). Declared for every server this adapter speaks to, and
/// offered on a diff only where the server resolves threads: Gitea does,
/// Forgejo 17 has no endpoint for it and answers 405, so on Forgejo the diff
/// offers no resolve ([`source::version_resolves`]) and a write that reaches
/// it anyway is refused with that sentence.
pub const WRITE_OP_RESOLVE_THREAD: &str = "resolve_thread";
/// See [`WRITE_OP_CREATE_BRANCH`]. *Request…* in a pull request's Reviewers
/// panel (#2303): `POST …/pulls/{n}/requested_reviewers`.
pub const WRITE_OP_REQUEST_REVIEWERS: &str = "request_reviewers";

/// This adapter's one [source action](knobas_source::WriteOp::SourceAction)
/// (#1194, ADR-0029): *Add label…* on a pull request, a write no role's
/// vocabulary has. Declared in the descriptor's `source_actions` and
/// dispatched by [`source`], which refuses any other id.
pub const SOURCE_ACTION_ADD_LABEL: &str = "add_label";

/// The field *Add label…*'s dialog asks under and its dispatch reads: the
/// picked label's id.
pub(crate) const ADD_LABEL_FIELD: &str = "label";

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
        // does not have. It reads a pull request and an issue by id
        // (`Capability::Refresh`, #2098), and no other kind.
        capabilities: vec![Capability::Write, Capability::Refresh],
        adapter_version: ADAPTER_VERSION.to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        // One credential. The second one of envelope version 2 is Uptime
        // Kuma's alone (issue #452), and a form that offered account
        // fields here would be asking for something nothing reads.
        accepts_account: false,
        min_sync_interval_secs: knobas_source::DEFAULT_MIN_SYNC_INTERVAL_SECS,
        // The code host (#1193, ADR-0029). Its views draw a pull request's
        // diff with its line threads, a commit's diff, and a repository's tree
        // and files. Its issues are the ticket system's tickets (#1206), and
        // their workflow is Gitea's two states.
        roles: vec![
            knobas_source::role::RoleClaim::new(
                knobas_source::role::CODE_HOST,
                [
                    "pull_request_diff",
                    "commit_diff",
                    "repository_tree",
                    "file_contents",
                    "line_threads",
                    "clone_urls",
                ],
            ),
            knobas_source::role::RoleClaim::new(
                knobas_source::role::TICKET_SYSTEM,
                ["reachable_transitions"],
            ),
        ],
        // M2's ratified Gitea set (issue #43, ADR-0006) and the merge the
        // source views added (issue #667, §10.8), the review and the edit the
        // git workstation added (issues #705, #706, §10.8), and the whole of what the
        // action bar offers. The battery holds this and `Capability::Write`
        // to each other in both directions.
        write_ops: vec![
            WRITE_OP_CREATE_BRANCH.to_owned(),
            WRITE_OP_CREATE_PULL_REQUEST.to_owned(),
            WRITE_OP_COMMENT.to_owned(),
            WRITE_OP_APPROVE.to_owned(),
            WRITE_OP_MERGE_PULL_REQUEST.to_owned(),
            WRITE_OP_SUBMIT_REVIEW.to_owned(),
            WRITE_OP_DELETE_BRANCH.to_owned(),
            WRITE_OP_EDIT_PULL_REQUEST.to_owned(),
            WRITE_OP_UPDATE_PULL_REQUEST_BRANCH.to_owned(),
            WRITE_OP_COMMIT_FILE.to_owned(),
            WRITE_OP_TRANSITION.to_owned(),
            WRITE_OP_CREATE_TICKET.to_owned(),
            knobas_source::SOURCE_ACTION.to_owned(),
            WRITE_OP_REPLY_TO_THREAD.to_owned(),
            WRITE_OP_RESOLVE_THREAD.to_owned(),
            WRITE_OP_REQUEST_REVIEWERS.to_owned(),
        ],
        // Writes outside every role's vocabulary (#1194, ADR-0029), each
        // drawn by the view that offers it.
        source_actions: vec![SOURCE_ACTION_ADD_LABEL.to_owned()],
        // The code host's clone URLs (#1197): a repository is cloned from the
        // base URL a person typed, which is what *Clone* built from before
        // the read existed (ADR-0023).
        clone_from: vec![knobas_source::clone_urls::CloneFrom::BaseUrl],
        url_forms: Vec::new(),
        // Measured against the testenv's Gitea 1.27.3 (#2274): `/src/<ref>/`
        // answers a branch or a sha (a 303 to `/src/branch/` or
        // `/src/commit/`), and compare takes `<sha>^` as written or encoded.
        code_pages: Some(knobas_source::code_pages::CodePages {
            repository: "/{repo}".to_owned(),
            file: "/{repo}/src/{ref}/{file}#L{line}".to_owned(),
            commit: "/{repo}/commit/{sha}".to_owned(),
            range: "/{repo}/compare/{base}...{head}".to_owned(),
            pull_request: "/{repo}/pulls/{pr}".to_owned(),
        }),
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
/// A pull request's review facts, every child kind's parent, and a
/// repository's language, teams and owner. A kind that declares nothing is a miss
/// for every reader.
///
/// * `language` is a repository's Gitea `language`: the main language of its
///   default branch, as Gitea's own language statistics work it out, and
///   `""` where it found none (#966).
/// * `teams` are the organization teams a repository was added to, each a
///   Gitea team record whose `name` is the team. Gitea's repository record
///   names no team, so `map` writes them beside it from
///   `/repos/{owner}/{repo}/teams`, less a team that holds every repository
///   of its organization (#966).
/// * `owner` is a repository's Gitea `owner.login`: the organization or user
///   it sits under, which a set's repository row draws where the repository
///   is in no team of its own (#1020).
///
/// * `reviewers` is Gitea's `requested_reviewers`, each element a user object
///   whose `login` is the account -- the same spelling `author` is filled from
///   (`map::pr_item`), which is what makes it comparable with the configured
///   identity. Gitea sends `null` there when nobody has been asked, and an
///   absent list is a rule that finds nothing rather than a rule that fails.
/// * `merged` is Gitea's boolean, which is what the start-work merge pass
///   follows. `merged_at` is the same fact as a timestamp and is deliberately
///   not a second candidate: a flag is a boolean here (see
///   [`knobas_source::KindPaths::merged`]). No path-driven read wants a pull
///   request's opening or merge time since the conversation went
///   comments-only (#2164), so neither is declared (#2173); the pull-request
///   view reads `merged_at` from its own fetch.
///
/// * `parent` is the repo, for a branch, a pull request and a commit
///   (ADR-0018, #595). It answers to `full_name` first -- the name a path line
///   shows -- and to its bare `name`, so `under:tidewater/payout-service` and
///   `under:payout-service` both reach it. A pull request reads Gitea's own
///   `base.repo`; a branch's and a commit's records name no repository, so
///   `map` writes one into the stored payload at `repository`, spelled the
///   way `base.repo` is. A repository has no parent.
///
/// * `head` is the branch a pull request is opened from, Gitea's `head.ref`:
///   the bare branch name, which is the title `map::branch_item` gives a
///   branch. The Code tile reads it to tell a branch nobody has proposed yet
///   from one an open pull request already stands for (#712).
///
/// * `commit` is the commit each code record stands at, by full sha (#710):
///   a pull request's `head.sha`, a branch's tip `commit.id` (the
///   `/branches` record's own spelling), a commit's `sha`. The detail asks
///   the mirror for the builds whose declared `revision` is it.
///
/// * `head_repo` is the repository a pull request is opened from, Gitea's
///   `head.repo.full_name`: a fork's for a fork pull request, which is why
///   the head is matched there and not on the base repo (#718). A deleted
///   fork has no `head.repo`, and its pull request proposes no branch.
///
/// * `default_branch` is a branch's repository's default branch. Gitea's
///   branch record does not say, so `map` writes the repository's
///   `default_branch` into the `repository` object it already writes, and a
///   branch titled so is the default (#718). On a repository it is the
///   record's own `default_branch`, which its detail carries to the
///   repository page (#2307).
///
/// **No project for the code kinds.** Gitea has no project grouping in knobas
/// (ADR-0010 says so in as many words: "Gitea has no such thing here and gets
/// none"), and a repository is an entity *kind*, not the axis a room is drawn
/// along -- and it is a parent, which is not a project either (ADR-0018).
///
/// **A ticket's project is its repository** (#1206): the ticket system's
/// minimum requires a project (ADR-0029), and the ticket names the repository
/// as Gitea's. Only the ticket declares one; [`ticket_paths`] has the rest.
#[must_use]
fn payload_paths() -> Vec<KindPaths> {
    let repo = |at: &[&str]| {
        vec![ParentPath {
            kind: KIND_REPO.to_owned(),
            at: PayloadPath::of(at.iter().copied()),
            names: vec![PayloadPath::of(["full_name"]), PayloadPath::of(["name"])],
        }]
    };
    vec![
        KindPaths {
            kind: KIND_REPO.to_owned(),
            language: vec![PayloadPath::of(["language"])],
            teams: vec![ListPath {
                at: PayloadPath::of(["teams"]),
                entry: PayloadPath::of(["name"]),
            }],
            owner: vec![PayloadPath::of(["owner", "login"])],
            // The repository record's own field (#2307), as a branch's row
            // carries it under `repository`: what the repository page draws
            // its tabs on before the branch list answers.
            default_branch: vec![PayloadPath::of(["default_branch"])],
            ..KindPaths::default()
        },
        KindPaths {
            kind: KIND_BRANCH.to_owned(),
            parent: repo(&["repository"]),
            commit: vec![PayloadPath::of(["commit", "id"])],
            default_branch: vec![PayloadPath::of(["repository", "default_branch"])],
            ..KindPaths::default()
        },
        KindPaths {
            kind: KIND_PR.to_owned(),
            reviewers: vec![ListPath {
                at: PayloadPath::of(["requested_reviewers"]),
                entry: PayloadPath::of(["login"]),
            }],
            merged: vec![PayloadPath::of(["merged"])],
            parent: repo(&["base", "repo"]),
            state: pr_state(),
            head: vec![PayloadPath::of(["head", "ref"])],
            // The branch it is opened into (#1227), as `head` names the one it
            // is opened from: what the code-host minimum requires of both.
            base: vec![PayloadPath::of(["base", "ref"])],
            commit: vec![PayloadPath::of(["head", "sha"])],
            head_repo: vec![PayloadPath::of(["head", "repo", "full_name"])],
            ..KindPaths::default()
        },
        KindPaths {
            kind: KIND_COMMIT.to_owned(),
            parent: repo(&["repository"]),
            commit: vec![PayloadPath::of(["sha"])],
            ..KindPaths::default()
        },
        ticket_paths(),
    ]
}

/// Where an issue keeps what the ticket system reads (#1206, ADR-0029).
///
/// The issue record as Gitea's `/issues` listing serves it, plus a `knobas`
/// object `map::ticket_item` writes beside it for the three fields Gitea has
/// no field for:
///
/// * **status** is Gitea's `state`, `open` or `closed`: the workflow
///   `reachable_transitions` answers and `transition` moves.
/// * **priority** is the value of a scoped label `Priority/<value>`, the
///   spelling of Gitea's own *Default* label set (`Priority/High`), matched
///   case-insensitively on the scope. An issue with none has no priority:
///   `knobas.priority` is `null`, which every reader reads as a miss.
/// * **type** and **parent** are always `null` in `knobas`: Gitea issues
///   have neither. Declared so the role's minimum is met, and a miss for
///   every reader. A ticket answers as a parent to `knobas.key`, its own
///   key (`owner/repo/issues/7`), which no Gitea record names today.
/// * **assignee** is `assignee.login`, `null` when nobody is assigned.
/// * **labels** are the label records' names.
/// * **project** is the repository the issue is in: `repository.full_name`
///   as its key, the bare `name` as its name.
/// * **comment count** is Gitea's `comments`, the card's *N comments*.
///
/// **Done-like is `closed`** (#1501, #1593): an issue's declared status is its
/// `state`, `open` or `closed`, and `closed` is the word for done, so a job on
/// a closed issue has its context archived. There is no blocked-like set: Gitea
/// has no such state.
fn ticket_paths() -> KindPaths {
    let knobas = |key: &str| vec![PayloadPath::of(["knobas", key])];
    KindPaths {
        kind: KIND_TICKET.to_owned(),
        status_name: vec![PayloadPath::of(["state"])],
        priority: knobas("priority"),
        issue_type: knobas("issue_type"),
        assignee: vec![PayloadPath::of(["assignee", "login"])],
        labels: vec![ListPath {
            at: PayloadPath::of(["labels"]),
            entry: PayloadPath::of(["name"]),
        }],
        parent: vec![ParentPath {
            kind: KIND_TICKET.to_owned(),
            at: PayloadPath::of(["knobas", "parent"]),
            names: vec![PayloadPath::of(["knobas", "key"])],
        }],
        project_key: vec![PayloadPath::of(["repository", "full_name"])],
        project_name: vec![PayloadPath::of(["repository", "name"])],
        comment_count: vec![PayloadPath::of(["comments"])],
        // The issue's body as written (#2308): Markdown, as Gitea renders it.
        description: vec![PayloadPath::of(["body"])],
        markup: knobas_source::Markup::Markdown,
        done_statuses: vec!["closed".to_owned()],
        ..KindPaths::default()
    }
}

/// A pull request's state chip (#666): `merged` when Gitea's merged flag is
/// true, else its `state` word, `open` or `closed`. The flag comes first
/// because Gitea says `closed` of a merged pull request too.
fn pr_state() -> Vec<StatePath> {
    vec![
        StatePath {
            at: PayloadPath::of(["merged"]),
            values: vec![StateValue::of(true, Tone::Ok).shown_as("merged")],
        },
        StatePath {
            at: PayloadPath::of(["state"]),
            values: vec![
                StateValue::of("open", Tone::Info),
                StateValue::of("closed", Tone::Muted),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #2298: the kinds whose view draws *Files* declare its scheme, and the
    /// rest none.
    #[test]
    fn the_kinds_with_a_tree_declare_its_sections_and_the_rest_none() {
        for kind in [KIND_REPO, KIND_BRANCH, KIND_COMMIT] {
            assert!(tree_sections(kind).is_some(), "{kind}");
        }
        for kind in [KIND_PR, KIND_TICKET, "build"] {
            assert_eq!(tree_sections(kind), None, "{kind}");
        }
    }

    /// #2307: a repository's row declares its default branch, as a branch's
    /// does, so the repository page reads it as a field.
    #[test]
    fn a_repository_declares_its_default_branch() {
        let template = descriptor_template();
        let repo = template
            .payload_paths
            .iter()
            .find(|paths| paths.kind == KIND_REPO)
            .expect("the repository declares paths");
        assert_eq!(
            repo.default_branch,
            vec![PayloadPath::of(["default_branch"])]
        );
    }

    /// A closed issue is done-like (#1593): its job context is archived while
    /// its `state` reads `closed`. The set is the ticket's alone, and `open`
    /// is not in it.
    #[test]
    fn a_closed_issue_is_declared_done_like() {
        let template = descriptor_template();
        let ticket = template
            .payload_paths
            .iter()
            .find(|paths| paths.kind == KIND_TICKET)
            .expect("the ticket kind declares its paths");
        assert_eq!(ticket.done_statuses, ["closed"]);
        assert_eq!(ticket.status_name, vec![PayloadPath::of(["state"])]);
        for paths in template
            .payload_paths
            .iter()
            .filter(|p| p.kind != KIND_TICKET)
        {
            assert!(
                paths.done_statuses.is_empty(),
                "{} is never done",
                paths.kind
            );
        }
    }

    /// A pull request's chip (#666): Gitea says `closed` of a merged pull
    /// request too, so the flag is what tells the two apart.
    #[test]
    fn a_pull_request_draws_merged_open_or_closed() {
        use knobas_source::StateChip;
        let state = descriptor_template()
            .payload_paths
            .into_iter()
            .find(|p| p.kind == KIND_PR)
            .map(|p| p.state)
            .unwrap_or_default();
        let chip =
            |payload: serde_json::Value| knobas_core::payload::resolve_state(&payload, &state);
        let drawn = |text: &str, tone: Tone| {
            Some(StateChip {
                text: text.to_owned(),
                tone,
            })
        };
        assert_eq!(
            chip(serde_json::json!({ "state": "closed", "merged": true })),
            drawn("merged", Tone::Ok)
        );
        assert_eq!(
            chip(serde_json::json!({ "state": "open", "merged": false })),
            drawn("open", Tone::Info)
        );
        assert_eq!(
            chip(serde_json::json!({ "state": "closed", "merged": false })),
            drawn("closed", Tone::Muted)
        );
    }

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
        assert_eq!(d.capabilities, vec![Capability::Write, Capability::Refresh]);
        assert_eq!(
            d.write_ops,
            vec![
                "create_branch",
                "create_pull_request",
                "comment",
                "approve",
                "merge_pull_request",
                "submit_review",
                "delete_branch",
                "edit_pull_request",
                "update_pull_request_branch",
                "commit_file",
                "transition",
                "create_ticket",
                "source_action",
                "reply_to_thread",
                "resolve_thread",
                "request_reviewers"
            ],
            "the action bar is rendered from this list alone (issue #43)"
        );
        assert_eq!(d.auth_methods, vec![AuthMethod::Pat]);
        let kinds: Vec<&str> = d.entity_kinds.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(
            kinds,
            vec![KIND_REPO, KIND_BRANCH, KIND_PR, KIND_COMMIT, KIND_TICKET]
        );
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
    fn exactly_the_unbudgeted_code_host_kinds_are_exhaustive_and_the_ticket_is_not() {
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
                (KIND_TICKET.to_owned(), false),
            ]
        );
        // The ticket is the one unbudgeted kind that is not exhaustive: its
        // listing may be refused per repository without failing the run
        // (`sync::issues`), which a budget-free walk of the code host's kinds
        // never is.
        for (kind, exhaustive) in claims.iter().filter(|(kind, _)| kind != KIND_TICKET) {
            assert_eq!(
                *exhaustive,
                !budgeted.contains(&kind.as_str()),
                "{kind:?} is exhaustive if and only if no {budgets:?} bounds it"
            );
        }
    }

    /// A branch, a pull request and a commit sit in a repo (ADR-0018, #595),
    /// and the repo answers to `owner/name` first -- the name a path line
    /// shows -- and to its bare `name`. A pull request reads Gitea's own
    /// `base.repo`; a branch and a commit read the `repository` object the
    /// adapter writes into their payloads (`map`). A repo has no parent.
    #[test]
    fn a_branch_pr_and_commit_declare_their_repo_as_parent() {
        let declared = |kind: &str| {
            descriptor_template()
                .payload_paths
                .into_iter()
                .find(|paths| paths.kind == kind)
                .map(|paths| paths.parent)
                .unwrap_or_default()
        };
        let repo = |at: &[&str]| {
            vec![ParentPath {
                kind: KIND_REPO.to_owned(),
                at: PayloadPath::of(at.iter().copied()),
                names: vec![PayloadPath::of(["full_name"]), PayloadPath::of(["name"])],
            }]
        };
        assert_eq!(declared(KIND_BRANCH), repo(&["repository"]));
        assert_eq!(declared(KIND_COMMIT), repo(&["repository"]));
        assert_eq!(declared(KIND_PR), repo(&["base", "repo"]));
        assert!(declared(KIND_REPO).is_empty());
    }

    /// A pull request names both its ends by branch name (#1227): the head it
    /// is opened from and the base it is opened into, Gitea's own `ref`s.
    #[test]
    fn a_pull_request_declares_its_head_and_base_branch() {
        let pr = descriptor_template()
            .payload_paths
            .into_iter()
            .find(|paths| paths.kind == KIND_PR)
            .expect("a pull request declares its paths");
        assert_eq!(pr.head, vec![PayloadPath::of(["head", "ref"])]);
        assert_eq!(pr.base, vec![PayloadPath::of(["base", "ref"])]);
    }

    /// The whole descriptor crosses the IPC bridge as plain data (spec §3a).
    #[test]
    fn the_template_round_trips_as_json() {
        let json = serde_json::to_value(descriptor_template()).unwrap();
        let back: SourceDescriptor = serde_json::from_value(json).unwrap();
        assert_eq!(back.entity_kinds.len(), 5);
        assert_eq!(back.config_schema, config::config_schema());
    }

    /// The role this adapter fills (#1193, ADR-0029), held to its minimum
    /// on the descriptor alone: the install-time check, which the battery
    /// repeats as its clause 10.
    #[test]
    fn the_template_claims_the_code_host_and_ticket_system_roles_and_fills_them() {
        let d = descriptor_template();
        let roles: Vec<&str> = d.roles.iter().map(|c| c.role.as_str()).collect();
        assert_eq!(
            roles,
            [
                knobas_source::role::CODE_HOST,
                knobas_source::role::TICKET_SYSTEM
            ]
        );
        assert_eq!(knobas_source::role::check(&d), Ok(()));
    }
}
