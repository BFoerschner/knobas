//! The data half of a role check (ADR-0028, *The checks*; issue #1199): the
//! declared paths a claimed role requires, held to the records a sync
//! emitted.
//!
//! [`check`](super::check) is the static half: it reads the descriptor and
//! nothing else. This one reads records. It asks of each path a role's
//! [`Minimum`](super::Minimum) requires the two questions the contract
//! battery's clause 6 asks of every declared path, and asks them the same
//! way, over the corpus rather than one record:
//!
//! * **a wrong shape**: the path leads to a value of the wrong type (a status
//!   declared one segment short reaches the `{"name": …}` object). One record
//!   is enough to say so.
//! * **a missing key**: no record of the kind resolved the field, and some
//!   record has an object where the path's next key would sit without that
//!   key (see [`names_a_missing_key`]). An unassigned ticket's `null` is the
//!   source saying nothing, not a miss. Neither is an absent [`Field::Parent`]:
//!   Jira leaves `fields.parent` out of a ticket with no parent, so a project
//!   with no epics or sub-tasks would otherwise lose its Tickets surface. A
//!   parent's wrong shape is still a miss, and so is a missing
//!   [`Field::ParentOf`] (a branch always has its repo).
//!
//! A kind the records hold none of is not judged: there is no evidence, and
//! the verdict says which kinds it saw so the caller can tell "passed" from
//! "not asked yet".
//!
//! Streaming: [`DataCheck::observe`] takes one record at a time, so a sync
//! engine can feed it from its sink without holding the corpus.

use std::collections::BTreeSet;

use knobas_spi_types::payload::{KindPaths, PayloadPath, at, names_a_missing_key};
use serde_json::Value;

use super::{Field, KindOf, minimum};
use crate::SourceDescriptor;

/// One declared path of a claimed role that missed on the records the
/// source's syncs emitted (#1199, ADR-0028), so the shell does not draw that
/// role for the source.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct RoleMiss {
    /// The role, as claimed (`ticket-system`).
    pub role: String,
    /// The kind whose records missed (`ticket`).
    pub kind: String,
    /// The [`KindPaths`] field (`priority`).
    pub field: String,
    /// The declared path that missed, its segments joined by `.`
    /// (`fields.priorityy.name`).
    pub path: String,
}

/// What the records said about one claimed built-in role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleVerdict {
    pub role: String,
    /// The first required path that missed, in the minimum's order.
    pub miss: Option<RoleMiss>,
    /// The required kinds with paths that some record was of.
    pub seen: BTreeSet<String>,
    /// Every required kind with paths was seen, so a verdict without a miss
    /// is a pass and not an absence of evidence.
    pub complete: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Shape {
    Text,
    Instant,
    Flag,
    /// A JSON number (#1224): a monitor's response time.
    Number,
    /// A state: a string or a boolean.
    TextOrFlag,
    List,
    /// One object, or an array of objects.
    Parent,
    /// One JSON object (#2050): a saved view's sprint.
    Object,
}

impl Shape {
    fn fits(self, value: &Value) -> bool {
        match self {
            Shape::Text => value.is_string(),
            Shape::Instant => value
                .as_str()
                .is_some_and(|t| knobas_spi_types::payload::parse_instant(t).is_some()),
            Shape::Flag => value.is_boolean(),
            Shape::Number => value.is_number(),
            Shape::TextOrFlag => value.is_string() || value.is_boolean(),
            Shape::List => value.is_array(),
            Shape::Parent => {
                value.is_object()
                    || value
                        .as_array()
                        .is_some_and(|chain| chain.iter().all(Value::is_object))
            }
            Shape::Object => value.is_object(),
        }
    }
}

/// One required path, and what the records showed of it.
#[derive(Debug, Clone)]
struct Need {
    role_at: usize,
    kind: String,
    field: &'static str,
    shape: Shape,
    candidates: Vec<PayloadPath>,
    /// An absent key is the record having none (a ticket with no parent),
    /// never a misnamed path.
    absent_is_none: bool,
    resolved: bool,
    wrong: Option<String>,
    missing: Option<String>,
}

/// The data check of one descriptor's role claims, fed one record at a time.
#[derive(Debug, Clone)]
pub struct DataCheck {
    roles: Vec<(String, BTreeSet<String>)>,
    needs: Vec<Need>,
    seen: BTreeSet<String>,
}

fn dotted(path: &PayloadPath) -> String {
    path.segments().join(".")
}

/// Where `paths` declares `field`, and what it must lead to: the one map from
/// a [`Field`] to its [`KindPaths`] candidates, which
/// [`Field::declared_in`] reads too.
pub(super) fn needs_of(field: Field, paths: &KindPaths) -> (Shape, Vec<PayloadPath>) {
    let plain = |v: &Vec<PayloadPath>| v.clone();
    let lists =
        |v: &Vec<knobas_spi_types::payload::ListPath>| v.iter().map(|l| l.at.clone()).collect();
    match field {
        Field::StatusName => (Shape::Text, plain(&paths.status_name)),
        Field::Priority => (Shape::Text, plain(&paths.priority)),
        Field::Assignee => (Shape::Text, plain(&paths.assignee)),
        Field::IssueType => (Shape::Text, plain(&paths.issue_type)),
        Field::ProjectKey => (Shape::Text, plain(&paths.project_key)),
        Field::Head => (Shape::Text, plain(&paths.head)),
        Field::Base => (Shape::Text, plain(&paths.base)),
        Field::Commit => (Shape::Text, plain(&paths.commit)),
        Field::DefaultBranch => (Shape::Text, plain(&paths.default_branch)),
        Field::Name => (Shape::Text, plain(&paths.name)),
        Field::Pipeline => (Shape::Text, plain(&paths.pipeline)),
        Field::Number => (Shape::Text, plain(&paths.number)),
        Field::Branch => (Shape::Text, plain(&paths.branch)),
        Field::Started => (Shape::Instant, plain(&paths.started)),
        Field::Finished => (Shape::Instant, plain(&paths.finished)),
        Field::Merged => (Shape::Flag, plain(&paths.merged)),
        Field::ResponseTime => (Shape::Number, plain(&paths.response_time)),
        Field::SourceQuery => (Shape::Text, plain(&paths.source_query)),
        Field::BoardType => (Shape::Text, plain(&paths.board_type)),
        Field::SubQuery => (Shape::Text, plain(&paths.sub_query)),
        Field::Sprint => (Shape::Object, plain(&paths.sprint)),
        Field::State => (
            Shape::TextOrFlag,
            paths.state.iter().map(|s| s.at.clone()).collect(),
        ),
        Field::Labels => (Shape::List, lists(&paths.labels)),
        Field::Reviewers => (Shape::List, lists(&paths.reviewers)),
        Field::Parent => (
            Shape::Parent,
            paths.parent.iter().map(|p| p.at.clone()).collect(),
        ),
        Field::ParentOf(kind) => (
            Shape::Parent,
            paths
                .parent
                .iter()
                .filter(|p| p.kind == kind)
                .map(|p| p.at.clone())
                .collect(),
        ),
    }
}

/// The kind a minimum's [`KindOf`] names on this descriptor, and its paths:
/// by id, or the first emitted kind declaring the frame, as
/// [`check`](super::check) picks it.
fn kind_paths(d: &SourceDescriptor, of: KindOf) -> Option<&KindPaths> {
    let emitted = |id: &str| d.entity_kinds.iter().any(|k| k.id == id);
    match of {
        KindOf::Id(id) => d.payload_paths.iter().find(|p| p.kind == id),
        KindOf::Frame(frame) => d
            .payload_paths
            .iter()
            .find(|p| p.frame == Some(frame) && emitted(&p.kind)),
    }
}

impl DataCheck {
    /// The check of every built-in role `descriptor` claims. A custom role has
    /// no minimum and nothing to check; a claim the static check would refuse
    /// (a kind with no declared paths) checks what it can.
    #[must_use]
    pub fn new(descriptor: &SourceDescriptor) -> Self {
        let mut roles = Vec::new();
        let mut needs = Vec::new();
        for claim in &descriptor.roles {
            let Some(min) = minimum(&claim.role) else {
                continue;
            };
            let role_at = roles.len();
            let mut kinds = BTreeSet::new();
            for need in min.kinds.iter().filter(|n| !n.paths.is_empty()) {
                let Some(paths) = kind_paths(descriptor, need.kind) else {
                    continue;
                };
                kinds.insert(paths.kind.clone());
                for field in need.paths {
                    let (shape, candidates) = needs_of(*field, paths);
                    needs.push(Need {
                        role_at,
                        kind: paths.kind.clone(),
                        field: field.name(),
                        shape,
                        candidates,
                        absent_is_none: matches!(field, Field::Parent),
                        resolved: false,
                        wrong: None,
                        missing: None,
                    });
                }
            }
            roles.push((claim.role.clone(), kinds));
        }
        Self {
            roles,
            needs,
            seen: BTreeSet::new(),
        }
    }

    /// Whether there is anything to check: some claimed role is built in.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.roles.is_empty()
    }

    /// The roles this check judges, in claim order.
    pub fn roles(&self) -> impl Iterator<Item = &str> {
        self.roles.iter().map(|(role, _)| role.as_str())
    }

    /// Weigh one record of `kind`.
    pub fn observe(&mut self, kind: &str, payload: &Value) {
        let mut any = false;
        for need in self.needs.iter_mut().filter(|n| n.kind == kind) {
            any = true;
            for path in &need.candidates {
                if let Some(value) = at(payload, path) {
                    if need.shape.fits(value) {
                        need.resolved = true;
                    } else if need.wrong.is_none() {
                        need.wrong = Some(dotted(path));
                    }
                } else if !need.absent_is_none
                    && need.missing.is_none()
                    && names_a_missing_key(payload, path)
                {
                    need.missing = Some(dotted(path));
                }
            }
        }
        if any {
            self.seen.insert(kind.to_owned());
        }
    }

    /// One verdict per claimed built-in role, in claim order.
    #[must_use]
    pub fn finish(self) -> Vec<RoleVerdict> {
        self.roles
            .iter()
            .enumerate()
            .map(|(i, (role, kinds))| {
                let miss = self
                    .needs
                    .iter()
                    .filter(|n| n.role_at == i && self.seen.contains(&n.kind))
                    .find_map(|n| {
                        let path = n
                            .wrong
                            .clone()
                            .or_else(|| (!n.resolved).then(|| n.missing.clone()).flatten())?;
                        Some(RoleMiss {
                            role: role.clone(),
                            kind: n.kind.clone(),
                            field: n.field.to_owned(),
                            path,
                        })
                    });
                let seen: BTreeSet<String> = kinds.intersection(&self.seen).cloned().collect();
                RoleVerdict {
                    role: role.clone(),
                    complete: seen.len() == kinds.len(),
                    miss,
                    seen,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::role::{RoleClaim, TICKET_SYSTEM};
    use crate::{KindInfo, ParentPath};
    use knobas_spi_types::payload::ListPath;
    use serde_json::json;

    /// A Jira-shaped ticket system whose priority sits at `priority_at`.
    fn tickets(priority_at: &[&str]) -> SourceDescriptor {
        let at = |segments: &[&str]| vec![PayloadPath::of(segments.iter().copied())];
        SourceDescriptor {
            id: "jira".into(),
            adapter_kind: "jira".into(),
            name: "Jira".into(),
            capabilities: vec![],
            adapter_version: "0.1.0".into(),
            auth_methods: vec![],
            accepts_account: false,
            min_sync_interval_secs: 60,
            write_ops: vec![],
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "JI".into(),
                full_sync_exhaustive: true,
                refreshes: true,
            }],
            config_schema: json!({}),
            payload_paths: vec![KindPaths {
                kind: "ticket".into(),
                status_name: at(&["fields", "status", "name"]),
                priority: at(priority_at),
                assignee: at(&["fields", "assignee", "name"]),
                issue_type: at(&["fields", "issuetype", "name"]),
                project_key: at(&["fields", "project", "key"]),
                labels: vec![ListPath {
                    at: PayloadPath::of(["fields", "labels"]),
                    entry: PayloadPath::default(),
                }],
                parent: vec![ParentPath {
                    kind: "ticket".into(),
                    at: PayloadPath::of(["fields", "parent"]),
                    names: vec![PayloadPath::of(["key"])],
                }],
                ..KindPaths::default()
            }],
            roles: vec![RoleClaim::new(TICKET_SYSTEM, ["reachable_transitions"])],
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            code_pages: None,
        }
    }

    fn ticket(assignee: Value) -> Value {
        json!({ "fields": {
            "status": { "name": "Open" },
            "priority": { "name": "High" },
            "assignee": assignee,
            "issuetype": { "name": "Bug" },
            "project": { "key": "PAY" },
            "labels": [],
            "parent": { "key": "PAY-1" },
        }})
    }

    fn verdicts(d: &SourceDescriptor, records: &[Value]) -> Vec<RoleVerdict> {
        let mut check = DataCheck::new(d);
        for record in records {
            check.observe("ticket", record);
        }
        check.finish()
    }

    #[test]
    fn records_that_resolve_every_required_path_fill_the_role() {
        let d = tickets(&["fields", "priority", "name"]);
        let [v] = verdicts(&d, &[ticket(json!({ "name": "mara" }))])
            .try_into()
            .unwrap();
        assert_eq!(v.role, TICKET_SYSTEM);
        assert_eq!(v.miss, None);
        assert!(v.complete);
        assert_eq!(v.seen, BTreeSet::from(["ticket".to_owned()]));
    }

    #[test]
    fn a_misspelled_key_no_record_resolves_is_a_miss_naming_the_path() {
        let d = tickets(&["fields", "priorityy", "name"]);
        let [v] = verdicts(&d, &[ticket(Value::Null)]).try_into().unwrap();
        assert_eq!(
            v.miss,
            Some(RoleMiss {
                role: TICKET_SYSTEM.into(),
                kind: "ticket".into(),
                field: "priority".into(),
                path: "fields.priorityy.name".into(),
            })
        );
    }

    /// An unassigned ticket's `null` is the source saying nothing: a corpus of
    /// them resolves no assignee and still fills the role.
    #[test]
    fn a_field_every_record_leaves_null_is_not_a_miss() {
        let d = tickets(&["fields", "priority", "name"]);
        let [v] = verdicts(&d, &[ticket(Value::Null), ticket(Value::Null)])
            .try_into()
            .unwrap();
        assert_eq!(v.miss, None);
    }

    #[test]
    fn one_record_that_resolves_settles_the_kind() {
        let d = tickets(&["fields", "priority", "name"]);
        let mut untyped = ticket(Value::Null);
        untyped["fields"]
            .as_object_mut()
            .unwrap()
            .remove("issuetype");
        let [v] = verdicts(&d, &[untyped.clone(), ticket(Value::Null)])
            .try_into()
            .unwrap();
        assert_eq!(v.miss, None);
        let [alone] = verdicts(&d, &[untyped]).try_into().unwrap();
        assert_eq!(
            alone.miss.map(|m| m.path),
            Some("fields.issuetype.name".to_owned())
        );
    }

    /// Jira leaves `fields.parent` out of a ticket with no parent: a project
    /// with no epics or sub-tasks still fills the role. A parent of the wrong
    /// shape is still a miss.
    #[test]
    fn a_parent_no_record_has_is_not_a_miss_but_a_wrong_one_is() {
        let d = tickets(&["fields", "priority", "name"]);
        let mut parentless = ticket(Value::Null);
        parentless["fields"]
            .as_object_mut()
            .unwrap()
            .remove("parent");
        let [v] = verdicts(&d, &[parentless.clone(), parentless])
            .try_into()
            .unwrap();
        assert_eq!(v.miss, None);
        assert!(v.complete);
        let mut wrong = ticket(Value::Null);
        wrong["fields"]["parent"] = json!("PAY-1");
        let [w] = verdicts(&d, &[wrong]).try_into().unwrap();
        assert_eq!(w.miss.map(|m| m.path), Some("fields.parent".to_owned()));
    }

    #[test]
    fn a_path_one_segment_short_is_a_wrong_shape_and_a_miss() {
        let d = tickets(&["fields", "priority"]);
        let [v] = verdicts(&d, &[ticket(Value::Null)]).try_into().unwrap();
        assert_eq!(v.miss.map(|m| m.path), Some("fields.priority".to_owned()));
    }

    /// A monitor's response time is a number (#1224): one written as a
    /// string is a wrong shape, and Kuma's `null` for a check that did not
    /// answer is the source saying nothing.
    #[test]
    fn a_response_time_that_is_no_number_is_a_miss_and_a_null_is_not() {
        let mut d = tickets(&["fields", "priority", "name"]);
        d.roles = vec![RoleClaim::new(crate::role::MONITORING, [])];
        d.entity_kinds[0].id = "monitor".into();
        d.payload_paths = vec![KindPaths {
            kind: "monitor".into(),
            status_name: vec![PayloadPath::of(["state"])],
            response_time: vec![PayloadPath::of(["response_time_ms"])],
            ..KindPaths::default()
        }];
        let judge = |records: &[Value]| {
            let mut check = DataCheck::new(&d);
            for record in records {
                check.observe("monitor", record);
            }
            let [v] = check.finish().try_into().unwrap();
            v.miss.map(|m| m.path)
        };
        let up = json!({ "state": "up", "response_time_ms": 13.0 });
        let silent = json!({ "state": "down", "response_time_ms": null });
        let spelled = json!({ "state": "up", "response_time_ms": "13" });
        assert_eq!(judge(&[up, silent.clone()]), None);
        assert_eq!(judge(&[silent]), None);
        assert_eq!(judge(&[spelled]), Some("response_time_ms".to_owned()));
    }

    #[test]
    fn records_of_no_required_kind_are_no_evidence() {
        let d = tickets(&["fields", "priorityy", "name"]);
        let mut check = DataCheck::new(&d);
        check.observe("board", &json!({}));
        let [v] = check.finish().try_into().unwrap();
        assert_eq!(v.miss, None);
        assert!(!v.complete);
        assert!(v.seen.is_empty());
    }

    #[test]
    fn a_custom_role_and_no_claim_check_nothing() {
        let mut d = tickets(&["fields", "priorityy", "name"]);
        d.roles = vec![RoleClaim::new("jira:dashboard-host", [])];
        assert!(DataCheck::new(&d).is_empty());
        assert!(verdicts(&d, &[ticket(Value::Null)]).is_empty());
    }
}
