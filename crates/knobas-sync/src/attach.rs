//! The names an asset carries, resolved against the monitors the mirror holds
//! (issue #453, spec #427).
//!
//! Spec #427, *Import*: a monitor name *"the mirror does not hold yet is kept
//! on the asset and resolved by the next import **or the M4.1 sync**"*. The
//! import half landed with #439 (`knobas_app::assets`' `monitor_plan`); this
//! is the sync half, and it is what makes the rest of the sentence true.
//!
//! # Why the sync needs its own half at all
//!
//! Because the two ways a name arrives on an asset both point here.
//!
//! * **The estate import** names monitors that do not exist yet -- an estate
//!   file written before the Kuma source was configured names seven of them,
//!   and re-importing that file to pick them up is a gesture nobody would
//!   think to make.
//! * **`create_monitor`** (issue #453) records the name on the asset *before*
//!   it queues the write, precisely because that is what attaches the monitor
//!   when it arrives. The write goes to Kuma, Kuma starts checking, the next
//!   poll mirrors the monitor, and the link has to be drawn by whatever is
//!   watching that poll -- which is this.
//!
//! One rule serves both, and it is the import's own: **a name becomes a
//! `monitored-by` link the moment the mirror holds a monitor called that.**
//!
//! # One rule, two encodings
//!
//! The import's half is `knobas_app::assets`' `monitor_plan` -- `LIVE_MONITORS`
//! and `MONITOR_LINKS` -- and it is a *preview* over a caller's transaction in
//! a crate this one cannot see, so the statement below is not shared with it.
//! What must stay shared is the **predicate**: a name matches a live monitor's
//! `title`, and a pair that already has a link is left alone. A change to
//! either side is a change to both, and neither compiler will say so; this
//! paragraph and its twin in `monitor_plan`'s doc are what a reader has.
//!
//! **A name matches whatever carries it, in both directions**, which is that
//! rule taken literally and is the import's behaviour too: two monitors called
//! `gitea` attach an asset naming `gitea` twice, and two assets naming `gitea`
//! are both attached to the one monitor. Neither is a mistake -- an asset
//! really is watched by two checks of that name, and one check really does
//! watch both assets -- and the alternative would be knobas choosing which of
//! two live monitors counts.
//!
//! # What it will not do
//!
//! * **It never removes a link.** A name is added to an asset and never taken
//!   away by a resolution -- `knobas.link`'s own `unlink` is what withdraws an
//!   attachment, and a sync that undrew one would undo a reader's decision on
//!   the strength of a monitor being renamed in Kuma.
//! * **It never touches a pair that already has an active link row**, whether
//!   that row is confirmed or a *proposal*. Both matter and for different
//!   reasons: a confirmed one is already drawn, and a proposal is `0011`'s
//!   unordered unique index in force -- `monitor_url_host` (#478) proposes
//!   exactly this pair from the other direction, and inserting over its
//!   proposal would fail the whole run rather than draw a link. A pair with a
//!   proposal outstanding is left for the reader to accept, and the name stays
//!   on the asset's *waiting* list until they do.
//! * **It writes no activity line.** The import writes one when the *name* is
//!   added (`edited`, `field: monitors`) and so does `create_monitor`'s
//!   caller; the resolution is that name's mechanical consequence, and a line
//!   per resolved name would put seven of them in front of a reader who
//!   imported one file. The link itself is the record, and it is drawn in
//!   both ends' *Linked* panels.

use knobas_core::entity::EntityRef;
use knobas_core::link;
use sqlx::{Postgres, Row, Transaction};

/// Every (asset, monitor) pair this source could attach and has not.
///
/// `$1` is the source, `$2` the monitor kind, `$3` the relation.
///
/// **`sync.live_item` and not `sync.item`**: a tombstoned monitor is one Kuma
/// has stopped publishing, and attaching an asset to it would be knobas
/// drawing an attachment to something it has just been told is gone. The name
/// stays on the asset, and a monitor that comes back -- a resume -- is
/// attached by the poll that finds it.
///
/// **The `not exists` is `0011`'s index read as a query**: `least`/`greatest`
/// over the pair, `relation`, and `deleted_at is null`. Written that way
/// rather than as two `or`ed comparisons so that a reader can see it is the
/// same statement the unique index makes -- if the two ever disagree, this
/// insert is the one that fails.
///
/// **And that is why it reads `knobas.link` rather than a view.** `0007` split
/// the table into `confirmed_link` and `proposed_link` so that no reader has
/// to remember which population it means, and `knobas-core`'s `link_reads`
/// battery holds every module to it. This is the third exemption on that
/// battery's list, and the reason it is one: the question here is not *which*
/// population the row is in but **whether the index already holds the pair** --
/// `link_pair_active_idx` does not look at `confirmed_at`, so a guard that did
/// would insert over `monitor_url_host`'s proposal (#478) and fail this poll,
/// and the next, and every one after. `tests/it/attach.rs`'
/// `a_proposal_over_the_same_pair_is_left_alone` is that failure as a test.
const UNATTACHED: &str = "select ast.id as asset_id,
                                 m.entity_id as monitor_id,
                                 m.title as monitor_name
   from sync.live_item m
   join knobas.asset ast on m.title = any(ast.monitors)
  where m.source_id = $1
    and m.kind = $2
    and not exists (
          select 1 from knobas.link l
           where l.deleted_at is null
             and l.relation = $3
             and least(l.from_id, l.to_id) = least(ast.id, m.entity_id)
             and greatest(l.from_id, l.to_id) = greatest(ast.id, m.entity_id)
        )
  order by ast.id, m.title, m.entity_id";

/// Draw the `monitored-by` links this source's monitors have earned, inside
/// the run's own transaction.
///
/// Called from `run_locked` immediately after [`crate::alerts::reconcile`],
/// and the order is not arbitrary: the sweep above them has already tombstoned
/// whatever left the roster, so `sync.live_item` here is this run's answer
/// rather than the last one's. Answers how many links it drew, which the tests
/// read and nothing else does.
///
/// **Directed asset → monitor**, which is the direction the import draws and
/// the direction the relation is worded in: an asset is *monitored by* a
/// monitor. The pair is unique undirected either way (`0011`), so this is
/// about what the *Linked* panel says on each end, not about what it finds.
///
/// # Errors
///
/// [`sqlx::Error`] if the read or any insert fails; the caller rolls the run
/// back, which is what keeps a half-drawn set of links out of the estate.
pub async fn resolve(
    tx: &mut Transaction<'_, Postgres>,
    source_id: &str,
) -> Result<u64, sqlx::Error> {
    let pairs = sqlx::query(UNATTACHED)
        .bind(source_id)
        .bind(crate::samples::KIND)
        .bind(link::MONITORED_BY)
        .fetch_all(&mut **tx)
        .await?;

    let actor = format!("sync:{source_id}");
    let mut drawn = 0;
    for row in &pairs {
        let asset_id: String = row.try_get("asset_id")?;
        let monitor_id: String = row.try_get("monitor_id")?;
        // Neither id can fail to parse from the join above -- `knobas.asset`
        // constrains one namespace and the mirror the other -- and guessing an
        // entity would be worse than saying nothing, which is
        // `alerts::recovered_lines`' rule applied to a write rather than to a
        // line.
        let (Ok(asset), Ok(monitor)) = (EntityRef::parse(&asset_id), EntityRef::parse(&monitor_id))
        else {
            continue;
        };
        link::create_with(
            &mut **tx,
            &asset,
            &monitor,
            link::MONITORED_BY,
            // The origin the import writes for exactly this link, so a name
            // resolved on the poll after it was typed and a name resolved by
            // the next import produce the same row. What `origin` records is
            // the population a link belongs to -- *knobas drew this from a
            // name on an asset* -- and not which surface put the name there.
            link::Origin::Imported,
            None,
            &actor,
        )
        .await
        .map_err(|error| match error {
            knobas_core::CoreError::Db(error) => error,
            other => sqlx::Error::Protocol(other.to_string()),
        })?;
        drawn += 1;
    }
    Ok(drawn)
}
