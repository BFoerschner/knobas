//! A ticket system's saved views (#1207, ADR-0029; first Jira's views, #877,
//! migration `0044`): every live item of a source's saved-view kinds,
//! projected into `sync.saved_view` and `sync.saved_view_member` at the end of
//! each of that source's runs, and each one's `knobas.ticket_view` row kept in
//! step.
//!
//! **Keyed on the frame, not on an adapter (#1207).** A saved-view kind is one
//! whose [`KindPaths`](knobas_core::payload::KindPaths) declares
//! [`Frame::SavedView`], the ticket-system minimum's optional kind
//! (`knobas_source::role`); Jira's `jira_view` is one. Nothing here names an
//! adapter: the group a view is listed under is the ticket system's name the
//! kind declares (`system_name`, else the descriptor's `name`) with *boards*
//! or *filters* after it, so Jira's read *Jira boards* and *Jira filters* as
//! before, whatever the source is called. The tables keep the names #877
//! gave them.
//!
//! **Off the mirror, not off the emitted items**, for `samples`' reason: the
//! Jira adapter delivers a view only when it changed, so an idle run emits
//! nothing and the views are still exactly what they were. Reading the
//! mirror makes every run say so, and `last_read` is the run that did.
//!
//! **After the sweep and inside the run's transaction** (`run_locked`): a view
//! the adapter delivered as gone, or a full sync's sweep tombstoned, is
//! already out of `sync.live_item`, so its rows go here -- members by the
//! foreign key's cascade -- and its ticket view with it. A run that fails
//! leaves nothing behind.
//!
//! **What the ticket view row takes from the source and what it keeps.** The name,
//! the group (*Jira boards* or *Jira filters*: `<system name> boards`, or
//! `filters` for a view whose `kind` is `filter`) and the columns follow Jira at
//! every run (R-3.24); the layout, lanes, card fields, list columns and sort
//! are the reader's (*View settings*) and are written only when the row is
//! first made -- a filter's as a list, since Jira gives a filter no columns
//! (#878). A row made while the source already had Jira views is marked
//! `is_new` (*new* until opened, R-3.20); the views of a source's first run
//! are not. `hidden` is the reader's and never written here. The row's id
//! is the item's entity id as a slug
//! (`jira:board-21` → `jira-board-21`), so `#/tickets/<id>` is stable. A view
//! somebody made that already holds that id is left alone, and the Jira view
//! does not show: a name collision costs a view, never somebody's own.
//!
//! **Payload reads outside the adapter (ADR-0007).** A view's query, board
//! type, sub-query and sprint are read at the paths its kind declares
//! (`source_query`, `board_type`, `sub_query`, `sprint` on
//! [`KindPaths`](knobas_core::payload::KindPaths), #2050), the ticket-system
//! minimum's saved-view paths (`knobas_source::role`), and at no key named
//! after any source: Jira declares its `jql`, `type`, `sub_query` and
//! `sprint`, which `crate::views` in `knobas-source-jira` writes. The rest is
//! the frame's own shape, which every saved-view kind writes: `kind` (`board`
//! or `filter`), `id`, `columns` (`[{"name", "statuses", "limit"}]`), `keys`
//! (the tickets' keys in the source's order) and `query` (the query the keys
//! were read with). Read in these statements and nowhere else. **A miss is an
//! empty answer, never a guess**: no query at the declared path is the empty
//! query, a payload whose `columns` is not an array has no columns (one per
//! status on the board), one whose `keys` is not an array holds no tickets, a
//! sprint that is no object is none, and a `kind` that is missing or not
//! `filter` is a board. Pinned by `tests/it/jira_views.rs`'s
//! `a_view_whose_payload_says_nothing_holds_nothing`,
//! `a_saved_view_of_a_ticket_system_that_is_not_jira_shows_as_a_ticket_view`
//! and `a_saved_view_keeps_the_query_its_kind_declares_and_no_key_named_after_jira`.

use knobas_core::payload::Frame;
use knobas_source::SourceDescriptor;
use sqlx::{Postgres, Transaction};

/// What a descriptor declares of saved views: the kinds they arrive as, and
/// the name their groups are made from. `None` when it declares none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedViewKinds {
    /// Each kind's declaration: where its views' query, board type,
    /// sub-query and sprint are (#2050).
    paths: Vec<knobas_core::payload::KindPaths>,
    source_name: String,
}

impl SavedViewKinds {
    /// The ticket system the views are listed under (`Jira`): what their
    /// group is made from, and what the shell's words about them name (#2032).
    #[must_use]
    pub fn source_name(&self) -> &str {
        &self.source_name
    }
}

/// The saved-view kinds `descriptor` emits: each declares
/// [`Frame::SavedView`] and is one of its `entity_kinds`. Their groups take
/// the first one's declared `system_name`, else the descriptor's `name`: one
/// source is one ticket system.
#[must_use]
pub fn declared(descriptor: &SourceDescriptor) -> Option<SavedViewKinds> {
    let saved: Vec<&knobas_core::payload::KindPaths> = descriptor
        .payload_paths
        .iter()
        .filter(|p| p.frame == Some(Frame::SavedView))
        .filter(|p| descriptor.entity_kinds.iter().any(|k| k.id == p.kind))
        .collect();
    let first = saved.first()?;
    Some(SavedViewKinds {
        paths: saved.iter().map(|p| (*p).clone()).collect(),
        source_name: first
            .system_name
            .clone()
            .unwrap_or_else(|| descriptor.name.clone()),
    })
}

/// `held` is what keeps a purge from failing the run (#1088). The statement
/// reads `sync.live_item` from its snapshot, and the foreign key on
/// `sync.saved_view.entity_id` checks against the item as it is *now*: an item
/// deleted after the snapshot -- `config::delete`'s purge, which runs beside a
/// source's run by design (#127, #632) -- was read as live and then failed the
/// insert, and with it the whole run. `for key share` waits for a delete in
/// flight and drops a row already deleted, so a view whose item went is not
/// projected; and the lock it leaves, the one the foreign key takes anyway,
/// keeps the item until the run commits (the purge's `skip locked` passes it
/// by, as it does every row a run holds).
const UPSERT_VIEWS: &str = concat!(
    "
with held as (
  select entity_id from sync.item
   where source_id = $1 and kind = any($2)
     for key share
)
insert into sync.saved_view
       (entity_id, source_id, ref_kind, ref_id, board_type, name, source_query, sub_query, query,
        columns, sprint, last_read)
select i.entity_id, i.source_id,
       case i.payload->>'kind' when 'filter' then 'filter' else 'board' end,
       coalesce(i.payload->>'id', ''),
       ",
    knobas_core::declared_string!("$3", "board_type"),
    ", i.title, coalesce(",
    knobas_core::declared_string!("$3", "source_query"),
    ", ''),
       ",
    knobas_core::declared_string!("$3", "sub_query"),
    ", coalesce(i.payload->>'query', ''),
       case when jsonb_typeof(i.payload->'columns') = 'array'
            then i.payload->'columns' else '[]'::jsonb end,
       (select i.payload #> c.segs from ",
    knobas_core::declared_candidates!("$3", "sprint"),
    " c where jsonb_typeof(i.payload #> c.segs) = 'object' order by c.ord limit 1),
       now()
  from sync.live_item i
  join held h on h.entity_id = i.entity_id
 where i.source_id = $1 and i.kind = any($2)
    on conflict (entity_id) do update set
       ref_kind = excluded.ref_kind, ref_id = excluded.ref_id,
       board_type = excluded.board_type, name = excluded.name, source_query = excluded.source_query,
       sub_query = excluded.sub_query, query = excluded.query, columns = excluded.columns,
       sprint = excluded.sprint, last_read = excluded.last_read"
);

const DROP_GONE_VIEWS: &str = "
delete from sync.saved_view v
 where v.source_id = $1
   and not exists (select 1 from sync.live_item i
                    where i.entity_id = v.entity_id and i.kind = any($2))";

const DROP_MEMBERS: &str = "
delete from sync.saved_view_member m
 using sync.saved_view v
 where m.view_id = v.entity_id and v.source_id = $1";

const INSERT_MEMBERS: &str = "
insert into sync.saved_view_member (view_id, entity_id, rank)
select v.entity_id, $1 || ':' || k.key, k.n::integer
  from sync.saved_view v
  join sync.item i on i.entity_id = v.entity_id
 cross join lateral jsonb_array_elements_text(
         case when jsonb_typeof(i.payload->'keys') = 'array'
              then i.payload->'keys' else '[]'::jsonb end) with ordinality as k(key, n)
 where v.source_id = $1";

const UPSERT_TICKET_VIEWS: &str = "
with views as (
  select v.entity_id, v.name, v.columns,
         $2 || case v.ref_kind when 'filter' then ' filters' else ' boards' end as view_group,
         case v.ref_kind when 'filter' then 'list' else 'board' end as layout,
         trim(both '-' from regexp_replace(lower(v.entity_id), '[^a-z0-9]+', '-', 'g')) as id,
         row_number() over (order by v.entity_id) as n
    from sync.saved_view v
   where v.source_id = $1
),
earlier as (
  select exists (select 1 from knobas.ticket_view t
                  where left(t.saved_view, length($1) + 1) = $1 || ':') as any
)
insert into knobas.ticket_view (id, name, view_group, position, layout, columns, saved_view, is_new)
select w.id, w.name, w.view_group,
       (select coalesce(max(t.position), 0) from knobas.ticket_view t) + w.n::integer,
       w.layout, w.columns, w.entity_id, e.any
  from views w cross join earlier e
    on conflict (id) do update set
       name = excluded.name, view_group = excluded.view_group,
       columns = excluded.columns, updated_at = now()
 where knobas.ticket_view.saved_view = excluded.saved_view
   and (knobas.ticket_view.name, knobas.ticket_view.view_group, knobas.ticket_view.columns)
       is distinct from (excluded.name, excluded.view_group, excluded.columns)";

const DROP_GONE_TICKET_VIEWS: &str = "
delete from knobas.ticket_view t
 where left(t.saved_view, length($1) + 1) = $1 || ':'
   and not exists (select 1 from sync.saved_view v where v.entity_id = t.saved_view)";

/// Project one source's live saved views (see the module docs).
///
/// # Errors
///
/// The `sqlx::Error` of the statement that failed; the caller's run rolls
/// back with it.
pub async fn project(
    tx: &mut Transaction<'_, Postgres>,
    source_id: &str,
    declared: &SavedViewKinds,
) -> Result<(), sqlx::Error> {
    let kinds: Vec<&str> = declared.paths.iter().map(|p| p.kind.as_str()).collect();
    let paths = knobas_core::payload::Declarations::empty()
        .with(source_id, declared.paths.clone())
        .as_param();
    sqlx::query(UPSERT_VIEWS)
        .bind(source_id)
        .bind(&kinds)
        .bind(paths)
        .execute(&mut **tx)
        .await?;
    sqlx::query(DROP_GONE_VIEWS)
        .bind(source_id)
        .bind(&kinds)
        .execute(&mut **tx)
        .await?;
    for statement in [DROP_MEMBERS, INSERT_MEMBERS] {
        sqlx::query(statement)
            .bind(source_id)
            .execute(&mut **tx)
            .await?;
    }
    // Before the gone ones are dropped: a view that leaves in the run another
    // turns up in still says the source had views, so the newcomer is new.
    sqlx::query(UPSERT_TICKET_VIEWS)
        .bind(source_id)
        .bind(&declared.source_name)
        .execute(&mut **tx)
        .await?;
    sqlx::query(DROP_GONE_TICKET_VIEWS)
        .bind(source_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
