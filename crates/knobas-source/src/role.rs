//! Roles: what a source must provide for the shell to draw a surface for it
//! (ADR-0029, `CONTEXT.md` **Role**, issue #1193).
//!
//! A descriptor claims roles ([`SourceDescriptor::roles`]); each built-in
//! role is stated here as a [`Minimum`]: the kinds a source emits, the
//! declared paths each kind carries, the live reads it answers, and the
//! writes it may offer. Each write is optional and declared on its own, in
//! [`SourceDescriptor::write_ops`] as every write is. [`check`] holds a
//! descriptor's claims to the minimums, statically: it reads the descriptor
//! and nothing else, which is the install-time check of ADR-0028.
//! [`contract::battery`](crate::contract::battery) runs it as clause 10, and
//! its clause 6 then holds the declared paths to the adapter's own records.
//!
//! **Frozen once landed.** Changing a minimum, or adding a role, is a §10.8
//! entry (ADR-0029).
//!
//! A read is named by an identifier, like a write. Where the SPI has a
//! method for it the identifier is the method's name (`run_log`,
//! `reachable_transitions`); the code-host reads are answered by
//! [`Source::view`](crate::Source::view)'s blocks (a diff, a tree, a file)
//! and `doc_content` by the adapter's [`DocProjection`](crate::docs::DocProjection),
//! so for those the identifier is the claim that the adapter draws them.
//! `clone_urls` (#1197) is answered by the descriptor's
//! [`clone_from`](SourceDescriptor::clone_from): which typed address of the
//! instance a repository is cloned from (ADR-0023), which [`check`] holds to
//! the claim and to the config schema.

use knobas_spi_types::payload::{Frame, KindPaths};

use crate::SourceDescriptor;

pub mod data;

/// The code host: repositories, branches, pull requests and commits.
pub const CODE_HOST: &str = "code-host";
/// The ticket system: tickets and their workflow.
pub const TICKET_SYSTEM: &str = "ticket-system";
/// CI: ADR-0022's Builds frame.
pub const CI: &str = "ci";
/// The wiki: ADR-0022's Docs frame.
pub const WIKI: &str = "wiki";
/// Monitoring: monitors, whose samples knobas keeps.
pub const MONITORING: &str = "monitoring";

/// One role a source claims.
///
/// `role` is a built-in role ([`CODE_HOST`], [`TICKET_SYSTEM`], [`CI`],
/// [`WIKI`], [`MONITORING`]) or a **custom role** namespaced to the adapter
/// kind that declares it (`grafana:dashboard-host`). `reads` names the live
/// reads the source answers for the role: every read the role requires, and
/// whichever of its optional reads the source has. A custom role has no
/// reads of its own; its view is the `view` read.
///
/// Plain serde data, like everything crossing the SPI (spec §3a).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct RoleClaim {
    pub role: String,
    /// The live reads the source answers for the role, by identifier
    /// (`run_log`).
    #[serde(default)]
    pub reads: Vec<String>,
}

impl RoleClaim {
    /// `RoleClaim::new(CI, ["run_log"])`.
    pub fn new<'a>(role: &str, reads: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            role: role.to_owned(),
            reads: reads.into_iter().map(str::to_owned).collect(),
        }
    }
}

/// How a minimum names a kind: by its id, or by the [`Frame`] a kind declares
/// (TeamCity's `build_config` is a pipeline, Jira's `jira_view` a saved view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindOf {
    Id(&'static str),
    Frame(Frame),
}

/// A declared path a minimum requires: a field of [`KindPaths`], named as it
/// serializes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    StatusName,
    Priority,
    Assignee,
    IssueType,
    Labels,
    ProjectKey,
    /// Any parent.
    Parent,
    /// A parent of this kind: a branch's repo.
    ParentOf(&'static str),
    State,
    Reviewers,
    Merged,
    Head,
    /// The branch a pull request is opened into (#1227).
    Base,
    Commit,
    DefaultBranch,
    Name,
    Pipeline,
    Number,
    Branch,
    Started,
    Finished,
    /// A monitor's response time in milliseconds (#1224).
    ResponseTime,
    /// A saved view's query, in the source's own language (#2050).
    SourceQuery,
    /// A saved view's board type (#2050).
    BoardType,
    /// The query a saved view narrows its own with (#2050).
    SubQuery,
    /// A saved view's active sprint, an object (#2050).
    Sprint,
}

impl Field {
    /// The [`KindPaths`] field this is, as it serializes.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Field::StatusName => "status_name",
            Field::Priority => "priority",
            Field::Assignee => "assignee",
            Field::IssueType => "issue_type",
            Field::Labels => "labels",
            Field::ProjectKey => "project_key",
            Field::Parent | Field::ParentOf(_) => "parent",
            Field::State => "state",
            Field::Reviewers => "reviewers",
            Field::Merged => "merged",
            Field::Head => "head",
            Field::Base => "base",
            Field::Commit => "commit",
            Field::DefaultBranch => "default_branch",
            Field::Name => "name",
            Field::Pipeline => "pipeline",
            Field::Number => "number",
            Field::Branch => "branch",
            Field::Started => "started",
            Field::Finished => "finished",
            Field::ResponseTime => "response_time",
            Field::SourceQuery => "source_query",
            Field::BoardType => "board_type",
            Field::SubQuery => "sub_query",
            Field::Sprint => "sprint",
        }
    }

    /// Whether `paths` declares this field: at least one candidate, and for
    /// [`Field::ParentOf`] one whose parent is of that kind.
    #[must_use]
    pub fn declared_in(self, paths: &KindPaths) -> bool {
        !data::needs_of(self, paths).1.is_empty()
    }

    fn describe(self) -> String {
        match self {
            Field::ParentOf(kind) => format!("parent (a {kind:?})"),
            other => other.name().to_owned(),
        }
    }
}

/// One kind a role requires, the declared paths it must carry, and the ones
/// it may: what a reader of the kind reads, and nothing else (#2050).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindMinimum {
    pub kind: KindOf,
    pub paths: &'static [Field],
    /// The paths the role reads when the kind declares them, and does
    /// without when it does not: a saved view's board type, sub-query and
    /// sprint. Never refused for being missing; the battery's clause 6 still
    /// holds what is declared to the adapter's records.
    pub optional_paths: &'static [Field],
}

/// A built-in role's minimum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Minimum {
    pub role: &'static str,
    pub kinds: &'static [KindMinimum],
    /// The kinds a source filling this role may emit beside them (#1207): a
    /// ticket system's saved views. A source that emits one is held to its
    /// paths as to a required kind's; one that emits none still fills the
    /// role.
    pub optional_kinds: &'static [KindMinimum],
    /// The reads every claim of this role declares.
    pub reads: &'static [&'static str],
    /// The reads a claim may declare beside them.
    pub optional_reads: &'static [&'static str],
    /// The role's write vocabulary, as [`WriteOp`](crate::WriteOp)
    /// identifiers. Each is optional, and a source offers one by declaring
    /// it in `write_ops`.
    pub writes: &'static [&'static str],
}

const fn kind(id: &'static str, paths: &'static [Field]) -> KindMinimum {
    KindMinimum {
        kind: KindOf::Id(id),
        paths,
        optional_paths: &[],
    }
}

const fn frame(frame: Frame, paths: &'static [Field]) -> KindMinimum {
    KindMinimum {
        kind: KindOf::Frame(frame),
        paths,
        optional_paths: &[],
    }
}

/// The five built-in roles' minimums (spec #1192, ADR-0029).
pub const MINIMUMS: &[Minimum] = &[
    Minimum {
        role: CODE_HOST,
        kinds: &[
            kind("repo", &[]),
            kind(
                "branch",
                &[Field::ParentOf("repo"), Field::Commit, Field::DefaultBranch],
            ),
            kind(
                "pr",
                &[
                    Field::ParentOf("repo"),
                    Field::State,
                    Field::Head,
                    Field::Base,
                    Field::Commit,
                    Field::Reviewers,
                    Field::Merged,
                ],
            ),
            kind("commit", &[Field::ParentOf("repo"), Field::Commit]),
        ],
        optional_kinds: &[],
        reads: &[
            "pull_request_diff",
            "commit_diff",
            "repository_tree",
            "file_contents",
            "line_threads",
            "clone_urls",
        ],
        optional_reads: &[],
        writes: &[
            "create_branch",
            "create_pull_request",
            "comment",
            "approve",
            "submit_review",
            "merge_pull_request",
            "edit_pull_request",
            "update_pull_request_branch",
            "delete_branch",
            "commit_file",
            // A diff's threads and the Reviewers panel (spec #2234 step 6d,
            // #2277, #2303): typed, since the shell draws them itself.
            "reply_to_thread",
            "resolve_thread",
            "request_reviewers",
        ],
    },
    Minimum {
        role: TICKET_SYSTEM,
        kinds: &[kind(
            "ticket",
            &[
                Field::StatusName,
                Field::Priority,
                Field::Assignee,
                Field::IssueType,
                Field::Labels,
                Field::Parent,
                Field::ProjectKey,
            ],
        )],
        // Saved views (#1207, ADR-0029): Jira's boards and filters. The kind
        // is named by its frame, as CI's are, so the projection
        // (`knobas_sync::saved_views`) keys on the frame and not on an
        // adapter's kind id. It reads the view's query, board type, sub-query
        // and sprint at these declared paths and at no key of its own
        // choosing (#2050): the query is what the Tickets screen says decides
        // the view, so a saved-view kind must say where it is.
        optional_kinds: &[KindMinimum {
            optional_paths: &[Field::BoardType, Field::SubQuery, Field::Sprint],
            ..frame(Frame::SavedView, &[Field::SourceQuery])
        }],
        // Required of every ticket system, and answered with a refusal by
        // name by one that declares no `transition` (#1225): a read-only
        // ticket system has no moves to offer. The battery's clause 8 holds
        // both halves.
        reads: &["reachable_transitions"],
        // *Make an editable copy* of a saved view (#1208).
        optional_reads: &[TRANSLATE_QUERY],
        writes: &["transition", "comment", "create_ticket", "log_work"],
    },
    Minimum {
        role: CI,
        kinds: &[
            frame(Frame::Pipeline, &[Field::Name, Field::Pipeline]),
            frame(
                Frame::Run,
                &[
                    Field::Pipeline,
                    Field::State,
                    Field::Number,
                    Field::Branch,
                    Field::Started,
                    Field::Finished,
                ],
            ),
        ],
        optional_kinds: &[],
        reads: &["run_log"],
        optional_reads: &[
            "pipeline_settings",
            "older_runs",
            "run_tests",
            "test_history",
            "run_artifacts",
            "artifact_preview",
            "download",
            "run_chain",
        ],
        writes: &["run_build", "rerun_build", "trigger_build", "set_parameter"],
    },
    Minimum {
        role: WIKI,
        kinds: &[kind(
            crate::docs::PAGE_KIND,
            &[Field::ProjectKey, Field::Parent],
        )],
        optional_kinds: &[],
        reads: &["doc_content"],
        optional_reads: &[],
        writes: &["create_page", "update_page", "comment"],
    },
    Minimum {
        role: MONITORING,
        // The state and the response time its samples are taken from
        // (#1224, ADR-0029).
        kinds: &[kind("monitor", &[Field::StatusName, Field::ResponseTime])],
        optional_kinds: &[],
        reads: &[],
        optional_reads: &[],
        writes: &["create_monitor", "pause_monitor", "resume_monitor"],
    },
];

/// The minimum of a built-in role, `None` for any other name.
#[must_use]
pub fn minimum(role: &str) -> Option<&'static Minimum> {
    MINIMUMS.iter().find(|m| m.role == role)
}

/// Why a descriptor's role claims were refused. Each names the one item that
/// is missing or wrong, which is what an adapter author reads first.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClaimError {
    #[error(
        "role {role:?} is neither a built-in role (code-host, ticket-system, ci, wiki, \
         monitoring) nor a custom role named \"{adapter_kind}:<name>\""
    )]
    UnknownRole { role: String, adapter_kind: String },
    #[error("role {role:?} is claimed twice")]
    ClaimedTwice { role: String },
    #[error("role {role:?} needs kind {kind}, which descriptor.entity_kinds does not declare")]
    MissingKind { role: String, kind: String },
    #[error("role {role:?} needs kind {kind} to declare the path {path}")]
    MissingPath {
        role: String,
        kind: String,
        path: String,
    },
    #[error("role {role:?} needs the read {read:?}, which its claim does not declare")]
    MissingRead { role: String, read: String },
    #[error("role {role:?} has no read {read:?}")]
    UnknownRead { role: String, read: String },
    #[error(
        "role {role:?} reads \"clone_urls\", and descriptor.clone_from names no address to \
         clone from"
    )]
    NoCloneAddress { role: String },
    #[error(
        "descriptor.clone_from names config key {key:?}, which config_schema does not declare \
         as a string"
    )]
    UndeclaredCloneKey { key: String },
    #[error(
        "descriptor.clone_from names an address to clone from, and no \"code-host\" claim \
         reads \"clone_urls\""
    )]
    UnclaimedCloneAddress,
}

/// The code-host read a descriptor answers with
/// [`clone_from`](SourceDescriptor::clone_from) (#1197).
pub const CLONE_URLS: &str = "clone_urls";
/// The ticket-system read [`Source::translate_query`](crate::Source::translate_query)
/// answers (#1208): a saved view's query read into the search grammar.
pub const TRANSLATE_QUERY: &str = "translate_query";

/// Hold every role `descriptor` claims to that role's minimum, and answer the
/// first item missing. A descriptor that claims nothing passes.
///
/// # Errors
///
/// The first [`ClaimError`] found, in claim order.
pub fn check(descriptor: &SourceDescriptor) -> Result<(), ClaimError> {
    let mut seen = std::collections::HashSet::new();
    for claim in &descriptor.roles {
        let role = claim.role.clone();
        if !seen.insert(claim.role.as_str()) {
            return Err(ClaimError::ClaimedTwice { role });
        }
        let Some(min) = minimum(&claim.role) else {
            if !is_custom(&claim.role, &descriptor.adapter_kind) {
                return Err(ClaimError::UnknownRole {
                    role,
                    adapter_kind: descriptor.adapter_kind.clone(),
                });
            }
            if let Some(read) = claim.reads.first() {
                return Err(ClaimError::UnknownRead {
                    role,
                    read: read.clone(),
                });
            }
            continue;
        };
        for need in min.kinds {
            check_kind(descriptor, &role, need, Need::Required)?;
        }
        for need in min.optional_kinds {
            check_kind(descriptor, &role, need, Need::Optional)?;
        }
        if let Some(read) = min
            .reads
            .iter()
            .find(|r| !claim.reads.iter().any(|c| c == *r))
        {
            return Err(ClaimError::MissingRead {
                role,
                read: (*read).to_owned(),
            });
        }
        if let Some(read) = claim
            .reads
            .iter()
            .find(|r| !min.reads.contains(&r.as_str()) && !min.optional_reads.contains(&r.as_str()))
        {
            return Err(ClaimError::UnknownRead {
                role,
                read: read.clone(),
            });
        }
        if claim.reads.iter().any(|r| r == CLONE_URLS) && descriptor.clone_from.is_empty() {
            return Err(ClaimError::NoCloneAddress { role });
        }
    }
    check_clone_from(descriptor)
}

/// `clone_from` and a code-host claim's `clone_urls` are two signals for one
/// fact, and a config key it names is one the Add-source form asks for.
fn check_clone_from(d: &SourceDescriptor) -> Result<(), ClaimError> {
    if d.clone_from.is_empty() {
        return Ok(());
    }
    let claimed = d
        .roles
        .iter()
        .any(|c| c.role == CODE_HOST && c.reads.iter().any(|r| r == CLONE_URLS));
    if !claimed {
        return Err(ClaimError::UnclaimedCloneAddress);
    }
    for from in &d.clone_from {
        if let crate::clone_urls::CloneFrom::Config(key) = from
            && !crate::clone_urls::config_declares(&d.config_schema, key)
        {
            return Err(ClaimError::UndeclaredCloneKey { key: key.clone() });
        }
    }
    Ok(())
}

/// Whether a [`KindMinimum`] is one of the role's `kinds` or its
/// `optional_kinds`: an optional kind the descriptor does not emit passes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Need {
    Required,
    Optional,
}

fn check_kind(
    d: &SourceDescriptor,
    role: &str,
    need: &KindMinimum,
    how: Need,
) -> Result<(), ClaimError> {
    let emitted = |id: &str| d.entity_kinds.iter().any(|k| k.id == id);
    let (named, paths) = match need.kind {
        KindOf::Id(id) => {
            if !emitted(id) {
                if how == Need::Optional {
                    return Ok(());
                }
                return Err(ClaimError::MissingKind {
                    role: role.to_owned(),
                    kind: format!("{id:?}"),
                });
            }
            (
                format!("{id:?}"),
                d.payload_paths.iter().find(|p| p.kind == id),
            )
        }
        KindOf::Frame(frame) => {
            let word = match frame {
                Frame::Pipeline => "pipeline",
                Frame::Run => "run",
                Frame::SavedView => "saved_view",
            };
            let Some(paths) = d
                .payload_paths
                .iter()
                .find(|p| p.frame == Some(frame) && emitted(&p.kind))
            else {
                if how == Need::Optional {
                    return Ok(());
                }
                return Err(ClaimError::MissingKind {
                    role: role.to_owned(),
                    kind: format!("a {word} (a kind declaring frame {word:?})"),
                });
            };
            (format!("{:?}", paths.kind), Some(paths))
        }
    };
    let missing = need
        .paths
        .iter()
        .find(|f| !paths.is_some_and(|p| f.declared_in(p)));
    match missing {
        Some(field) => Err(ClaimError::MissingPath {
            role: role.to_owned(),
            kind: named,
            path: field.describe(),
        }),
        None => Ok(()),
    }
}

/// `<adapter_kind>:<name>`, the name a lowercase slug.
fn is_custom(role: &str, adapter_kind: &str) -> bool {
    role.split_once(':').is_some_and(|(owner, name)| {
        owner == adapter_kind
            && !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KindInfo, ParentPath, PayloadPath};

    /// A code host that fills its role: four kinds, each child under a repo.
    fn code_host(roles: Vec<RoleClaim>, parent_kind: &str) -> SourceDescriptor {
        let kind = |id: &str| KindInfo {
            id: id.to_owned(),
            label: id.to_owned(),
            plural: id.to_owned(),
            monogram: "XX".to_owned(),
            full_sync_exhaustive: true,
            refreshes: true,
        };
        let at = |key: &str| vec![PayloadPath::of([key])];
        let parent = vec![ParentPath {
            kind: parent_kind.to_owned(),
            at: PayloadPath::of(["repository"]),
            names: vec![PayloadPath::of(["full_name"])],
        }];
        let paths = |kind: &str| KindPaths {
            kind: kind.to_owned(),
            parent: parent.clone(),
            commit: at("sha"),
            ..KindPaths::default()
        };
        SourceDescriptor {
            id: "forge".into(),
            adapter_kind: "forge".into(),
            name: "Forge".into(),
            capabilities: vec![],
            adapter_version: "0.1.0".into(),
            auth_methods: vec![],
            accepts_account: false,
            min_sync_interval_secs: 60,
            source_actions: Vec::new(),
            clone_from: vec![crate::clone_urls::CloneFrom::BaseUrl],
            url_forms: Vec::new(),
            code_pages: None,
            write_ops: vec![],
            entity_kinds: ["repo", "branch", "pr", "commit"].map(kind).to_vec(),
            config_schema: serde_json::json!({}),
            payload_paths: vec![
                KindPaths {
                    default_branch: at("default_branch"),
                    ..paths("branch")
                },
                KindPaths {
                    state: vec![crate::StatePath::default()],
                    head: at("head"),
                    base: at("base"),
                    reviewers: vec![crate::ListPath::default()],
                    merged: at("merged"),
                    ..paths("pr")
                },
                paths("commit"),
            ],
            roles,
        }
    }

    fn code_host_min() -> &'static Minimum {
        minimum(CODE_HOST).expect("code-host is a built-in role")
    }

    /// A field's name is the key `KindPaths` serializes it under, so a
    /// refusal names the path an adapter author writes.
    #[test]
    fn every_required_field_is_named_as_kind_paths_serializes_it() {
        let wire = serde_json::to_value(KindPaths::default()).unwrap();
        for min in MINIMUMS {
            for need in min.kinds.iter().chain(min.optional_kinds) {
                for field in need.paths.iter().chain(need.optional_paths) {
                    assert!(
                        wire.get(field.name()).is_some(),
                        "{:?} is not a KindPaths key",
                        field.name()
                    );
                }
            }
        }
    }

    fn code_host_claim() -> RoleClaim {
        RoleClaim::new(CODE_HOST, code_host_min().reads.iter().copied())
    }

    #[test]
    fn a_claim_round_trips_and_a_descriptor_without_roles_claims_nothing() {
        let d = code_host(vec![code_host_claim()], "repo");
        let wire = serde_json::to_value(&d).unwrap();
        assert_eq!(wire["roles"][0]["role"], "code-host");
        assert_eq!(wire["roles"][0]["reads"][0], "pull_request_diff");
        let back: SourceDescriptor = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(back.roles, d.roles);
        let mut old = wire;
        assert_eq!(old["clone_from"], serde_json::json!(["base_url"]));
        assert_eq!(back.clone_from, d.clone_from);
        old.as_object_mut().unwrap().remove("roles");
        old.as_object_mut().unwrap().remove("clone_from");
        let old: SourceDescriptor = serde_json::from_value(old).unwrap();
        assert!(old.roles.is_empty());
        assert!(
            old.clone_from.is_empty(),
            "a peer from before #1197 names none"
        );
        let bare: RoleClaim = serde_json::from_value(serde_json::json!({ "role": "ci" })).unwrap();
        assert!(bare.reads.is_empty());
    }

    #[test]
    fn a_code_host_that_fills_its_role_passes() {
        assert_eq!(check(&code_host(vec![code_host_claim()], "repo")), Ok(()));
    }

    #[test]
    fn a_child_under_a_parent_that_is_no_repo_misses_the_parent() {
        let refused = check(&code_host(vec![code_host_claim()], "project")).unwrap_err();
        assert_eq!(
            refused.to_string(),
            r#"role "code-host" needs kind "branch" to declare the path parent (a "repo")"#
        );
    }

    /// The code-host minimum's pull request names both ends (#1227, spec
    /// #1192): head and base.
    #[test]
    fn a_pull_request_that_declares_no_base_misses_the_base() {
        let mut d = code_host(vec![code_host_claim()], "repo");
        let pr = d.payload_paths.iter_mut().find(|p| p.kind == "pr").unwrap();
        pr.base.clear();
        assert_eq!(
            check(&d).unwrap_err().to_string(),
            r#"role "code-host" needs kind "pr" to declare the path base"#
        );
    }

    /// The monitoring minimum (#1224, ADR-0029): a monitor with a state *and*
    /// a response time, the two facts its samples are taken from.
    #[test]
    fn a_monitor_needs_its_state_and_its_response_time() {
        let mut d = code_host(vec![RoleClaim::new(MONITORING, [])], "repo");
        d.clone_from.clear();
        d.entity_kinds.push(KindInfo {
            id: "monitor".into(),
            label: "Monitor".into(),
            plural: "Monitors".into(),
            monogram: "MO".into(),
            full_sync_exhaustive: true,
            refreshes: true,
        });
        let monitor = KindPaths {
            kind: "monitor".into(),
            status_name: vec![PayloadPath::of(["state"])],
            response_time: vec![PayloadPath::of(["response_time_ms"])],
            ..KindPaths::default()
        };
        d.payload_paths.push(monitor.clone());
        assert_eq!(check(&d), Ok(()));
        d.payload_paths.last_mut().unwrap().response_time.clear();
        assert_eq!(
            check(&d).unwrap_err().to_string(),
            r#"role "monitoring" needs kind "monitor" to declare the path response_time"#
        );
    }

    /// The ticket-system minimum's saved view (#2050, ADR-0029): a kind
    /// declaring the frame must say where its query is, since the projection
    /// reads it there and nowhere else. Its board type, sub-query and sprint
    /// are the frame's too, and optional. A ticket system that emits no saved
    /// view is not asked.
    #[test]
    fn a_saved_view_kind_needs_its_query() {
        let mut d = code_host(
            vec![RoleClaim::new(TICKET_SYSTEM, ["reachable_transitions"])],
            "repo",
        );
        d.clone_from.clear();
        let info = |id: &str| KindInfo {
            id: id.into(),
            label: id.into(),
            plural: id.into(),
            monogram: "TK".into(),
            full_sync_exhaustive: true,
            refreshes: true,
        };
        d.entity_kinds.push(info("ticket"));
        let at = |key: &str| vec![PayloadPath::of([key])];
        d.payload_paths.push(KindPaths {
            kind: "ticket".into(),
            status_name: at("status"),
            priority: at("priority"),
            assignee: at("assignee"),
            issue_type: at("type"),
            labels: vec![crate::ListPath::default()],
            parent: vec![ParentPath {
                kind: "ticket".into(),
                at: PayloadPath::of(["parent"]),
                names: vec![PayloadPath::of(["key"])],
            }],
            project_key: at("project"),
            ..KindPaths::default()
        });
        assert_eq!(check(&d), Ok(()), "no saved view, nothing to ask");
        d.entity_kinds.push(info("project_view"));
        d.payload_paths.push(KindPaths {
            kind: "project_view".into(),
            frame: Some(Frame::SavedView),
            source_query: at("filter"),
            ..KindPaths::default()
        });
        assert_eq!(check(&d), Ok(()), "the optional paths may go undeclared");
        d.payload_paths.last_mut().unwrap().source_query.clear();
        assert_eq!(
            check(&d).unwrap_err().to_string(),
            r#"role "ticket-system" needs kind "project_view" to declare the path source_query"#
        );
        let saved = &minimum(TICKET_SYSTEM).unwrap().optional_kinds[0];
        assert_eq!(
            saved.optional_paths,
            [Field::BoardType, Field::SubQuery, Field::Sprint]
        );
    }

    #[test]
    fn a_role_claimed_twice_is_refused() {
        let d = code_host(vec![code_host_claim(), code_host_claim()], "repo");
        assert_eq!(
            check(&d),
            Err(ClaimError::ClaimedTwice {
                role: CODE_HOST.into()
            })
        );
    }

    #[test]
    fn a_read_the_role_does_not_have_is_refused_by_name() {
        let mut claim = code_host_claim();
        claim.reads.push("run_log".into());
        assert_eq!(
            check(&code_host(vec![claim], "repo"))
                .unwrap_err()
                .to_string(),
            r#"role "code-host" has no read "run_log""#
        );
    }

    #[test]
    fn a_custom_role_is_namespaced_to_its_adapter_and_has_no_reads() {
        let custom = |role: &str, reads: &[&str]| {
            let mut d = code_host(vec![RoleClaim::new(role, reads.iter().copied())], "repo");
            // No code-host claim, so no address to clone from either.
            d.clone_from.clear();
            check(&d)
        };
        assert_eq!(custom("forge:dashboard-host", &[]), Ok(()));
        for role in [
            "grafana:dashboard-host",
            "forge:",
            "forge:Dash",
            "dashboards",
        ] {
            assert!(
                matches!(custom(role, &[]), Err(ClaimError::UnknownRole { .. })),
                "{role:?} must be refused"
            );
        }
        assert!(matches!(
            custom("forge:dashboard-host", &["view"]),
            Err(ClaimError::UnknownRead { .. })
        ));
    }

    /// Every required read is a read the claim is refused without.
    #[test]
    fn each_required_read_is_needed() {
        for read in code_host_min().reads {
            let claim = RoleClaim::new(
                CODE_HOST,
                code_host_min().reads.iter().copied().filter(|r| r != read),
            );
            assert_eq!(
                check(&code_host(vec![claim], "repo")),
                Err(ClaimError::MissingRead {
                    role: CODE_HOST.into(),
                    read: (*read).to_owned()
                })
            );
        }
    }

    /// The clone-URLs read (#1197): a code host answers it from a typed
    /// address, a config key it names is in the schema, and an address
    /// declared without the claim is refused.
    #[test]
    fn a_code_host_answers_its_clone_urls_from_a_typed_address() {
        let mut none = code_host(vec![code_host_claim()], "repo");
        none.clone_from.clear();
        assert_eq!(
            check(&none).unwrap_err().to_string(),
            r#"role "code-host" reads "clone_urls", and descriptor.clone_from names no address to clone from"#
        );

        let mut keyed = code_host(vec![code_host_claim()], "repo");
        keyed.clone_from = vec![crate::clone_urls::CloneFrom::Config("ssh_base".into())];
        assert_eq!(
            check(&keyed),
            Err(ClaimError::UndeclaredCloneKey {
                key: "ssh_base".into()
            })
        );
        keyed.config_schema = serde_json::json!({
            "properties": { "ssh_base": { "type": "string" } }
        });
        assert_eq!(check(&keyed), Ok(()));

        let unclaimed = code_host(vec![], "repo");
        assert_eq!(check(&unclaimed), Err(ClaimError::UnclaimedCloneAddress));
        let mut nothing = unclaimed;
        nothing.clone_from.clear();
        assert_eq!(check(&nothing), Ok(()));
    }
}
