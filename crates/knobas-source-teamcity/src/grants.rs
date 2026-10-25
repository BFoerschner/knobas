//! What the TeamCity user may not write (#2040), read from its permissions.
//!
//! A TeamCity token carries its user's roles. A user holding only
//! `PROJECT_VIEWER` may run builds but may not edit a configuration, and
//! TeamCity refuses the parameter `PUT` with a 403 *You do not have enough
//! permissions to edit build configuration*, after the write was queued.
//!
//! **No write is sent to find out.** `POST /app/rest/buildQueue` with `{}`
//! answers 400 *No 'buildType' element* before any permission check, and with
//! a build type it queues a build (measured on 2026.1.3, #2040). So the
//! adapter reads `GET /app/rest/users/current/permissions`: one assignment
//! per permission and scope, each `isGlobalScope` or naming the `project` it
//! was given on (measured on 2026.1.3, 2026-10-24). A role given on a project
//! holds in every project below it, so an assignment holds for a
//! configuration when it is global, or names the configuration's project or
//! one of its ancestors. The project tree is `/app/rest/projects`, read as
//! the sync reads it ([`ProjectTree`]). A server with per-project
//! permissions off makes every role global, and answers every assignment
//! `isGlobalScope`, which this reads as it is.
//!
//! | op | permission |
//! |---|---|
//! | `run_build`, `trigger_build`, `rerun_build` | `run_build` |
//! | `set_parameter` | `edit_project` |
//! | `run_build` carrying `parameters` | `customize_build_parameters` |
//! | `run_build` carrying `first_in_queue` | `reorder_build_queue`, held anywhere |
//!
//! **What a run may carry (#2055).** A run's parameter overrides and its
//! place at the queue's top need permissions of their own, which a custom
//! role can leave out while it gives `run_build`. TeamCity 2026.1.3 checks
//! `customize_build_parameters` on the configuration's project when the
//! run's parameters are set (`BuildPromotionImpl.setCustomParameters`),
//! before anything is queued, so the `POST` is refused and queues nothing.
//! It checks `reorder_build_queue` globally when the queued build is moved
//! to the top (`SecuredBuildQueue.moveTop`, which is the `BuildQueue` the
//! REST plugin is handed), **after** the build was queued: the run starts
//! and the write still answers 403. So the dialog asks about each field
//! ([`WriteAsk::field`]) and draws the refused ones disabled. TeamCity says
//! `reorder_build_queue` cannot be given on a project, so an assignment of
//! it anywhere is read as held rather than as a refusal on a guess.

use std::collections::{HashMap, HashSet};

use knobas_source::SourceError;
use knobas_source::grants::{RefusedWrite, WriteAsk};

use crate::client::{HttpRest, Rest};
use crate::projects::{ProjectTree, ROOT};

/// The permission a TeamCity write needs, by its op, or the one a field of
/// its payload needs beyond the write; `None` for an op this adapter does
/// not declare, or a field that needs nothing more.
fn needs(op: &str, field: Option<&str>) -> Option<Permission> {
    match (op, field) {
        (
            crate::WRITE_OP_RUN_BUILD | crate::WRITE_OP_TRIGGER_BUILD | crate::WRITE_OP_RERUN_BUILD,
            None,
        ) => Some(Permission::RunBuild),
        (crate::WRITE_OP_SET_PARAMETER, None) => Some(Permission::EditProject),
        (crate::WRITE_OP_RUN_BUILD, Some(RUN_PARAMETERS)) => Some(Permission::CustomizeParameters),
        (crate::WRITE_OP_RUN_BUILD, Some(RUN_FIRST_IN_QUEUE)) => Some(Permission::ReorderQueue),
        _ => None,
    }
}

/// `run_build`'s overrides, by the name its payload carries them under.
pub(crate) const RUN_PARAMETERS: &str = "parameters";
/// `run_build`'s *Put first in the queue*, by its payload name.
const RUN_FIRST_IN_QUEUE: &str = "first_in_queue";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Permission {
    RunBuild,
    EditProject,
    CustomizeParameters,
    ReorderQueue,
}

impl Permission {
    /// TeamCity's id for it, as the permissions read spells it.
    fn id(self) -> &'static str {
        match self {
            Permission::RunBuild => "run_build",
            Permission::EditProject => "edit_project",
            Permission::CustomizeParameters => "customize_build_parameters",
            Permission::ReorderQueue => "reorder_build_queue",
        }
    }

    /// Whether TeamCity checks it globally, whatever the project: then an
    /// assignment of it anywhere is read as held, rather than refusing on a
    /// guess about how a project's role carrying it is answered.
    fn is_global(self) -> bool {
        self == Permission::ReorderQueue
    }

    /// Why a user without it cannot make the write, in `project`.
    fn reason(self, project: &str) -> String {
        match self {
            Permission::RunBuild => format!(
                "This TeamCity user cannot run builds in {project}: it lacks the Run build \
                 permission"
            ),
            Permission::EditProject => format!(
                "This TeamCity user cannot change the settings of {project}: it lacks the Edit \
                 project permission"
            ),
            Permission::CustomizeParameters => format!(
                "This TeamCity user cannot override parameters for one build in {project}: it \
                 lacks the Customize build parameters permission"
            ),
            Permission::ReorderQueue => "This TeamCity user cannot put a build first in the \
                                         queue: it lacks the Reorder builds in queue permission"
                .to_owned(),
        }
    }
}

/// What the user holds: its global permissions, and the ones given on each
/// project, with the tree to walk up.
#[derive(Debug, Default)]
pub(crate) struct Grants {
    global: HashSet<String>,
    on_project: HashMap<String, HashSet<String>>,
    tree: ProjectTree,
}

impl Grants {
    /// The user's permissions, then the project tree.
    ///
    /// # Errors
    ///
    /// Whatever either read maps to, and a project listing that reports a
    /// further page, which [`ProjectTree`] cannot walk.
    pub(crate) async fn read(rest: &HttpRest) -> Result<Self, SourceError> {
        let assignments = rest.permissions().await?;
        let projects = rest.projects().await?;
        if projects.more {
            return Err(SourceError::protocol(
                "teamcity: `/app/rest/projects` reported a further page, so a project's \
                 ancestors cannot be told and no write is judged",
            ));
        }
        let mut grants = Grants {
            tree: ProjectTree::new(projects.items),
            ..Grants::default()
        };
        for a in assignments.permission_assignment {
            let id = a.permission.id.to_ascii_lowercase();
            match a.project {
                Some(project) if a.is_global_scope != Some(true) => {
                    grants.on_project.entry(project.id).or_default().insert(id);
                }
                _ => {
                    grants.global.insert(id);
                }
            }
        }
        Ok(grants)
    }

    /// Why `op` cannot be made in `project`, or carry `field` there, or
    /// `None` where the user may, or this adapter governs neither.
    pub(crate) fn refusal(&self, op: &str, field: Option<&str>, project: &str) -> Option<String> {
        let permission = needs(op, field)?;
        let held = |scope: &str| {
            self.on_project
                .get(scope)
                .is_some_and(|ids| ids.contains(permission.id()))
        };
        let holds = self.global.contains(permission.id())
            || if permission.is_global() {
                self.on_project
                    .values()
                    .any(|ids| ids.contains(permission.id()))
            } else {
                held(ROOT) || held(project) || self.tree.path(project).iter().any(|p| held(&p.id))
            };
        (!holds).then(|| permission.reason(&self.tree.name_of(project)))
    }
}

/// The asks among `asks` the user's permissions refuse, in the order asked.
/// Each comes with its entity's key; an entity whose project cannot be read
/// (a build that is gone) is no verdict.
///
/// # Errors
///
/// [`Grants::read`]'s. Nothing is read when no ask is one this adapter
/// governs.
pub(crate) async fn refused(
    rest: &HttpRest,
    asks: &[(&WriteAsk, String)],
) -> Result<Vec<RefusedWrite>, SourceError> {
    let governed: Vec<_> = asks
        .iter()
        .filter(|(a, _)| needs(&a.op, a.field.as_deref()).is_some())
        .collect();
    if governed.is_empty() {
        return Ok(Vec::new());
    }
    let grants = Grants::read(rest).await?;
    let mut projects: HashMap<&str, Option<String>> = HashMap::new();
    let mut out = Vec::new();
    for (ask, key) in governed {
        if !projects.contains_key(key.as_str()) {
            projects.insert(key, project_of(rest, key).await);
        }
        let Some(Some(project)) = projects.get(key.as_str()) else {
            continue;
        };
        if let Some(reason) = grants.refusal(&ask.op, ask.field.as_deref(), project) {
            out.push(RefusedWrite::of(ask, reason));
        }
    }
    Ok(out)
}

/// The project of a configuration (`buildType:<id>`) or of a build's
/// configuration (`build:<id>`), or `None` where it cannot be read.
async fn project_of(rest: &HttpRest, key: &str) -> Option<String> {
    if let Some(id) = key.strip_prefix("buildType:") {
        return rest
            .config_project(crate::ids::config_id(key, id).ok()?)
            .await
            .ok();
    }
    let id = key.strip_prefix("build:")?;
    let id: i64 = id.parse().ok().filter(|n| *n > 0)?;
    rest.build_project(id).await.ok()
}
