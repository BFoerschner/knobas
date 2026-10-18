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

use knobas_core::payload::{Frame, KindPaths};

use crate::SourceDescriptor;

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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RoleClaim {
    pub role: String,
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

/// How a minimum names a kind: by its id, or, for ADR-0022's Builds frame, by
/// the [`Frame`] a kind declares (TeamCity's `build_config` is a pipeline).
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
    Commit,
    DefaultBranch,
    Name,
    Pipeline,
    Number,
    Branch,
    Started,
    Finished,
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
            Field::Commit => "commit",
            Field::DefaultBranch => "default_branch",
            Field::Name => "name",
            Field::Pipeline => "pipeline",
            Field::Number => "number",
            Field::Branch => "branch",
            Field::Started => "started",
            Field::Finished => "finished",
        }
    }

    /// Whether `paths` declares this field: at least one candidate, and for
    /// [`Field::ParentOf`] one whose parent is of that kind.
    #[must_use]
    pub fn declared_in(self, paths: &KindPaths) -> bool {
        match self {
            Field::StatusName => !paths.status_name.is_empty(),
            Field::Priority => !paths.priority.is_empty(),
            Field::Assignee => !paths.assignee.is_empty(),
            Field::IssueType => !paths.issue_type.is_empty(),
            Field::Labels => !paths.labels.is_empty(),
            Field::ProjectKey => !paths.project_key.is_empty(),
            Field::Parent => !paths.parent.is_empty(),
            Field::ParentOf(kind) => paths.parent.iter().any(|p| p.kind == kind),
            Field::State => !paths.state.is_empty(),
            Field::Reviewers => !paths.reviewers.is_empty(),
            Field::Merged => !paths.merged.is_empty(),
            Field::Head => !paths.head.is_empty(),
            Field::Commit => !paths.commit.is_empty(),
            Field::DefaultBranch => !paths.default_branch.is_empty(),
            Field::Name => !paths.name.is_empty(),
            Field::Pipeline => !paths.pipeline.is_empty(),
            Field::Number => !paths.number.is_empty(),
            Field::Branch => !paths.branch.is_empty(),
            Field::Started => !paths.started.is_empty(),
            Field::Finished => !paths.finished.is_empty(),
        }
    }

    fn describe(self) -> String {
        match self {
            Field::ParentOf(kind) => format!("parent (a {kind:?})"),
            other => other.name().to_owned(),
        }
    }
}

/// One kind a role requires, and the declared paths it must carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindMinimum {
    pub kind: KindOf,
    pub paths: &'static [Field],
}

/// A built-in role's minimum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Minimum {
    pub role: &'static str,
    pub kinds: &'static [KindMinimum],
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
    }
}

const fn frame(frame: Frame, paths: &'static [Field]) -> KindMinimum {
    KindMinimum {
        kind: KindOf::Frame(frame),
        paths,
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
                    Field::Commit,
                    Field::Reviewers,
                    Field::Merged,
                ],
            ),
            kind("commit", &[Field::ParentOf("repo"), Field::Commit]),
        ],
        reads: &[
            "pull_request_diff",
            "commit_diff",
            "repository_tree",
            "file_contents",
            "line_threads",
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
        reads: &["reachable_transitions"],
        optional_reads: &[],
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
        reads: &["doc_content"],
        optional_reads: &[],
        writes: &["create_page", "update_page", "comment"],
    },
    Minimum {
        role: MONITORING,
        kinds: &[kind("monitor", &[Field::StatusName])],
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
}

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
            check_kind(descriptor, &role, need)?;
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
    }
    Ok(())
}

fn check_kind(d: &SourceDescriptor, role: &str, need: &KindMinimum) -> Result<(), ClaimError> {
    let emitted = |id: &str| d.entity_kinds.iter().any(|k| k.id == id);
    let (named, paths) = match need.kind {
        KindOf::Id(id) => {
            if !emitted(id) {
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
            };
            let Some(paths) = d
                .payload_paths
                .iter()
                .find(|p| p.frame == Some(frame) && emitted(&p.kind))
            else {
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
                    reviewers: vec![crate::ListPath::default()],
                    merged: at("merged"),
                    ..paths("pr")
                },
                paths("commit"),
            ],
            roles,
        }
    }

    fn code_host_claim() -> RoleClaim {
        RoleClaim::new(CODE_HOST, MINIMUMS[0].reads.iter().copied())
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
        old.as_object_mut().unwrap().remove("roles");
        let old: SourceDescriptor = serde_json::from_value(old).unwrap();
        assert!(old.roles.is_empty());
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
            check(&code_host(
                vec![RoleClaim::new(role, reads.iter().copied())],
                "repo",
            ))
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
        for read in MINIMUMS[0].reads {
            let claim = RoleClaim::new(
                CODE_HOST,
                MINIMUMS[0].reads.iter().copied().filter(|r| r != read),
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
}
