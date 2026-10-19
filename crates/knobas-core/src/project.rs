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
//! There is no normalized project model. Contract §4.1 normalizes four fields and a project is not one of them, so
//! the value can only come out of the payload in the source's own shape --
//! which makes this a **payload read outside an adapter**, governed by
//! ADR-0007. Since #277 it is a **declared** read: the source says where its
//! project key and name live ([`crate::payload`]), per entity kind, and
//! [`project_key_read!`] and `declared_string!(…, "project_name")` resolve the
//! declaration.
//! A third source's spelling is one entry on that source's descriptor. Since
//! #759 the database runs those two reads when an item or a declaration
//! changes and keeps the answer in `sync.item_project` (migration `0032`,
//! ADR-0019), so the census reads stored rows rather than walking the mirror.
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
//! `knobas-core/tests/it/projects.rs`; and again for the build configuration's
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
/// a project is. The census's own copy is the one exception, and it is held
/// to this: migration `0032` stores this read's expansion as text, and
/// `knobas-app`'s `the_stored_resolver_is_the_macros_expansion` fails when the
/// two differ. `$decl` is where the calling statement binds the declarations.
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

/// SQL for "some project of the stored chain `$path` (`sync.item_project.path`,
/// #1019) is one of `$values`, by key or name", ignoring case: `$values` is a
/// lowercased `text[]`. A parent project takes in its subprojects this way,
/// and both `project:` readers, a link-set pattern's and the launcher's, expand
/// it, so they cannot disagree.
#[doc(hidden)]
#[macro_export]
macro_rules! project_chain_has {
    ($path:literal, $values:literal) => {
        concat!(
            "exists (select 1 from jsonb_array_elements(",
            $path,
            ") up(pr) where lower(up.pr->>'key') = any(",
            $values,
            ") or lower(up.pr->>'name') = any(",
            $values,
            "))"
        )
    };
}

/// One project the live corpus shows.
///
/// Flat, and carrying its source rather than being grouped under it: the
/// switcher wants a room list, and grouping here would only be ungrouped
/// there.
///
/// ADR-0010, issue #208.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
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
/// Read off `sync.item_project` (migration `0032`, #759): the database keeps
/// what [`project_key_read!`] and `declared_string!(…, "project_name")` yield per item, per
/// stored adapter, from `sync.parent_declaration` (ADR-0019). It used to walk
/// every live item through those two reads on every call: 341-461 ms on the
/// 22,000-item responsiveness probe (#753), re-read after every finished sync.
///
/// Joined to `knobas.source_config` on the row's `adapter_kind`: a stored row
/// says what one adapter's declarations make of the item, and it counts only
/// while the item's source is configured with that adapter -- the rule
/// `knobas_app::sources::declared_paths` applied when the declarations were
/// bound per read, so a source that is not configured, or whose adapter kind
/// is not compiled in, still shows no projects.
///
/// `sync.live_item` and never `sync.item`, which is what keeps a tombstoned
/// item -- and, since migration `0012`, every item of a source the user turned
/// off -- from vouching for a project the live corpus no longer shows.
///
/// One row per project however its items disagree. A project renamed
/// upstream leaves older items carrying the older name and an item may carry
/// the key with no name at all, and either would otherwise split one project
/// into two rooms holding the same work. The order decides which name wins --
/// a readable one over none, then the newest (`at`, the item's
/// `coalesce(item_updated_at, synced_at)`, kept on the row by the trigger) --
/// so a rename shows the new name and a nameless item erases nothing. The
/// trailing `p.name` only breaks a tie between two names recorded at the same
/// instant, and exists so the answer does not depend on which row PostgreSQL
/// reached first.
///
/// **Shaped so the mirror's size costs one narrow scan** (#759): the distinct
/// projects first, a hash over `sync.item_project`'s few columns, then for each one a `limit 1` walk of `item_project_census_idx` in
/// exactly that order, which stops at the project's first live row. Joining
/// every row to `sync.live_item` and sorting was 57 ms at 22,000 items with
/// fresh statistics and 220 ms without. The outer `coalesce(s.enabled,
/// true)` is `sync.live_item`'s own source test, repeated so a turned-off
/// source's projects are skipped before their rows are walked; the view stays
/// the rule.
///
/// `knobas-app`'s `tests/it/project_census.rs` holds this to the walk it
/// replaced after every change the trigger follows.
const PROJECTS: &str = "\
select pr.source_id, pr.key, w.name
  from (select distinct p.source_id, p.key, p.adapter_kind from sync.item_project p) pr
  join knobas.source_config s on s.id = pr.source_id and s.kind = pr.adapter_kind
 cross join lateral (
   select p.name from sync.item_project p
     join sync.live_item i on i.entity_id = p.entity_id
    where p.source_id = pr.source_id and p.key = pr.key and p.adapter_kind = pr.adapter_kind
    order by (p.name is null), p.at desc, p.name
    limit 1) w
 where coalesce(s.enabled, true)
 order by pr.source_id, pr.key";

/// The projects the live corpus shows, ordered by source then key.
///
/// Unscoped on purpose: the caller is the switcher, which offers a room for
/// every project every source shows, and a per-source read would be one round
/// trip per source to assemble the same list.
///
/// The declarations are the database's stored copy (`sync.parent_declaration`,
/// written by `knobas_search::Searcher::declaring`), so a caller must have
/// stored them first -- bring-up does, and `knobas_app`'s `list_projects`
/// does on its way in, as every search command does.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn list(pool: &PgPool) -> Result<Vec<Project>, CoreError> {
    let rows: Vec<(String, String, Option<String>)> =
        sqlx::query_as(PROJECTS).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(source_id, key, name)| Project {
            source_id,
            key,
            name,
        })
        .collect())
}
