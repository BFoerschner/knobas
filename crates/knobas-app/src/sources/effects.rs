//! A landed write's **effects** (`CONTEXT.md` *Effect*, ADR-0064): what the
//! write moves beyond the item it is read back by, as its op declares them
//! (`WriteOp::effects`), resolved from the mirror and read at once, each at
//! its own source and in its own place.
//!
//! Called by [`super::write_queue::after_landing`] after the item's own
//! read-back and before the run of the write's source that follows
//! (ADR-0031). An effect is read the way the item is: one
//! [`super::refresh::refresh`], which reads by id and, where the adapter does
//! not, walks the item as a parent (ADR-0047). So a pull request's repository
//! is walked and a linked ticket read by id. **No read here runs a source.**
//!
//! Each read waits behind a run of its own source in flight (ADR-0031's
//! serialization), and stops a run that may stand aside once it waits, as the
//! item's read-back does (ADR-0031 as amended 2026-11-24). It waits at most
//! [`EFFECT_PATIENCE`] ([`Lock::WaitAtMost`]): a run that may not stand
//! aside (one a person pressed, a first sync, a backfill) can last minutes,
//! and the run of the write's own source and every caller that waits out the
//! write's follow-up wait on this read. One that runs out answers
//! [`EffectAnswer::RunInFlight`], said rather than waited on (ADR-0064 rule
//! 3). Each one that read tells the shell on `item:refreshed` with the
//! write's id, so a page holding the write's echo reads again.

use knobas_core::delivery::Effect;
use knobas_core::write_queue::QueuedWrite;
use knobas_sync::refresh::{Lock, RefreshError, RefreshOutcome};
use knobas_sync::scheduler::Scheduler;

/// How long an effect's read waits behind a run of its source before it
/// gives up: past a run that stands aside, and past the other effects read
/// in the same source, which are one item or one parent's walk each.
const EFFECT_PATIENCE: std::time::Duration = std::time::Duration::from_secs(10);

/// The kind a ticket is mirrored as, whichever source holds it: what a
/// [`Effect::LinkedTickets`] reads.
const TICKET_KIND: &str = "ticket";

/// The **declared parent** of `$1` (`sync.item_parent`, migrations `0028` and
/// `0029`): each live item that answers, as a parent of the declared kind in
/// the same source, to a name `$1` names, matched under the adapter its
/// source is configured with. A parent may be named by more than one path (a
/// Gitea pull request names its repository by `full_name` and by `name`), so
/// only the parents matching the most of those names are kept: a repository
/// of the same short name under another owner matches one and is not read.
const DECLARED_PARENT: &str = "with matched as (
         select p.entity_id as parent, count(distinct lower(c.name)) as names
           from sync.item_parent c
           join knobas.source_config s on s.id = c.source_id and s.kind = c.adapter_kind
           join sync.item_parent p
             on p.arm = 'parent' and p.kind = c.kind and p.source_id = c.source_id
            and p.adapter_kind = c.adapter_kind and lower(p.name) = lower(c.name)
            and p.entity_id <> c.entity_id
           join sync.live_item l on l.entity_id = p.entity_id
          where c.arm = 'child' and c.entity_id = $1
          group by p.entity_id
       )
       select parent from matched
        where names = (select max(names) from matched)
        order by parent";

/// The live tickets `$1` is linked to, by a confirmed link at either end.
/// `$2` the ticket kind.
const LINKED_TICKETS: &str = "select distinct i.entity_id
       from knobas.confirmed_link l
       join sync.live_item i
         on i.entity_id = case when l.from_id = $1 then l.to_id else l.from_id end
      where (l.from_id = $1 or l.to_id = $1) and i.kind = $2
      order by i.entity_id";

/// The live **saved views** that hold `$1` (`sync.saved_view_member`), and
/// those that hold any item of `$1`'s project in its source
/// (`sync.item_project`, the key its configured adapter declares): the boards
/// and filters a ticket write may have moved it out of or into. A view
/// holding no ticket of the project yet is not found; the next run of its
/// source brings it.
///
/// At most `$2` ([`SAVED_VIEWS_AT_MOST`]), the views holding `$1` first, as
/// those are the ones a write takes it out of; then by view id. The project's
/// items are one range of `item_project_census_idx`, hashed once, so the
/// members are scanned once whatever the project's size.
const SAVED_VIEWS: &str = "with project as (
         select p.source_id, p.adapter_kind, p.key
           from sync.item_project p
           join knobas.source_config s on s.id = p.source_id and s.kind = p.adapter_kind
          where p.entity_id = $1
       ),
       held as (
         select m.view_id, bool_or(m.entity_id = $1) as holds
           from sync.saved_view_member m
          where m.entity_id = $1
             or m.entity_id in (select q.entity_id
                                  from sync.item_project q
                                  join project p on p.source_id = q.source_id
                                   and p.key = q.key and p.adapter_kind = q.adapter_kind)
          group by m.view_id
       )
       select h.view_id
         from held h
         join sync.live_item v on v.entity_id = h.view_id
        order by h.holds desc, h.view_id
        limit $2";

/// How many saved views one ticket write reads at once ([`SAVED_VIEWS`]).
/// Each read opens a connection of its own and waits on its source's lock
/// beside the others, and a project on a large Jira sits on dozens of boards
/// and filters; the ones past it are brought by the run that follows the
/// write. Eight, as the bounded schedule's scopes (`look_again::AT_ONCE`).
const SAVED_VIEWS_AT_MOST: i64 = 8;

/// What one effect's read answered: what the *say so* rule (ADR-0064 rule 3,
/// #2596) names for an effect knobas could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectAnswer {
    /// Read and written: the mirror holds it as its source does, or it is
    /// tombstoned because the source no longer has it.
    Read,
    /// Read, and the mirror already held it as its source does.
    Unchanged,
    /// Its adapter reads neither it by id nor it as a parent: it shows after
    /// the next run of its source.
    NotOffered,
    /// A run of its source held it past [`EFFECT_PATIENCE`], so nothing was
    /// read: it shows when that run, or the next, ends.
    RunInFlight,
    /// Its source is turned off, so nothing was read.
    TurnedOff,
    /// The read failed, or could not be asked, and nothing changed.
    Failed {
        /// The reason, in the engine's words.
        reason: String,
    },
}

/// One effect of a landed write and what its read answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectRead {
    /// Which declared effect named it.
    pub effect: Effect,
    /// The item read.
    pub entity: String,
    /// What the read answered.
    pub answer: EffectAnswer,
}

impl EffectAnswer {
    fn of(outcome: Result<RefreshOutcome, RefreshError>) -> Self {
        match outcome {
            Ok(RefreshOutcome::Refreshed | RefreshOutcome::Gone) => Self::Read,
            Ok(RefreshOutcome::Unchanged) => Self::Unchanged,
            Ok(RefreshOutcome::NotOffered) => Self::NotOffered,
            Ok(RefreshOutcome::RunInFlight) => Self::RunInFlight,
            Err(RefreshError::TurnedOff(_)) => Self::TurnedOff,
            Err(error) => Self::Failed {
                reason: error.to_string(),
            },
        }
    }
}

/// The live declared parents of `entity` ([`DECLARED_PARENT`]): what
/// [`Effect::Parent`] names, and what [`super::look_again`] finds a moved
/// branch's repository by.
pub(crate) async fn parents(pool: &sqlx::PgPool, entity: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(DECLARED_PARENT)
        .bind(entity)
        .fetch_all(pool)
        .await
}

/// The live tickets `entity` is linked to by a confirmed link, at either end
/// ([`LINKED_TICKETS`]): what [`Effect::LinkedTickets`] names, and what
/// [`super::look_again`] looks at again after a write on `entity` (#2595).
pub(crate) async fn linked_tickets(
    pool: &sqlx::PgPool,
    entity: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(LINKED_TICKETS)
        .bind(entity)
        .bind(TICKET_KIND)
        .fetch_all(pool)
        .await
}

/// The items `effect` names for the write against `target`, from the mirror.
/// The saved views are the ones of `read_back`, the item the write made or
/// touched: a ticket filed names its project as its target, which the mirror
/// does not hold, and the new ticket as what is read back.
async fn resolve(
    pool: &sqlx::PgPool,
    effect: Effect,
    target: &str,
    read_back: &str,
) -> Result<Vec<String>, sqlx::Error> {
    match effect {
        Effect::Parent => parents(pool, target).await,
        Effect::LinkedTickets => linked_tickets(pool, target).await,
        Effect::SavedViews => {
            sqlx::query_scalar(SAVED_VIEWS)
                .bind(read_back)
                .bind(SAVED_VIEWS_AT_MOST)
                .fetch_all(pool)
                .await
        }
    }
}

/// `effects`, the ones `write`'s op declares, each resolved against the
/// write's target, once each, without `read_back` (the item already read
/// back) and in the order the op declares them.
async fn named(
    scheduler: &Scheduler,
    write: &QueuedWrite,
    read_back: &str,
    effects: &[Effect],
) -> Vec<(Effect, String)> {
    if effects.is_empty() {
        return Vec::new();
    }
    let deps = scheduler.deps();
    // The declarations `sync.item_parent` and `sync.item_project` are kept
    // from are the registry's: the same call every search makes before it
    // reads (#627). One read when they are stored already.
    if effects
        .iter()
        .any(|effect| matches!(effect, Effect::Parent | Effect::SavedViews))
        && let Err(error) =
            knobas_search::Searcher::declaring(deps.pool.clone(), deps.registry.descriptors()).await
    {
        tracing::warn!(write_id = write.id, %error, "the declared parents could not be stated");
    }
    let mut seen = std::collections::HashSet::from([read_back.to_owned()]);
    let mut named = Vec::new();
    for &effect in effects {
        match resolve(&deps.pool, effect, &write.entity_id, read_back).await {
            Ok(entities) => named.extend(
                entities
                    .into_iter()
                    .filter(|entity| seen.insert(entity.clone()))
                    .map(|entity| (effect, entity)),
            ),
            Err(error) => tracing::warn!(
                write_id = write.id,
                ?effect,
                %error,
                "a landed write's effect could not be resolved from the mirror"
            ),
        }
    }
    named
}

/// Read `effects`, each one `write`'s op declares (handed in by
/// [`super::write_queue::after_landing`], which decodes the op: the one
/// place outside the queue that may, `write_choke_point.rs`), all at once, each at its own source
/// under that source's lock, waited for at most [`EFFECT_PATIENCE`];
/// `read_back` is the item already read back, which
/// is not read again. Answers what each read answered, in the order the op
/// declares them.
pub(crate) async fn read(
    scheduler: &Scheduler,
    write: &QueuedWrite,
    read_back: &str,
    effects: &[Effect],
) -> Vec<EffectRead> {
    let named = named(scheduler, write, read_back, effects).await;
    futures_util::future::join_all(named.into_iter().map(|(effect, entity)| async move {
        let source = entity
            .split_once(':')
            .map_or_else(String::new, |(source, _)| source.to_owned());
        let outcome = {
            let refresh = super::refresh::refresh(
                scheduler,
                &entity,
                Some(write.id),
                Lock::WaitAtMost(EFFECT_PATIENCE),
            );
            tokio::pin!(refresh);
            // As the item's read-back: once this read waits on a run that may
            // stand aside, that run is stopped (ADR-0031 as amended 2026-11-24).
            tokio::select! {
                biased;
                outcome = &mut refresh => outcome,
                () = super::write_queue::stand_aside_once_waiting(scheduler, &source, write.id) => {
                    refresh.await
                }
            }
        };
        let answer = EffectAnswer::of(outcome);
        if !matches!(answer, EffectAnswer::Read | EffectAnswer::Unchanged) {
            tracing::info!(
                write_id = write.id,
                ?effect,
                entity,
                ?answer,
                "a landed write's effect was not read back"
            );
        }
        EffectRead {
            effect,
            entity,
            answer,
        }
    }))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A ticket write reads at most [`SAVED_VIEWS_AT_MOST`] views, the
    /// ones holding the ticket first**: on a project that sits on more boards
    /// than that, the board holding the ticket itself is read even though its
    /// id sorts last, a board over another project is not, and neither is a
    /// view no longer live.
    #[tokio::test]
    async fn a_ticket_write_reads_a_bounded_set_of_views_its_own_first() {
        let pool = knobas_db::test_util::test_pool().await;
        knobas_db::migrate::run(&pool).await.unwrap();
        let source = "effects_views";
        let id = |key: &str| format!("{source}:{key}");
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
             values ($1, 'effects_views_kind', $1, 'http://localhost', 'pat')
             on conflict (id) do nothing",
        )
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
        let tickets = [("PAY-1", "PAY"), ("PAY-2", "PAY"), ("OPS-1", "OPS")];
        // Ten boards over PAY that hold PAY-2, one that holds PAY-1 and sorts
        // last, one over OPS, and one holding PAY-1 that is gone.
        let mut views: Vec<(String, &str)> = (0..10)
            .map(|n| (format!("board-{n:02}"), "PAY-2"))
            .collect();
        views.extend([
            ("board-99".to_owned(), "PAY-1"),
            ("board-50".to_owned(), "OPS-1"),
            ("board-77".to_owned(), "PAY-1"),
        ]);
        for (key, kind) in tickets
            .iter()
            .map(|(key, _)| (*key, "ticket"))
            .chain(views.iter().map(|(key, _)| (key.as_str(), "view")))
        {
            sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $1)")
                .bind(id(key))
                .bind(kind)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "insert into sync.item (entity_id, source_id, kind, payload)
                 values ($1, $2, $3, '{}')",
            )
            .bind(id(key))
            .bind(source)
            .bind(kind)
            .execute(&pool)
            .await
            .unwrap();
        }
        for (key, project) in tickets {
            sqlx::query(
                "insert into sync.item_project (entity_id, adapter_kind, source_id, key)
                 values ($1, 'effects_views_kind', $2, $3)",
            )
            .bind(id(key))
            .bind(source)
            .bind(project)
            .execute(&pool)
            .await
            .unwrap();
        }
        for (key, holds) in &views {
            sqlx::query(
                "insert into sync.saved_view (entity_id, source_id, ref_kind, ref_id, name)
                 values ($1, $2, 'board', $1, $1)",
            )
            .bind(id(key))
            .bind(source)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "insert into sync.saved_view_member (view_id, entity_id, rank) values ($1, $2, 1)",
            )
            .bind(id(key))
            .bind(id(holds))
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
            .bind(id("board-77"))
            .execute(&pool)
            .await
            .unwrap();

        let ticket = id("PAY-1");
        let read = resolve(&pool, Effect::SavedViews, &ticket, &ticket)
            .await
            .unwrap();
        let mut expected = vec![id("board-99")];
        expected.extend((0..7).map(|n| id(&format!("board-{n:02}"))));
        assert_eq!(read, expected);
    }
}
