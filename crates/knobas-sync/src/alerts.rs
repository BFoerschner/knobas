//! The alert a monitor opens, and the recovery that closes it (issue #444).
//!
//! Spec #427: *"the engine … reconciles alerts from the newest two samples per
//! monitor: a crossing into down or warn opens an alert if none is open, a
//! return to up closes the open one."* `CONTEXT.md`, **Alert**: *"a monitor's
//! transition to down or warn, open until the monitor recovers; at most one
//! open per monitor."*
//!
//! # What this module owns
//!
//! One rule, in one function: [`decide`], which takes what the newest sample
//! says and whether this monitor already has an alert open, and answers with
//! the one thing to do about it. [`reconcile`] is that rule applied to a
//! run's whole roster, and it owns no clock — the row's `opened_at` and the
//! `closed_at` an update writes are both `now()`, which inside the run's
//! transaction is the *transaction* timestamp and therefore the same instant
//! [`crate::samples::append`] stamped the samples this decision was made from.
//!
//! # Why the open row is read and the second sample is not
//!
//! The spec's sentence names *the newest two samples*, and this module reads
//! **one sample and the open alert**. They answer the same question — *was
//! this monitor already in trouble when the poll landed?* — and the row
//! answers it better in two places where they come apart:
//!
//! * **A monitor that is already down the first time knobas looks.** The two
//!   newest samples are `down, down`, which is not a crossing, so a rule
//!   reading only samples opens nothing and the estate is quietly silent about
//!   a monitor that is quietly broken. It stays silent until the monitor
//!   recovers and falls again. The open row says *nothing is open*, and one
//!   alert opens. This is not a hypothetical: it is what every existing
//!   install does on the first run after this migration, and what a fresh
//!   profile does against an estate that is already having a bad morning.
//! * **A poll that produced no sample.** A monitor paused in Kuma leaves
//!   `sync.live_item`, so it is sampled no more (#443) — and its alert stays
//!   open, because an alert closes on a *return to up* and silence is not
//!   recovery. A rule holding "the previous state" in a sample would have to
//!   decide what a gap means; the row simply keeps standing.
//!
//! The two guards a crossing rule would spend — *the state changed* and *no
//! alert is open* — are also each sufficient on their own for the ordinary
//! sequences, which is the shape #443's mutation round found masking itself
//! (a source's descriptor gate and its roster filter). One guard, load-bearing
//! in every direction, is what is here.
//!
//! # What the five sample words mean here
//!
//! [`crate::samples::STATES`] holds five: `up`, `down` and `warn` are
//! `CONTEXT.md`'s **Monitor** vocabulary, and `pending` and `maintenance` are
//! states Uptime Kuma has that knobas' rollup has no word for. An alert is a
//! statement about trouble, so:
//!
//! * `down` and `warn` **open** one ([`OPENS`], which is migration `0022`'s
//!   CHECK);
//! * `up` **closes** the open one;
//! * `pending`, `maintenance` and a miss (a null state) do **neither**.
//!
//! The last line is the conservative reading, chosen because the ticket and
//! the spec are silent on those two words and because the other reading is the
//! one that can lie: a monitor put into maintenance while it is down would
//! have its alert closed, and knobas would be reporting a recovery that nobody
//! made. A monitor coming *out* of maintenance broken is sampled `down` on the
//! next poll and opens one then.

use knobas_core::activity;
use knobas_core::entity::EntityRef;
use sqlx::{Postgres, Row, Transaction};

/// The two state words an alert may carry: the trouble a monitor can be in.
///
/// The same two migration `0022`'s `monitor_alert_state_chk` allows, pinned by
/// [`tests::the_states_are_the_ones_the_column_accepts`]. Deliberately not
/// [`crate::samples::STATES`] minus something: a sample records what was seen
/// and an alert records trouble, and the two lists are different lengths
/// because they are different statements.
pub const OPENS: &[&str] = &["down", "warn"];

/// The activity verb a recovery writes on the asset the monitor watches.
///
/// Past tense, like every other verb in the log. `knobas_app::assets` reads
/// the same spelling to draw the line, and its own ack writes `acked` beside
/// it.
pub const VERB: &str = "recovered";

/// The state that closes an open alert.
///
/// `CONTEXT.md`, **Alert**: *"open until the monitor recovers"*. One word, and
/// [`crate::samples::UP`]'s, so "up" has one spelling in this crate.
pub const RECOVERS: &str = crate::samples::UP;

/// What one monitor's newest sample means for its alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconciled {
    /// Open one, carrying this state — one of [`OPENS`].
    Open(&'static str),
    /// Close the one that is open.
    Close,
    /// Leave the monitor's alerts exactly as they are.
    Nothing,
}

/// What to do about one monitor, given its newest sample and whether it
/// already has an alert open.
///
/// The whole rule, in one place and without a database, so the table of cases
/// below is a test and not a paragraph. `newest` is the state word the newest
/// sample carries, or `None` for a sample that missed (#443's absence
/// direction: a drifted read is a gap, never a *down* nobody is having).
#[must_use]
pub fn decide(newest: Option<&str>, already_open: bool) -> Reconciled {
    let Some(word) = newest else {
        return Reconciled::Nothing;
    };
    // `find` rather than `contains`, so the answer carries the `'static` word
    // this module owns and not the borrowed one a row handed in -- which is
    // what lets a caller bind it straight into the insert.
    if let Some(known) = OPENS.iter().copied().find(|known| *known == word) {
        return if already_open {
            Reconciled::Nothing
        } else {
            Reconciled::Open(known)
        };
    }
    if word == RECOVERS && already_open {
        return Reconciled::Close;
    }
    Reconciled::Nothing
}

/// The newest sample of every live monitor of one source, and whether that
/// monitor already has an alert open.
///
/// `distinct on` with the index's own order — `(entity_id, taken_at desc)` is
/// `monitor_sample_entity_idx` — so the newest row per monitor costs an index
/// scan and not a sort of the timeseries. `id desc` breaks the tie two samples
/// of the same poll would have; there is only ever one per poll, and a tie
/// resolved by insertion order is the honest one if that ever stops being
/// true.
///
/// Through `sync.live_item` and not `sync.item`, which is what makes *"a
/// paused monitor is left alone"* a property of the view: a tombstoned monitor
/// and a monitor of a source the reader disabled are both absent from the
/// roster, so neither is decided about and neither's alert is touched.
const ROSTER: &str = "select distinct on (i.entity_id)
            i.entity_id, s.state,
            exists (select 1 from knobas.monitor_alert a
                     where a.entity_id = i.entity_id and a.closed_at is null) as already_open
       from sync.live_item i
       join knobas.monitor_sample s on s.entity_id = i.entity_id
      where i.source_id = $1 and i.kind = $2
      order by i.entity_id, s.taken_at desc, s.id desc";

/// Every asset a recovering monitor watches, with the monitor's own name.
///
/// The `monitored-by` link (`CONTEXT.md`, **Monitor**) read from the monitor's
/// end, over `knobas.confirmed_link` and never `knobas.link`: a *proposed*
/// attachment is a guess, and a history line on somebody's server saying a
/// monitor nobody confirmed watches it recovered would be knobas writing a
/// fact out of a suggestion.
///
/// A monitor watching nothing yields no rows and therefore no lines, which is
/// the honest answer: story 64's line goes on *the affected asset*, and a
/// monitor nobody has finished wiring up affects none. `$1` is the monitor
/// ids, `$2` the relation.
const RECOVERED_ASSETS: &str = "select m.id as monitor_id, e.title as monitor_name, ast.id as asset_id
       from unnest($1::text[]) as m(id)
       join knobas.entity e on e.id = m.id
       join knobas.confirmed_link l
         on (l.from_id = m.id or l.to_id = m.id) and l.relation = $2
       join knobas.asset ast
         on ast.id = case when l.from_id = m.id then l.to_id else l.from_id end
      order by ast.id asc";

/// Reconcile every live monitor of `source_id` against its newest sample,
/// inside the run's own transaction.
///
/// Called from `run_locked` **immediately after** [`crate::samples::append`],
/// so the newest sample of every monitor on the roster is the one this run
/// just took. Three round trips at most, whatever the roster size: the roster
/// read, one insert of everything to open, one update of everything to close.
///
/// Answers with nothing: the run has no use for a count and nothing logs one,
/// so a shape carrying two numbers would be a shape nothing could be wrong
/// about -- the reason `OpenAlert` carries no `web_url`. What the reconcile
/// did is read back from the table, which is where the twelve tests in
/// `tests/alerts.rs` read it.
///
/// # Errors
///
/// [`sqlx::Error`] if any of the three statements fails; the caller rolls the
/// run back, which is what keeps an alert from claiming to have seen a state
/// the run never committed.
pub async fn reconcile(
    tx: &mut Transaction<'_, Postgres>,
    source_id: &str,
) -> Result<(), sqlx::Error> {
    let roster = sqlx::query(ROSTER)
        .bind(source_id)
        .bind(crate::samples::KIND)
        .fetch_all(&mut **tx)
        .await?;

    let mut opening: Vec<String> = Vec::new();
    let mut states: Vec<&'static str> = Vec::new();
    let mut closing: Vec<String> = Vec::new();
    for row in &roster {
        let entity_id: String = row.try_get("entity_id")?;
        let state: Option<String> = row.try_get("state")?;
        let already_open: bool = row.try_get("already_open")?;
        match decide(state.as_deref(), already_open) {
            Reconciled::Open(word) => {
                opening.push(entity_id);
                states.push(word);
            }
            Reconciled::Close => closing.push(entity_id),
            Reconciled::Nothing => {}
        }
    }

    if !opening.is_empty() {
        // `opened_at` is left to the column default, which is `now()` -- the
        // transaction timestamp, and therefore the same instant the samples
        // this decision was made from carry.
        sqlx::query(
            "insert into knobas.monitor_alert (entity_id, state)
             select * from unnest($1::text[], $2::text[])",
        )
        .bind(&opening)
        .bind(&states)
        .execute(&mut **tx)
        .await?;
    }
    if !closing.is_empty() {
        // `closed_at is null` again, although `decide` only asked to close
        // monitors that had one open: the statement is what is true of the
        // rows, and a closed alert must not have its `closed_at` moved by a
        // later run.
        sqlx::query(
            "update knobas.monitor_alert set closed_at = now()
              where closed_at is null and entity_id = any($1::text[])",
        )
        .bind(&closing)
        .execute(&mut **tx)
        .await?;
        recovered_lines(tx, source_id, &closing).await?;
    }
    Ok(())
}

/// One history line per asset a recovered monitor watches (spec #427 story
/// 64, issue #446).
///
/// **Only recovery writes one, and only on the asset.** Story 64 is *"recovery
/// to close the alert and remove an un-acked inbox item with a history line"*,
/// and the pair to it is #446's ack line, which the app writes. Opening one
/// writes nothing: an alert *is* the record that a monitor fell, it is drawn in
/// the Assets view and counted in the top strip from the moment it opens, and a
/// second copy of it in every affected asset's history would put a line in front
/// of the reader for something no person did. What recovery leaves behind is
/// the only thing that would otherwise be unrecoverable — the alert row goes
/// out of every open-alert read the instant it closes, so without this the
/// asset's history would have nothing to say about a night it spent down.
///
/// Inside the run's transaction, so a run that rolls back leaves no line
/// claiming a recovery it never committed — and, unlike the run's own `synced`
/// line, a failure here **fails the run**: that line is written after the
/// commit and is a log entry about work already durable, while this one is part
/// of the same write as the `closed_at` it describes.
///
/// The actor is `sync:<source_id>`, which is `knobas_core::activity`'s own
/// vocabulary for a synced event and what tells this line apart from the ack's
/// `user`.
async fn recovered_lines(
    tx: &mut Transaction<'_, Postgres>,
    source_id: &str,
    closing: &[String],
) -> Result<(), sqlx::Error> {
    let watched = sqlx::query(RECOVERED_ASSETS)
        .bind(closing)
        .bind(knobas_core::link::MONITORED_BY)
        .fetch_all(&mut **tx)
        .await?;
    let actor = format!("sync:{source_id}");
    for row in &watched {
        let asset_id: String = row.try_get("asset_id")?;
        let monitor_id: String = row.try_get("monitor_id")?;
        let monitor_name: String = row.try_get("monitor_name")?;
        // An asset id that does not parse is not a thing this run can write a
        // line about; `knobas.asset` constrains the namespace, so it cannot
        // happen from the join above, and guessing an entity would be worse
        // than saying nothing.
        let Ok(entity) = EntityRef::parse(&asset_id) else {
            continue;
        };
        activity::record_with(
            &mut **tx,
            &actor,
            VERB,
            Some(&entity),
            serde_json::json!({
                "monitor": monitor_id,
                "monitor_name": monitor_name,
            }),
        )
        .await
        .map_err(|error| match error {
            knobas_core::CoreError::Db(error) => error,
            other => sqlx::Error::Protocol(other.to_string()),
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list this module holds and the list the column accepts are one
    /// list, and the file that says so is the migration.
    ///
    /// `samples`' own pin, applied to the narrower vocabulary: a word added
    /// here and not there is a failed insert on the first poll that sees it --
    /// green in every unit test, red only against a real database.
    #[test]
    fn the_states_are_the_ones_the_column_accepts() {
        let migration =
            include_str!("../../knobas-db/migrations/0022_the_alert_a_monitor_opens.sql");
        let check = migration
            .split("monitor_alert_state_chk")
            .nth(1)
            .expect("the constraint is in the migration");
        for state in OPENS {
            assert!(
                check.contains(&format!("'{state}'")),
                "`{state}` is a state this module writes and the column refuses"
            );
        }
        assert_eq!(
            check.matches('\'').count(),
            OPENS.len() * 2,
            "the column accepts a word this module never writes"
        );
    }

    /// An alert's states are a subset of a sample's, and a strict one.
    ///
    /// The two lists are separate on purpose (see [`OPENS`]), so this is the
    /// pin that keeps them from drifting *apart* rather than one derived from
    /// the other: a word an alert may carry that no sample can ever hold would
    /// be a state nothing could open.
    #[test]
    fn every_alert_state_is_a_state_a_sample_can_hold() {
        for state in OPENS {
            assert!(
                crate::samples::STATES.contains(state),
                "`{state}` opens an alert and no sample can carry it"
            );
        }
        assert!(
            crate::samples::STATES.contains(&RECOVERS),
            "nothing a sample can carry closes an alert"
        );
        assert!(
            !OPENS.contains(&RECOVERS),
            "the word that closes an alert also opens one"
        );
    }

    /// The whole rule as a table: five sample words times two, plus the miss.
    ///
    /// Written out rather than derived from `OPENS`, so a sixth sample word
    /// arriving in `samples::STATES` has to be given a line here — the device
    /// `health::the_five_states_are_the_five_the_check_constraint_allows`
    /// uses, and for its reason.
    #[test]
    fn the_rule_is_the_whole_table() {
        use Reconciled::{Close, Nothing, Open};
        let cases = [
            // Nothing open yet.
            (None, false, Nothing, "a miss opens nothing"),
            (Some("up"), false, Nothing, "an up monitor is not news"),
            (Some("down"), false, Open("down"), "the crossing into down"),
            (Some("warn"), false, Open("warn"), "the crossing into warn"),
            (Some("pending"), false, Nothing, "pending is not trouble"),
            (
                Some("maintenance"),
                false,
                Nothing,
                "maintenance is not trouble",
            ),
            // One already open.
            (
                None,
                true,
                Nothing,
                "a drifted read is not a recovery: the alert stands",
            ),
            (Some("up"), true, Close, "the return to up"),
            (
                Some("down"),
                true,
                Nothing,
                "a second down poll does not open a second alert",
            ),
            (
                Some("warn"),
                true,
                Nothing,
                "one open per monitor, whatever the crossing",
            ),
            (
                Some("pending"),
                true,
                Nothing,
                "pending does not close what is open",
            ),
            (
                Some("maintenance"),
                true,
                Nothing,
                "silencing a monitor is not fixing it",
            ),
        ];
        for (newest, already_open, want, why) in cases {
            assert_eq!(
                decide(newest, already_open),
                want,
                "{why} ({newest:?}, already_open = {already_open})"
            );
        }
        assert_eq!(
            cases.len(),
            (crate::samples::STATES.len() + 1) * 2,
            "every sample word, and the miss, in both directions"
        );
    }

    /// A word no sample can carry decides nothing, in either direction.
    ///
    /// The failure direction #443's module header sets for the whole feature:
    /// a state this build has no meaning for is an absence, never a *down*
    /// nobody is having.
    #[test]
    fn a_word_this_build_does_not_know_decides_nothing() {
        assert_eq!(decide(Some("bewildered"), false), Reconciled::Nothing);
        assert_eq!(decide(Some("bewildered"), true), Reconciled::Nothing);
        assert_eq!(decide(Some(""), false), Reconciled::Nothing);
    }
}
