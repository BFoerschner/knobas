//! What this Jira user may not write (#2052, #2040's SPI).
//!
//! A Data Center PAT, and a user and password, act with their user's project
//! permissions, so the four writes this adapter declares each hang on one of
//! them: a comment on *Add Comments*, a move on *Transition Issues*, a worklog
//! on *Work On Issues* and a create on *Create Issues*. A surface that draws
//! one of them from `write_ops` alone draws it for a user the project's
//! permission scheme refuses, and Jira refuses it only after it was queued.
//!
//! **Read, never probed.** `GET /rest/api/2/mypermissions?projectKey=…` answers
//! every permission with whether this user holds it in that project. One read
//! per project asked about, whatever the number of tickets in it.
//!
//! **The project, not the issue.** A ticket's asks are answered in its
//! project (the key's prefix). Jira answers a permission given through issue
//! data (*Reporter*, *Current Assignee*) as held when the user meets it on
//! *any* issue of the project, so a project answer can only refuse less than
//! an issue answer would, never more: never a refusal on a guess, and one read
//! per project rather than one per ticket on a busy page.
//!
//! The query sends `projectKey` alone. The WADL this adapter is held to
//! (`testenv/specs/jira-dc-rest.wadl`) declares no `permissions` filter on
//! the resource, so the whole answer comes back and the four keys are read
//! out of it.
//!
//! **No verdict refuses nothing.** A read that failed, a project key that is
//! not one, an op that is not one of the four, and a permission the answer
//! does not hold all leave the write drawn as before.

use std::collections::BTreeMap;

use knobas_source::SourceError;
use knobas_source::grants::{RefusedWrite, WriteAsk};

use crate::api::JiraApi;

/// The permission one op needs.
struct Need {
    /// The op, as `write_ops` spells it.
    op: &'static str,
    /// The key `mypermissions` answers the permission under.
    key: &'static str,
    /// The permission's name in Jira's permission scheme.
    name: &'static str,
    /// What the reason says the user cannot do.
    verb: &'static str,
}

/// Each op this adapter declares, and the permission it needs (measured on
/// Jira 10.3.24, #2052).
const NEEDS: [Need; 4] = [
    Need {
        op: crate::WRITE_OP_COMMENT,
        key: "ADD_COMMENTS",
        name: "Add Comments",
        verb: "comment",
    },
    Need {
        op: crate::WRITE_OP_TRANSITION,
        key: "TRANSITION_ISSUES",
        name: "Transition Issues",
        verb: "move tickets",
    },
    Need {
        op: crate::WRITE_OP_LOG_WORK,
        key: "WORK_ON_ISSUES",
        name: "Work On Issues",
        verb: "log work",
    },
    Need {
        op: crate::WRITE_OP_CREATE_TICKET,
        key: "CREATE_ISSUES",
        name: "Create Issues",
        verb: "create tickets",
    },
];

/// `mypermissions`' answer: every permission, keyed, with whether this user
/// holds it.
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct MyPermissions {
    #[serde(default)]
    pub(crate) permissions: BTreeMap<String, Held>,
}

/// One permission of [`MyPermissions`].
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Held {
    pub(crate) have_permission: bool,
}

/// Why `ask` cannot be made in `project`, by `answer`, or `None`.
fn refusal(answer: &MyPermissions, project: &str, ask: &WriteAsk) -> Option<RefusedWrite> {
    let need = NEEDS.iter().find(|need| need.op == ask.op)?;
    if answer.permissions.get(need.key)?.have_permission {
        return None;
    }
    Some(RefusedWrite::of(
        ask,
        format!(
            "This Jira user cannot {} in {project}: it lacks the {} permission",
            need.verb, need.name
        ),
    ))
}

/// The asks among `asks` this user's permissions refuse. Each comes with the
/// project it is answered in; the projects are read one after another, once
/// each.
pub(crate) async fn refused(
    api: &dyn JiraApi,
    asks: &[(&WriteAsk, String)],
) -> Result<Vec<RefusedWrite>, SourceError> {
    let mut projects: Vec<&str> = Vec::new();
    for (ask, project) in asks {
        if NEEDS.iter().any(|need| need.op == ask.op) && !projects.contains(&project.as_str()) {
            projects.push(project);
        }
    }
    let mut out = Vec::new();
    for project in projects {
        // A failed read is no verdict for this project's asks and none other's.
        let Ok(answer) = api.my_permissions(project).await else {
            continue;
        };
        out.extend(
            asks.iter()
                .filter(|(_, of)| of == project)
                .filter_map(|(ask, _)| refusal(&answer, project, ask)),
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(json: &str) -> MyPermissions {
        serde_json::from_str(json).expect("a mypermissions answer")
    }

    fn ask(entity: &str, op: &str) -> WriteAsk {
        WriteAsk {
            entity: entity.to_owned(),
            op: op.to_owned(),
            field: None,
        }
    }

    /// Each op is refused on its own permission and says which one.
    #[test]
    fn each_op_is_refused_on_its_own_permission() {
        let held = answer(
            r#"{"permissions":{
                "ADD_COMMENTS":{"havePermission":false},
                "TRANSITION_ISSUES":{"havePermission":true},
                "WORK_ON_ISSUES":{"havePermission":false},
                "CREATE_ISSUES":{"havePermission":true}}}"#,
        );
        let reason = |op: &str, entity: &str| {
            refusal(&held, "PAY", &ask(entity, op)).map(|refused| refused.reason)
        };
        assert_eq!(
            reason("comment", "jira:PAY-231").as_deref(),
            Some("This Jira user cannot comment in PAY: it lacks the Add Comments permission")
        );
        assert_eq!(
            reason("log_work", "jira:PAY-231").as_deref(),
            Some("This Jira user cannot log work in PAY: it lacks the Work On Issues permission")
        );
        assert_eq!(reason("transition", "jira:PAY-231"), None);
        assert_eq!(reason("create_ticket", "jira:PAY"), None);
    }

    /// One permission lacking refuses its own op and no other: each op reads
    /// its own key, as `mypermissions` names it on Jira 10.3.24.
    #[test]
    fn one_permission_lacking_refuses_its_op_alone() {
        for (lacking, op) in [
            ("ADD_COMMENTS", "comment"),
            ("TRANSITION_ISSUES", "transition"),
            ("WORK_ON_ISSUES", "log_work"),
            ("CREATE_ISSUES", "create_ticket"),
        ] {
            let held = answer(
                &serde_json::json!({ "permissions": {
                    "ADD_COMMENTS": { "havePermission": lacking != "ADD_COMMENTS" },
                    "TRANSITION_ISSUES": { "havePermission": lacking != "TRANSITION_ISSUES" },
                    "WORK_ON_ISSUES": { "havePermission": lacking != "WORK_ON_ISSUES" },
                    "CREATE_ISSUES": { "havePermission": lacking != "CREATE_ISSUES" },
                }})
                .to_string(),
            );
            let refused: Vec<&str> = ["comment", "transition", "log_work", "create_ticket"]
                .into_iter()
                .filter(|each| refusal(&held, "PAY", &ask("jira:PAY-231", each)).is_some())
                .collect();
            assert_eq!(refused, [op], "lacking {lacking}");
        }
    }

    /// What the answer does not say, and an op no permission gates, are no
    /// verdict.
    #[test]
    fn a_permission_the_answer_leaves_out_refuses_nothing() {
        let held = answer(r#"{"permissions":{"BROWSE_PROJECTS":{"havePermission":false}}}"#);
        for op in [
            "comment",
            "transition",
            "log_work",
            "create_ticket",
            "rerun_build",
        ] {
            assert_eq!(
                refusal(&held, "PAY", &ask("jira:PAY-231", op)),
                None,
                "{op}"
            );
        }
    }
}
