//! The **project** a mirrored record carries, and the projects a corpus shows
//! (ADR-0010; spec #188 asks for both, #208 builds them).
//!
//! A project is a source's own grouping of its items, in the source's own
//! word: a Jira project, a TeamCity project. It is a scoping dimension a room
//! narrows by, never an entity kind -- a Gitea repository is the other thing
//! and has a kind of its own.
//!
//! # A payload read, and which way it fails
//!
//! There is no normalized project model and no migration behind any of this.
//! Contract §4.1 normalizes four fields and a project is not one of them, so
//! the value can only come out of the payload in the source's own shape --
//! which makes this a **payload read outside an adapter**, governed by
//! ADR-0007. Since #277 it is a **declared** read: the source says where its
//! project key and name live ([`crate::payload`]), per entity kind, and
//! [`project_key_read!`] and [`project_name_read!`] resolve the declaration.
//! A third source's spelling is one entry on that source's descriptor.
//!
//! **Failure direction (ADR-0007 requirement 3): absence, never a wrong
//! room.** A record carrying no readable project *key* belongs to no project
//! room, and is still in *All work* and in its source's room -- so nothing is
//! hidden by a dimension the record does not carry, and there is no "No
//! project" room to put it in (ADR-0010 refuses one, because two other rooms
//! already hold it). This is deliberately *unlike* the mini board's terminal
//! group, where the column is the only place a statusless ticket could appear
//! at all. A record with a readable key and no readable name is carried by its
//! key: reported, with [`Project::name`] empty. Pinned per direction by
//! `a_project_key_that_is_not_a_string_contributes_nothing`,
//! `a_blank_project_key_contributes_nothing`,
//! `a_whitespace_only_project_key_contributes_nothing`,
//! `a_record_with_no_project_at_all_contributes_nothing` and
//! `a_project_with_no_readable_name_is_reported_by_its_key` in
//! `knobas-core/tests/projects.rs`; and again for the build configuration's
//! own spelling (#232) by
//! `a_build_configuration_with_no_readable_project_contributes_nothing`,
//! `a_build_configuration_with_no_readable_name_is_reported_by_its_key` and
//! `a_top_level_project_id_on_any_other_kind_contributes_nothing` beside them.
//!
//! # Why a census rather than a room's own scan
//!
//! The switcher cannot derive its project rooms from a room's own read: that
//! one is a window over the newest items and explicitly not a census
//! (`Room.svelte`), so a quiet project would silently have no room. [`list`]
//! answers over the whole **live** corpus instead.

use serde::Serialize;
use sqlx::PgPool;

use crate::CoreError;

/// A record's project **key**, at the path its own source declares for its
/// kind (#277).
///
/// Exported, because narrowing by a project happens in the statements that
/// draw a room -- [`crate::mini_board`]'s here and
/// `knobas_app::commands::entity`'s across the bridge -- and a second copy of
/// this read is how one of them starts disagreeing with the census about what
/// a project is. `$decl` is where the calling statement binds the
/// declarations.
///
/// **The kind guard is now the declaration itself.** A TeamCity build names
/// its project on the `buildType` it ran; a build *configuration*'s record
/// **is** that `buildType` object and names it at the top level. That used to
/// be a `case when i.kind = 'build_config'` arm here (#232, ruled at triage
/// because a top-level `projectId` is a less distinctive path than a
/// container-scoped one, and a future adapter's incidental one must not
/// silently open a room). It is two per-kind declarations on the TeamCity
/// descriptor now, and the guard is stronger than the arm it replaces: no
/// adapter's records can reach a path no adapter declared.
///
/// **Failure direction (ADR-0007 requirement 3), unchanged:** a record whose
/// declared key resolves to nothing -- an absent path, a non-string, a blank
/// -- belongs to no project room, and is still in *All work* and in its
/// source's room. Pinned by the tests named in this module's header.
#[macro_export]
macro_rules! project_key_read {
    ($decl:literal) => {
        $crate::declared_string!($decl, "project_key")
    };
}

/// A record's project **name**, at the path its own source declares.
///
/// Not exported: a room narrows by the *key*, which is its identity, and the
/// name is only ever read here, where the census is taken. A reader that
/// narrowed by a name would be narrowing by a label the source may rewrite.
macro_rules! project_name_read {
    ($decl:literal) => {
        $crate::declared_string!($decl, "project_name")
    };
}

/// One project the live corpus shows.
///
/// Flat, and carrying its source rather than being grouped under it: the
/// switcher wants a room list, and grouping here would only be ungrouped
/// there.
#[derive(Clone, Debug, Serialize)]
pub struct Project {
    /// Which source shows it.
    ///
    /// Half of the identity, not decoration: a project key is unique only
    /// inside its own source, so this is what keeps two sources' `PAY` two
    /// projects and two rooms.
    pub source_id: String,
    /// The source's own key for it (`PAY`).
    pub key: String,
    /// The source's own name for it, or `None` where no name is readable.
    ///
    /// `None` rather than a copy of the key: what to *label* a room with is
    /// the shell's decision, and a name knobas invented would be
    /// indistinguishable on the wire from one the source said.
    pub name: Option<String>,
}

/// Every project the live corpus shows, one row each, ordered by source then
/// key.
///
/// `sync.live_item` and never `sync.item`, which is what keeps a tombstoned
/// item -- and, since migration `0012`, every item of a source the user turned
/// off -- from vouching for a project the live corpus no longer shows.
///
/// `distinct on` rather than a `group by` over all three columns: one project
/// is one row however its items disagree. A project renamed upstream leaves
/// older items carrying the older name and an item may carry the key with no
/// name at all, and either would otherwise split one project into two rooms
/// holding the same work. The `order by` decides which name wins -- a readable
/// one over none, then the newest -- so a rename shows the new name and a
/// nameless item erases nothing. Its trailing `observed.name` only breaks a
/// tie between two names recorded at the same instant, and exists so the
/// answer does not depend on which row PostgreSQL reached first.
const PROJECTS: &str = concat!(
    "select distinct on (observed.source_id, observed.key)
             observed.source_id, observed.key, observed.name
       from (select i.source_id as source_id, ",
    project_key_read!("$1"),
    " as key, ",
    project_name_read!("$1"),
    " as name,
                    coalesce(i.item_updated_at, i.synced_at) as at
               from sync.live_item i) observed
      where observed.key is not null
      order by observed.source_id, observed.key,
               (observed.name is null), observed.at desc, observed.name"
);

/// The projects the live corpus shows, ordered by source then key.
///
/// Unscoped on purpose: the caller is the switcher, which offers a room for
/// every project every source shows, and a per-source read would be one round
/// trip per source to assemble the same list.
///
/// `declarations` is what every configured source says about where it keeps a
/// project (#277), assembled by `knobas_app::sources::declared_paths`. A
/// source that declares nothing shows no projects, which is this read's stated
/// failure direction reaching the one case where knobas knows nothing about a
/// source at all.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn list(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
) -> Result<Vec<Project>, CoreError> {
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(PROJECTS)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(source_id, key, name)| Project {
            source_id,
            key,
            name,
        })
        .collect())
}
