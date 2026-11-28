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
use knobas_sync::scheduler::{Scheduler, Unread, UnreadWhy, WriteUnread};

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
/// At most `$2` (one past [`SAVED_VIEWS_AT_MOST`], so a cut is known), the
/// views holding `$1` first, as
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
    /// Not asked: a saved view past [`SAVED_VIEWS_AT_MOST`], the first of
    /// those left out. The run that follows the write brings them.
    LeftOut,
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
    /// What a refresh answered, as an effect's (or the item's) read.
    pub(crate) fn of(outcome: Result<RefreshOutcome, RefreshError>) -> Self {
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
                .bind(SAVED_VIEWS_AT_MOST + 1)
                .fetch_all(pool)
                .await
        }
    }
}

/// The items of `effect` that are read, and the first left out past the cap
/// on saved views ([`SAVED_VIEWS_AT_MOST`]), which is said instead. Only
/// saved views are capped.
fn capped(effect: Effect, mut items: Vec<String>) -> (Vec<String>, Option<String>) {
    let cap = usize::try_from(SAVED_VIEWS_AT_MOST).unwrap_or(usize::MAX);
    if effect != Effect::SavedViews || items.len() <= cap {
        return (items, None);
    }
    let left_out = items.split_off(cap).into_iter().next();
    (items, left_out)
}

/// `effects`, the ones `write`'s op declares, each resolved against the
/// write's target, once each, without `read_back` (the item already read
/// back) and in the order the op declares them. Past the cap on saved views
/// the first one left out comes last of its effect, marked `true`: it is
/// said ([`EffectAnswer::LeftOut`]), not read.
async fn named(
    scheduler: &Scheduler,
    write: &QueuedWrite,
    read_back: &str,
    effects: &[Effect],
) -> Vec<(Effect, String, bool)> {
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
            Ok(entities) => {
                let fresh: Vec<String> = entities
                    .into_iter()
                    .filter(|entity| seen.insert(entity.clone()))
                    .collect();
                let (read, left_out) = capped(effect, fresh);
                named.extend(read.into_iter().map(|entity| (effect, entity, false)));
                named.extend(left_out.map(|entity| (effect, entity, true)));
            }
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
    futures_util::future::join_all(named.into_iter().map(|(effect, entity, left_out)| async move {
        if left_out {
            return EffectRead {
                effect,
                entity,
                answer: EffectAnswer::LeftOut,
            };
        }
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

/// What [`unsaid`] knows of the write beside its reads.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Said<'a> {
    /// The write's source.
    pub source: &'a str,
    /// Whether it syncs only by hand (ADR-0056): no run follows the write.
    pub by_hand: bool,
    /// Whether the write is a source's own action (`WriteOp::SourceAction`),
    /// whose other changes knobas cannot know.
    pub own_action: bool,
}

/// One thing a landed write moved that was not read back, before it is
/// named for the shell ([`tell`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unsaid {
    /// The item, or `None` for what else the write moved, unnamed.
    pub entity: Option<String>,
    /// The source whose next run brings it.
    pub source: String,
    pub why: UnreadWhy,
}

/// The source an entity id names: what precedes its first `:`.
fn source_of(entity: &str) -> &str {
    entity.split_once(':').map_or(entity, |(source, _)| source)
}

/// **What a landed write did not read back** (ADR-0064 rule 3): the item
/// `read_back`, where its read answered `item`, then each of `effects` not
/// read, in the order found, then what else the write moved where nothing
/// will read it soon: a source that syncs by hand, where no run follows the
/// write, or a source's own action, which nobody declared. Empty when every
/// read answered; a write read back whole tells nothing.
pub(crate) fn unsaid(
    read_back: &str,
    item: &EffectAnswer,
    effects: &[EffectRead],
    said: Said<'_>,
) -> Vec<Unsaid> {
    let why = |answer: &EffectAnswer| match answer {
        EffectAnswer::Read | EffectAnswer::Unchanged => None,
        EffectAnswer::NotOffered => Some(UnreadWhy::NotOffered),
        EffectAnswer::RunInFlight => Some(UnreadWhy::RunInFlight),
        EffectAnswer::TurnedOff => Some(UnreadWhy::TurnedOff),
        EffectAnswer::Failed { .. } => Some(UnreadWhy::Failed),
        EffectAnswer::LeftOut => Some(UnreadWhy::LeftOut),
    };
    let mut unsaid: Vec<Unsaid> = why(item)
        .map(|why| Unsaid {
            entity: Some(read_back.to_owned()),
            source: source_of(read_back).to_owned(),
            why,
        })
        .into_iter()
        .collect();
    unsaid.extend(effects.iter().filter_map(|read| {
        why(&read.answer).map(|why| Unsaid {
            // The views past the cap are many and unnamed: the first one
            // left out stands for them only by its source.
            entity: (why != UnreadWhy::LeftOut).then(|| read.entity.clone()),
            source: source_of(&read.entity).to_owned(),
            why,
        })
    }));
    let rest = if said.by_hand {
        Some(UnreadWhy::ByHand)
    } else {
        said.own_action.then_some(UnreadWhy::Unknown)
    };
    unsaid.extend(rest.map(|why| Unsaid {
        entity: None,
        source: said.source.to_owned(),
        why,
    }));
    unsaid
}

/// The `write:unread` event for a landed write whose reads left `unsaid`
/// (ADR-0064 rule 3, #2596), each named as the shell says it: the item's
/// title, the source's name, whether that source syncs by hand, and when it
/// runs next where that is what brings it. `None` when nothing is unsaid.
pub(crate) async fn tell(
    pool: &sqlx::PgPool,
    write: &QueuedWrite,
    read_back: &str,
    unsaid: Vec<Unsaid>,
) -> Option<WriteUnread> {
    if unsaid.is_empty() {
        return None;
    }
    let mut ids: Vec<String> = vec![read_back.to_owned()];
    ids.extend(unsaid.iter().filter_map(|line| line.entity.clone()));
    let titles: std::collections::HashMap<String, String> =
        sqlx::query_as("select id, title from knobas.entity where id = any($1) and title <> ''")
            .bind(&ids)
            .fetch_all(pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect();
    let title = |id: &str| titles.get(id).cloned().unwrap_or_else(|| id.to_owned());
    let mut sources: Vec<String> = unsaid.iter().map(|line| line.source.clone()).collect();
    sources.sort();
    sources.dedup();
    let named: std::collections::HashMap<String, (String, bool)> = sqlx::query_as(
        "select id, display_name, sync_by_hand from knobas.source_config where id = any($1)",
    )
    .bind(&sources)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(id, name, by_hand): (String, String, bool)| (id, (name, by_hand)))
    .collect();
    let mut next_runs = std::collections::HashMap::new();
    for source in &sources {
        // The write's own source runs right after the write (ADR-0031), or
        // not at all where it syncs by hand: no countdown is the truth.
        if *source == write.source_id {
            continue;
        }
        if let Ok(Some(status)) = knobas_sync::scheduler::status_for(pool, source).await {
            next_runs.insert(source.clone(), status.next_run_at);
        }
    }
    let unread = unsaid
        .into_iter()
        .map(|line| {
            let (source_name, by_hand) = named
                .get(&line.source)
                .cloned()
                .unwrap_or_else(|| (line.source.clone(), false));
            Unread {
                title: title(line.entity.as_deref().unwrap_or(read_back)),
                entity_id: line.entity,
                source_name: if source_name.is_empty() {
                    line.source.clone()
                } else {
                    source_name
                },
                by_hand,
                next_run_at: next_runs
                    .get(&line.source)
                    .copied()
                    .flatten()
                    .filter(|_| !by_hand),
                source_id: line.source,
                why: line.why,
            }
        })
        .collect();
    Some(WriteUnread {
        write_id: write.id,
        entity_id: read_back.to_owned(),
        title: title(read_back),
        unread,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(effect: Effect, entity: &str, answer: EffectAnswer) -> EffectRead {
        EffectRead {
            effect,
            entity: entity.to_owned(),
            answer,
        }
    }

    fn line(entity: Option<&str>, source: &str, why: UnreadWhy) -> Unsaid {
        Unsaid {
            entity: entity.map(str::to_owned),
            source: source.to_owned(),
            why,
        }
    }

    /// **What a landed write did not read back is told, and what it read is
    /// not** (ADR-0064 rule 3): the item a 1.3.0 component does not refresh,
    /// an effect its adapter does not read, one a run held past its patience,
    /// one whose source is off or whose read failed, the views past the cap,
    /// in the order found; an effect read or unchanged says nothing.
    #[test]
    fn every_effect_not_read_is_told_in_the_order_found_and_none_that_was() {
        let effects = [
            read(Effect::Parent, "forge:tide/pay", EffectAnswer::Read),
            read(Effect::LinkedTickets, "jira:PAY-1", EffectAnswer::Unchanged),
            read(
                Effect::LinkedTickets,
                "jira:PAY-2",
                EffectAnswer::RunInFlight,
            ),
            read(Effect::LinkedTickets, "ext:T-1", EffectAnswer::NotOffered),
            read(Effect::LinkedTickets, "off:T-2", EffectAnswer::TurnedOff),
            read(
                Effect::LinkedTickets,
                "jira:PAY-3",
                EffectAnswer::Failed {
                    reason: "503".to_owned(),
                },
            ),
            read(Effect::SavedViews, "jira:board-9", EffectAnswer::LeftOut),
        ];
        assert_eq!(
            unsaid(
                "forge:tide/pay#7",
                &EffectAnswer::NotOffered,
                &effects,
                Said::default()
            ),
            [
                line(Some("forge:tide/pay#7"), "forge", UnreadWhy::NotOffered),
                line(Some("jira:PAY-2"), "jira", UnreadWhy::RunInFlight),
                line(Some("ext:T-1"), "ext", UnreadWhy::NotOffered),
                line(Some("off:T-2"), "off", UnreadWhy::TurnedOff),
                line(Some("jira:PAY-3"), "jira", UnreadWhy::Failed),
                line(None, "jira", UnreadWhy::LeftOut),
            ]
        );
    }

    /// **Past eight saved views, the first left out is said, not read**; at
    /// eight nothing is left out, and no other effect is capped.
    #[test]
    fn the_views_past_the_cap_are_said_by_the_first_left_out() {
        let views = |n: usize| {
            (0..n)
                .map(|i| format!("jira:board-{i}"))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            capped(Effect::SavedViews, views(9)),
            (views(8), Some("jira:board-8".to_owned()))
        );
        assert_eq!(capped(Effect::SavedViews, views(8)), (views(8), None));
        assert_eq!(capped(Effect::LinkedTickets, views(9)), (views(9), None));
    }

    /// **A write whose item and effects were all read tells nothing.**
    #[test]
    fn a_write_read_back_whole_tells_nothing() {
        let effects = [read(Effect::Parent, "forge:tide/pay", EffectAnswer::Read)];
        assert!(
            unsaid(
                "forge:tide/pay#7",
                &EffectAnswer::Read,
                &effects,
                Said::default()
            )
            .is_empty()
        );
        assert!(
            unsaid(
                "forge:tide/pay#7",
                &EffectAnswer::Unchanged,
                &[],
                Said::default()
            )
            .is_empty()
        );
    }

    /// **A source that syncs by hand says what else the write moved waits for
    /// *Sync now***, once, and a source's own action on it says nothing more
    /// (the press brings both). A source's own action on a scheduled source
    /// says its other changes are unknown.
    #[test]
    fn a_by_hand_source_and_a_sources_own_action_tell_what_else_moved() {
        let by_hand = Said {
            source: "kuma",
            by_hand: true,
            own_action: true,
        };
        assert_eq!(
            unsaid("kuma:7", &EffectAnswer::Read, &[], by_hand),
            [line(None, "kuma", UnreadWhy::ByHand)]
        );
        let scheduled = Said {
            source: "surveys",
            by_hand: false,
            own_action: true,
        };
        assert_eq!(
            unsaid(
                "surveys:survey-1",
                &EffectAnswer::NotOffered,
                &[],
                scheduled
            ),
            [
                line(Some("surveys:survey-1"), "surveys", UnreadWhy::NotOffered),
                line(None, "surveys", UnreadWhy::Unknown),
            ]
        );
    }

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
        // Eight are read; the ninth is answered so the cut is known.
        let mut expected = vec![id("board-99")];
        expected.extend((0..8).map(|n| id(&format!("board-{n:02}"))));
        assert_eq!(read, expected);
    }
}
