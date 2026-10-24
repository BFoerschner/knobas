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

/// The permission each op needs: the op, the key `mypermissions` answers it
/// under, its name in Jira's permission scheme, and what the reason says the
/// user cannot do.
const NEEDS: [(&str, &str, &str, &str); 4] = [
    ("comment", "ADD_COMMENTS", "Add Comments", "comment"),
    (
        "transition",
        "TRANSITION_ISSUES",
        "Transition Issues",
        "move tickets",
    ),
    ("log_work", "WORK_ON_ISSUES", "Work On Issues", "log work"),
    (
        "create_ticket",
        "CREATE_ISSUES",
        "Create Issues",
        "create tickets",
    ),
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
    let (_, key, name, verb) = NEEDS.iter().find(|(op, ..)| *op == ask.op)?;
    if answer.permissions.get(*key)?.have_permission {
        return None;
    }
    Some(RefusedWrite::of(
        ask,
        format!("This Jira user cannot {verb} in {project}: it lacks the {name} permission"),
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
        if NEEDS.iter().any(|(op, ..)| *op == ask.op) && !projects.contains(&project.as_str()) {
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

    /// What the answer does not say, and an op no permission gates, are no
    /// verdict.
    #[test]
    fn a_permission_the_answer_leaves_out_refuses_nothing() {
        let held = answer(r#"{"permissions":{"BROWSE_PROJECTS":{"havePermission":false}}}"#);
        for op in ["comment", "transition", "log_work", "create_ticket", "rerun_build"] {
            assert_eq!(refusal(&held, "PAY", &ask("jira:PAY-231", op)), None, "{op}");
        }
    }
}
