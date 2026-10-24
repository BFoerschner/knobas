//! TeamCity's project tree (#849, R-9.3): `/app/rest/projects` read once a
//! run, for two things.
//!
//! * **A configuration's project path.** The Builds list draws the tree as
//!   deep as TeamCity's goes, and a configuration's record names only its
//!   own project (`projectId`, `projectName`). So the adapter writes the
//!   chain beside the record as `projectPath`, outermost first and ending in
//!   the configuration's own project, below the server's root `_Root`:
//!   `[{"id": "Payments", "name": "Payments"}, {"id": "Payments_Payouts",
//!   "name": "Payouts"}]`. The descriptor declares it as the pipeline's
//!   `group_path`. It is the third departure from §3a's verbatim payload,
//!   for the reason ADR-0018 gives Gitea's repository and Jira's epic: the
//!   fact is the source's, and no one record carries it.
//! * **The scope.** A configured `project_ids` entry takes in its
//!   subprojects at every depth, as TeamCity's own project page does.
//!
//! A project the listing does not name ends the chain there: a path is never
//! guessed, and a cycle (which TeamCity does not allow) cannot loop.

use std::collections::{HashMap, HashSet};

use crate::client::Rec;
use crate::rest::Project;

/// The id TeamCity gives the root project every other project sits under.
pub(crate) const ROOT: &str = "_Root";

/// The payload key a configuration's chain of projects is written under.
pub(crate) const PROJECT_PATH: &str = "projectPath";

/// Every project, by id.
#[derive(Debug, Default)]
pub(crate) struct ProjectTree {
    by_id: HashMap<String, Project>,
}

impl ProjectTree {
    pub(crate) fn new(projects: impl IntoIterator<Item = Rec<Project>>) -> Self {
        Self {
            by_id: projects
                .into_iter()
                .map(|p| (p.rec.id.clone(), p.rec))
                .collect(),
        }
    }

    /// The chain from the outermost project below the root down to `id`
    /// itself, outermost first. Empty for the root, and for a project the
    /// listing does not name.
    pub(crate) fn path(&self, id: &str) -> Vec<&Project> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut at = Some(id);
        while let Some(current) = at {
            if current == ROOT || !seen.insert(current) {
                break;
            }
            let Some(project) = self.by_id.get(current) else {
                break;
            };
            out.push(project);
            at = project.parent_project_id.as_deref();
        }
        out.reverse();
        out
    }

    /// The name a person knows `id` by: its name, else its id.
    pub(crate) fn name_of(&self, id: &str) -> String {
        self.by_id
            .get(id)
            .and_then(|p| p.name.as_deref())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(id)
            .to_owned()
    }

    /// Whether `id` is one of `roots` or sits anywhere beneath one.
    pub(crate) fn within(&self, id: &str, roots: &[String]) -> bool {
        if roots.iter().any(|r| r == id) {
            return true;
        }
        self.path(id).iter().any(|p| roots.contains(&p.id))
    }

    /// `raw` with its project's chain written beside it under
    /// [`PROJECT_PATH`]. A record with no `projectId` is left as it is.
    pub(crate) fn annotate(
        &self,
        raw: &serde_json::Value,
        project_id: Option<&str>,
    ) -> serde_json::Value {
        let mut out = raw.clone();
        if let (Some(id), Some(obj)) = (project_id, out.as_object_mut()) {
            let chain: Vec<serde_json::Value> = self
                .path(id)
                .into_iter()
                .map(|p| {
                    serde_json::json!({
                        "id": p.id,
                        "name": p.name.clone().unwrap_or_else(|| p.id.clone()),
                    })
                })
                .collect();
            obj.insert(PROJECT_PATH.to_owned(), serde_json::Value::Array(chain));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: &str, name: &str, parent: Option<&str>) -> Rec<Project> {
        let raw = serde_json::json!({ "id": id, "name": name, "parentProjectId": parent });
        Rec {
            rec: serde_json::from_value(raw.clone()).expect("parses"),
            raw,
        }
    }

    fn payments() -> ProjectTree {
        ProjectTree::new([
            project(ROOT, "<Root project>", None),
            project("Payments", "Payments", Some(ROOT)),
            project("Payments_Payouts", "Payouts", Some("Payments")),
            project(
                "Payments_Payouts_Deploy",
                "Deploy",
                Some("Payments_Payouts"),
            ),
            project("Data", "Data", Some(ROOT)),
        ])
    }

    #[test]
    fn a_path_runs_from_below_the_root_down_to_the_project_itself() {
        let tree = payments();
        let names: Vec<&str> = tree
            .path("Payments_Payouts_Deploy")
            .iter()
            .map(|p| p.name.as_deref().unwrap())
            .collect();
        assert_eq!(names, ["Payments", "Payouts", "Deploy"]);
        assert_eq!(tree.path("Data").len(), 1);
        assert!(tree.path(ROOT).is_empty());
        assert!(tree.path("Nowhere").is_empty());
    }

    #[test]
    fn a_configured_project_takes_in_its_subprojects_at_every_depth() {
        let tree = payments();
        let roots = vec!["Payments".to_owned()];
        assert!(tree.within("Payments", &roots));
        assert!(tree.within("Payments_Payouts_Deploy", &roots));
        assert!(!tree.within("Data", &roots));
        // A project the listing does not name is in scope only by its own id.
        assert!(!tree.within("Unlisted", &roots));
        assert!(tree.within("Unlisted", &["Unlisted".to_owned()]));
    }

    #[test]
    fn a_cycle_ends_the_chain_rather_than_looping() {
        let tree = ProjectTree::new([project("A", "A", Some("B")), project("B", "B", Some("A"))]);
        assert_eq!(tree.path("A").len(), 2);
    }

    #[test]
    fn the_chain_is_written_beside_the_record() {
        let raw = serde_json::json!({ "id": "Payout_Deploy_Prod", "projectId": "Payments_Payouts_Deploy" });
        let out = payments().annotate(&raw, Some("Payments_Payouts_Deploy"));
        assert_eq!(out["id"], "Payout_Deploy_Prod");
        assert_eq!(
            out[PROJECT_PATH],
            serde_json::json!([
                { "id": "Payments", "name": "Payments" },
                { "id": "Payments_Payouts", "name": "Payouts" },
                { "id": "Payments_Payouts_Deploy", "name": "Deploy" },
            ])
        );
        // A configuration straight under the root has an empty chain.
        assert_eq!(
            payments().annotate(&raw, Some(ROOT))[PROJECT_PATH],
            serde_json::json!([])
        );
        assert!(payments().annotate(&raw, None).get(PROJECT_PATH).is_none());
    }
}
