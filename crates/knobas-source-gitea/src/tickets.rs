//! A Gitea issue's fields (ADR-0054): what a person may change on it
//! besides its state, its people and labels to choose from, and the edit.
//!
//! - `title` is text; `assignee` a person searched in the repository's
//!   assignees, read only where the issue has more than one (setting one
//!   would drop the others); `labels` the repository's and its
//!   organisation's, which a person cannot make here.
//! - **No priority**: knobas reads a Gitea priority out of a scoped label,
//!   and a person changes it by changing the label.
//! - **The due date** is the issue's own field ([`DUE`], a `date`): set by
//!   `due_date` at the day's start in UTC, cleared by `unset_due_date`, and
//!   read back as the day the answer names.
//! - A pull request is not a ticket: [`fields`] is asked of issues only.

use knobas_source::ticket::{
    self, FieldOption, FieldOptions, TicketField, TicketFieldKind, TicketFields,
};
use knobas_source::{Assignee, LabelChange, SourceError};
use serde_json::{Value, json};

use crate::GiteaSource;
use crate::client::issues_path;

/// A Gitea issue's due date: a source's own field (ADR-0054, step 10).
pub(crate) const DUE: &str = "due_date";

/// One issue as the fields read it.
struct Issue {
    title: String,
    /// `YYYY-MM-DD`, the day of its `due_date`, or `None`.
    due: Option<String>,
    /// `(login, full name)`, in Gitea's order.
    assignees: Vec<(String, Option<String>)>,
    /// `(id, name)`.
    labels: Vec<(u64, String)>,
}

fn issue_of(value: &Value) -> Issue {
    let text = |v: &Value| v.as_str().map(str::to_owned);
    let assignees = value["assignees"]
        .as_array()
        .map(|people| {
            people
                .iter()
                .filter_map(|p| {
                    let login = text(&p["login"])?;
                    let name = text(&p["full_name"]).filter(|n| !n.trim().is_empty());
                    Some((login, name))
                })
                .collect()
        })
        .unwrap_or_default();
    let labels = value["labels"]
        .as_array()
        .map(|labels| {
            labels
                .iter()
                .filter_map(|l| Some((l["id"].as_u64()?, text(&l["name"])?)))
                .collect()
        })
        .unwrap_or_default();
    Issue {
        title: text(&value["title"]).unwrap_or_default(),
        due: value["due_date"]
            .as_str()
            .and_then(|d| d.get(..10))
            .map(str::to_owned),
        assignees,
        labels,
    }
}

/// The scope a repository's people and labels are kept under:
/// `owner/repo`, the same for every issue of it.
fn scope(owner: &str, repo: &str) -> String {
    format!("{owner}/{repo}")
}

/// Why an issue with several assignees cannot be given one here.
fn several(count: usize, source: &str) -> String {
    format!("This issue has {count} assignees. Change them in {source}.")
}

/// The fields of issue `owner/repo#number`, from the read
/// `reachable_transitions` makes.
///
/// # Errors
///
/// As the issue read.
pub(crate) async fn fields(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<TicketFields, SourceError> {
    let issue = issue_of(&source.client.issue(owner, repo, number).await?);
    Ok(answer(&issue, owner, repo, source.display_name()))
}

fn answer(issue: &Issue, owner: &str, repo: &str, source: &str) -> TicketFields {
    let scope = scope(owner, repo);
    let first = issue.assignees.first();
    TicketFields {
        fields: vec![
            TicketField {
                id: ticket::TITLE.into(),
                label: "Summary".into(),
                kind: TicketFieldKind::Text,
                value: json!(issue.title),
                shown: None,
                required: true,
                readonly: None,
            },
            TicketField {
                id: ticket::ASSIGNEE.into(),
                label: "Assignee".into(),
                kind: TicketFieldKind::Search {
                    scope: scope.clone(),
                    person: true,
                },
                value: first.map_or(Value::Null, |(login, _)| json!(login)),
                shown: first.map(|(login, name)| name.clone().unwrap_or_else(|| login.clone())),
                required: false,
                readonly: (issue.assignees.len() > 1)
                    .then(|| several(issue.assignees.len(), source)),
            },
            TicketField {
                id: ticket::LABELS.into(),
                label: "Labels".into(),
                kind: TicketFieldKind::Labels {
                    scope: Some(scope),
                    create: false,
                },
                value: json!(
                    issue
                        .labels
                        .iter()
                        .map(|(_, name)| name.as_str())
                        .collect::<Vec<_>>()
                ),
                shown: None,
                required: false,
                readonly: None,
            },
            TicketField {
                id: DUE.into(),
                label: "Due date".into(),
                kind: TicketFieldKind::Date,
                value: issue.due.as_ref().map_or(Value::Null, |d| json!(d)),
                shown: None,
                required: false,
                readonly: None,
            },
        ],
    }
}

/// The `PATCH` keys a due date of `value` sets: a day at its start in UTC,
/// or `null` to clear it. `None` for anything else, which is refused by name.
fn due_patch(value: &Value) -> Option<(&'static str, Value)> {
    match value {
        Value::Null => Some(("unset_due_date", json!(true))),
        Value::String(day) if TicketFieldKind::Date.fits(value) => {
            Some(("due_date", json!(format!("{day}T00:00:00Z"))))
        }
        _ => None,
    }
}

/// The choices of `field` on an issue of `owner/repo` matching `typed`:
/// the repository's assignees, or the labels it may use. Each list is read
/// whole, so the answer is complete.
///
/// # Errors
///
/// As the list's read.
pub(crate) async fn options(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    field: &str,
    typed: &str,
) -> Result<Option<FieldOptions>, SourceError> {
    let options = match field {
        ticket::ASSIGNEE => people(source.client.assignees(owner, repo).await?),
        ticket::LABELS => {
            let labels = crate::view::pr::label::read(source, owner, repo)
                .await
                .map_err(SourceError::protocol)?;
            label_names(labels.iter().map(|l| l.name.as_str()))
        }
        _ => return Ok(None),
    };
    Ok(Some(FieldOptions {
        options: matching(options, typed),
        complete: true,
    }))
}

fn people(users: Vec<crate::model::UserRef>) -> Vec<FieldOption> {
    let mut options: Vec<FieldOption> = users
        .into_iter()
        .filter_map(|u| {
            let login = u.login?;
            let name = u.full_name.filter(|n| !n.trim().is_empty());
            Some(match name {
                Some(name) => FieldOption {
                    value: login.clone(),
                    label: name,
                    hint: Some(login),
                },
                None => FieldOption {
                    value: login.clone(),
                    label: login,
                    hint: None,
                },
            })
        })
        .collect();
    options.sort_by_key(|o| o.label.to_lowercase());
    options
}

/// Each name once, a repository's label before an organisation's of the
/// same name.
fn label_names<'a>(names: impl Iterator<Item = &'a str>) -> Vec<FieldOption> {
    let mut seen = std::collections::HashSet::new();
    names
        .filter(|name| seen.insert(name.to_owned()))
        .map(|name| FieldOption {
            value: name.to_owned(),
            label: name.to_owned(),
            hint: None,
        })
        .collect()
}

/// The options whose label or hint holds `typed`, ignoring case; all of
/// them for nothing typed. The picker ranks them itself.
fn matching(options: Vec<FieldOption>, typed: &str) -> Vec<FieldOption> {
    let typed = typed.trim().to_lowercase();
    if typed.is_empty() {
        return options;
    }
    options
        .into_iter()
        .filter(|o| {
            o.label.to_lowercase().contains(&typed)
                || o.hint
                    .as_deref()
                    .is_some_and(|h| h.to_lowercase().contains(&typed))
        })
        .collect()
}

/// What an edit asks of an issue, with what it may not ask refused by name
/// before a request leaves.
pub(crate) struct Edit<'a> {
    pub(crate) title: Option<&'a str>,
    pub(crate) assignee: Option<&'a Assignee>,
    pub(crate) priority: Option<&'a str>,
    pub(crate) labels: Option<&'a LabelChange>,
    pub(crate) fields: &'a std::collections::BTreeMap<String, Value>,
}

impl Edit<'_> {
    /// Refuse an edit with nothing in it, a priority, a field Gitea's issue
    /// does not have, or a due date that is not a day, before anything is
    /// sent.
    fn check(&self, key: &str) -> Result<(), SourceError> {
        if self.title.is_none()
            && self.assignee.is_none()
            && self.priority.is_none()
            && self.labels.is_none_or(LabelChange::is_empty)
            && self.fields.is_empty()
        {
            return Err(SourceError::protocol(format!(
                "gitea: an edit of {key} with nothing to change"
            )));
        }
        if self.priority.is_some() {
            return Err(SourceError::protocol(format!(
                "gitea: {key} has no priority of its own: knobas reads it from a scoped label, \
                 so change the label"
            )));
        }
        if let Some(id) = self.fields.keys().find(|id| *id != DUE) {
            return Err(SourceError::protocol(format!(
                "gitea: {id:?} is not a field of a Gitea issue"
            )));
        }
        if let Some(due) = self.fields.get(DUE)
            && due_patch(due).is_none()
        {
            return Err(SourceError::protocol(format!(
                "gitea: {key}'s due date must be a day (YYYY-MM-DD) or nothing, not {due}"
            )));
        }
        Ok(())
    }
}

/// Edit issue `owner/repo#number`: its title and assignee in one `PATCH`,
/// then its labels, each added by id (resolved through the labels the
/// repository may use) and each removed by the id the issue carries it under.
/// A name added that the issue carries, or removed that it does not, sends
/// nothing; a name the repository has no label for is refused by name. A
/// resend sets what is already set.
///
/// The issue is read first when the assignee or the labels change: an issue
/// with several assignees is refused one (it would drop the others), and the
/// labels it carries are what a change is made against.
///
/// # Errors
///
/// [`SourceError::Protocol`] for what [`Edit::check`] refuses, and as each
/// request maps.
pub(crate) async fn edit(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    number: u64,
    edit: &Edit<'_>,
) -> Result<(), SourceError> {
    let key = format!("{owner}/{repo}#{number}");
    edit.check(&key)?;
    let labels = edit.labels.filter(|l| !l.is_empty());
    let issue = if edit.assignee.is_some() || labels.is_some() {
        Some(issue_of(&source.client.issue(owner, repo, number).await?))
    } else {
        None
    };
    if let (Some(_), Some(issue)) = (edit.assignee, &issue)
        && issue.assignees.len() > 1
    {
        return Err(SourceError::protocol(format!(
            "gitea: {key}: {}",
            several(issue.assignees.len(), source.display_name())
        )));
    }
    let path = format!("{}/{number}", issues_path(owner, repo)?);
    let mut patch = serde_json::Map::new();
    if let Some(title) = edit.title {
        patch.insert("title".into(), json!(title));
    }
    if let Some(assignee) = edit.assignee {
        // Replaced, never appended: the issue's one assignee becomes this one.
        patch.insert(
            "assignees".into(),
            json!(assignee.account().into_iter().collect::<Vec<_>>()),
        );
    }
    if let Some((key, value)) = edit.fields.get(DUE).and_then(due_patch) {
        patch.insert(key.into(), value);
    }
    if !patch.is_empty() {
        source.client.patch(&path, &Value::Object(patch)).await?;
    }
    let (Some(change), Some(issue)) = (labels, issue) else {
        return Ok(());
    };
    let carried = |name: &str| {
        issue
            .labels
            .iter()
            .find(|(_, n)| n == name)
            .map(|(id, _)| *id)
    };
    let to_add: Vec<&String> = change.add.iter().filter(|n| carried(n).is_none()).collect();
    if !to_add.is_empty() {
        let offered = crate::view::pr::label::read(source, owner, repo)
            .await
            .map_err(SourceError::protocol)?;
        let mut ids = Vec::new();
        for name in to_add {
            let id = offered
                .iter()
                .find(|l| &l.name == name)
                .map(|l| l.id)
                .ok_or_else(|| {
                    SourceError::protocol(format!(
                        "gitea: {owner}/{repo} has no label named {name:?}: a label is the \
                         repository's to make"
                    ))
                })?;
            ids.push(id);
        }
        source
            .client
            .post(&format!("{path}/labels"), &json!({ "labels": ids }))
            .await?;
    }
    for id in change.remove.iter().filter_map(|n| carried(n)) {
        source.client.delete(&format!("{path}/labels/{id}")).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(assignees: &[(&str, Option<&str>)]) -> Issue {
        Issue {
            title: "Retry SEPA".into(),
            due: Some("2026-12-01".into()),
            assignees: assignees
                .iter()
                .map(|(l, n)| ((*l).to_owned(), n.map(str::to_owned)))
                .collect(),
            labels: vec![(7, "payments".into())],
        }
    }

    #[test]
    fn an_issue_offers_its_title_its_one_assignee_and_its_labels_and_no_priority() {
        let fields = answer(
            &issue(&[("mara", Some("Mara Lindqvist"))]),
            "o",
            "r",
            "Gitea",
        );
        assert_eq!(ticket::fault(&fields), None);
        let ids: Vec<&str> = fields.fields.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["title", "assignee", "labels", "due_date"]);
        let due = fields.get(DUE).unwrap();
        assert_eq!(
            (&due.kind, &due.value, due.required),
            (&TicketFieldKind::Date, &json!("2026-12-01"), false)
        );
        let who = fields.get("assignee").unwrap();
        assert_eq!(who.value, json!("mara"));
        assert_eq!(who.shown.as_deref(), Some("Mara Lindqvist"));
        assert_eq!(who.readonly, None);
        assert_eq!(
            who.kind,
            TicketFieldKind::Search {
                scope: "o/r".into(),
                person: true
            }
        );
        assert_eq!(fields.get("labels").unwrap().value, json!(["payments"]));
        let none = answer(&issue(&[]), "o", "r", "Gitea");
        assert_eq!(none.get("assignee").unwrap().value, Value::Null);
        let undated = Issue {
            due: None,
            ..issue(&[])
        };
        assert_eq!(
            answer(&undated, "o", "r", "Gitea").get(DUE).unwrap().value,
            Value::Null
        );
    }

    #[test]
    fn a_due_date_is_read_as_its_day_and_sent_at_the_days_start_or_cleared() {
        let read = issue_of(&json!({ "title": "t", "due_date": "2026-12-01T23:59:59+01:00" }));
        assert_eq!(read.due.as_deref(), Some("2026-12-01"));
        assert_eq!(
            issue_of(&json!({ "title": "t", "due_date": null })).due,
            None
        );
        assert_eq!(
            due_patch(&json!("2026-12-01")),
            Some(("due_date", json!("2026-12-01T00:00:00Z")))
        );
        assert_eq!(
            due_patch(&Value::Null),
            Some(("unset_due_date", json!(true)))
        );
        assert_eq!(due_patch(&json!("2026-13-01")), None);
        assert_eq!(due_patch(&json!(20261201)), None);
    }

    #[test]
    fn an_issue_with_several_assignees_offers_none_of_them() {
        let fields = answer(
            &issue(&[("mara", None), ("ben", None), ("olu", None)]),
            "o",
            "r",
            "Forgejo",
        );
        assert_eq!(
            fields.get("assignee").unwrap().readonly.as_deref(),
            Some("This issue has 3 assignees. Change them in Forgejo.")
        );
    }

    #[test]
    fn people_read_by_name_and_are_matched_by_name_or_login() {
        let users = vec![
            crate::model::UserRef {
                login: Some("olu".into()),
                full_name: Some(String::new()),
            },
            crate::model::UserRef {
                login: Some("mara".into()),
                full_name: Some("Mara Lindqvist".into()),
            },
        ];
        let options = people(users);
        assert_eq!(options[0].label, "Mara Lindqvist");
        assert_eq!(options[0].hint.as_deref(), Some("mara"));
        assert_eq!(options[1].label, "olu");
        assert_eq!(options[1].hint, None);
        assert_eq!(matching(options.clone(), "LIND").len(), 1);
        assert_eq!(matching(options.clone(), "olu")[0].value, "olu");
        assert_eq!(matching(options, " ").len(), 2);
        let names = label_names(["a", "b", "a"].into_iter());
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn an_edit_is_refused_before_a_request_for_what_gitea_cannot_take() {
        let none = std::collections::BTreeMap::new();
        let base = Edit {
            title: None,
            assignee: None,
            priority: None,
            labels: None,
            fields: &none,
        };
        assert!(
            base.check("o/r#1")
                .unwrap_err()
                .to_string()
                .contains("nothing")
        );
        let priority = Edit {
            priority: Some("High"),
            ..base
        };
        assert!(
            priority
                .check("o/r#1")
                .unwrap_err()
                .to_string()
                .contains("priority")
        );
        let own: std::collections::BTreeMap<String, Value> =
            [("due".to_owned(), json!("2026-12-01"))].into();
        let field = Edit {
            fields: &own,
            ..base
        };
        assert!(
            field
                .check("o/r#1")
                .unwrap_err()
                .to_string()
                .contains("\"due\"")
        );
        let bad_day: std::collections::BTreeMap<String, Value> =
            [(DUE.to_owned(), json!("tomorrow"))].into();
        assert!(
            Edit {
                fields: &bad_day,
                ..base
            }
            .check("o/r#1")
            .unwrap_err()
            .to_string()
            .contains("must be a day")
        );
        let good_day: std::collections::BTreeMap<String, Value> =
            [(DUE.to_owned(), json!("2026-12-01"))].into();
        assert!(
            Edit {
                fields: &good_day,
                ..base
            }
            .check("o/r#1")
            .is_ok()
        );
        let empty_labels = LabelChange::default();
        let labels = Edit {
            labels: Some(&empty_labels),
            ..base
        };
        assert!(labels.check("o/r#1").is_err());
        let title = Edit {
            title: Some("x"),
            ..base
        };
        assert!(title.check("o/r#1").is_ok());
    }
}
