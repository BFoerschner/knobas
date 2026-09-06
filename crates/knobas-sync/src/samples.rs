//! One sample per poll per monitor: the knobas-owned timeseries (issue #443).
//!
//! Spec #427: *"At the end of every run of a source that emits `monitor`, the
//! engine appends one sample per live monitor (state, response time, taken at)
//! … Warn is derived from the response-time threshold setting at sample time.
//! Samples live in a knobas-owned table swept by the retention setting through
//! the existing retention tick; they are in the backup and out of the share
//! export."*
//!
//! # Why the engine keeps a history a source will not
//!
//! Uptime Kuma prunes its own heartbeats to a day, and `/metrics` -- the only
//! endpoint an API key opens (contract §4.2 E) -- publishes one number per
//! monitor with no history behind it and no clock in it. A monitor's past
//! therefore exists only if knobas writes it down as it goes.
//!
//! # A poll, not a change
//!
//! The row is written for every **live** monitor on every run, whether or not
//! the run emitted that monitor. That is not an accident of where the code
//! sits: the Kuma adapter's cursor is a digest of the last corpus, so an
//! unchanged Kuma emits *nothing at all* (contract battery clause 2), and a
//! timeseries built out of emitted items would go quiet on exactly the
//! monitors that are steadily up. Reading the mirror instead is also what
//! makes the *other* direction free: a monitor the run tombstoned is not in
//! `sync.live_item` any more, so "a monitor absent from this run gets no
//! sample" is a property of the view rather than a rule anyone maintains.
//!
//! # What this module owns, and what it borrows
//!
//! It owns the sample: what one is, the two settings that shape it, and the
//! retention rule that takes it away. It owns no clock -- [`prune`] takes
//! `now` as a parameter, the shape `knobas_app::time::passive::prune` has, so
//! a fixture can place the horizon; `knobas_app::backup::tick` is the one
//! wall-clock loop in the app and calls it from there. The rule stays with the
//! thing it is about; the clock stays where the app already keeps one.
//!
//! # Reading a monitor's state (ADR-0007, #277)
//!
//! The state comes from the adapter's **declared** `status_name` path and
//! never from a key spelled here. The response time has no declared slot to
//! come from -- `KindPaths` has no "a duration in milliseconds" field, and
//! adding one would be a `crates/knobas-source/src/**` change and a §10.8
//! conversation of its own -- so that read is under ADR-0007's *interim*
//! discipline and meets all three requirements:
//!
//! 1. **It misses, never guesses.** A payload with no `response_time_ms`, a
//!    value that is not a number, a negative one, or one no `i32` can hold:
//!    every one of them is `None`, and a sample with no reading derives no
//!    *warn*. The same applies to the state: a declaration that resolves to
//!    nothing, or to a word [`STATES`] does not hold, is `None` rather than a
//!    new state word.
//! 2. **One named place**, [`sample_of`], expanded by [`append`] and nowhere
//!    else.
//! 3. **The failure direction is absence**, pinned by this module's own tests
//!    and by `tests/samples.rs`'
//!    `a_monitor_whose_state_does_not_resolve_is_sampled_as_a_miss` and
//!    `a_source_declaring_no_state_path_samples_no_state`. A drifted read
//!    yields a row whose state is null, which draws as a gap in the Monitors
//!    tab's bar and opens no alert -- never a *down* nobody is having.

use chrono::{DateTime, Duration, Utc};
use knobas_core::payload::PayloadPath;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};

/// The entity kind a sample is a sample *of*.
///
/// The engine samples any source declaring this kind, not Uptime Kuma by
/// name: spec #427 says "a source that emits `monitor`", and a second
/// monitoring source would be one more configured source and no new code here.
pub const KIND: &str = "monitor";

/// The `knobas.setting` key holding the response time, in milliseconds, above
/// which an otherwise-up monitor samples as [`WARN`].
///
/// `knobas.setting` again, so neither setting needs a migration of its own --
/// the reasoning `backup::SCHEDULE_KEY` and `time::passive::SETTING_KEY`
/// record. Namespaced `monitoring.` because M4.1's alert notification category
/// will want its own.
pub const THRESHOLD_KEY: &str = "monitoring.response_time_warn_ms";

/// The `knobas.setting` key holding how many days of samples knobas keeps.
pub const RETENTION_KEY: &str = "monitoring.sample_retention_days";

/// Where *warn* begins when nobody has said otherwise: 1500 ms (spec #427,
/// story 56, and the roadmap's "One global response-time threshold (default
/// 1500 ms) makes the *warn* state").
pub const DEFAULT_THRESHOLD_MS: i64 = 1500;

/// How long knobas keeps a sample when nobody has said otherwise: ninety days
/// (spec #427, story 55, "so that history outlives Kuma's one-day pruning").
pub const DEFAULT_RETENTION_DAYS: i64 = 90;

/// The state words a sample may carry.
///
/// `up`, `down` and `warn` are `CONTEXT.md`'s **Monitor** vocabulary and the
/// health rollup's order; `pending` and `maintenance` are states Uptime Kuma
/// has that the rollup has no word for, kept because a sample records what was
/// seen and the rollup is a different question. **Anything else is a miss**
/// (see the module header), and this list is the same one migration `0021`'s
/// CHECK constraint holds, pinned by
/// `the_state_words_are_the_ones_the_column_accepts`.
pub const STATES: &[&str] = &["up", "down", "warn", "pending", "maintenance"];

/// The state a monitor answering inside the threshold is in.
pub const UP: &str = "up";

/// The state knobas derives, and Uptime Kuma does not have.
pub const WARN: &str = "warn";

/// The widest a threshold may be set: ten minutes.
///
/// Not a preference -- a bound. A threshold read back from a hand-edited row
/// or written by a settings dialog is clamped into this range on both sides,
/// so nothing downstream has to ask whether the number it was handed is one.
const THRESHOLD_RANGE: std::ops::RangeInclusive<i64> = 0..=600_000;

/// The narrowest and widest retention may be: one day to ten years.
///
/// The floor is one and not zero for [`prune`]'s sake: a retention of zero
/// would put the horizon at *now* and take every sample the poll a second ago
/// wrote, which is a table that is always empty rather than a retention rule.
const RETENTION_RANGE: std::ops::RangeInclusive<i64> = 1..=3650;

/// What one poll saw of one monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    /// One of [`STATES`], or `None` for a miss -- see the module header.
    pub state: Option<&'static str>,
    /// The monitor's last reading in milliseconds, or `None` when the source
    /// had none to give.
    pub response_time_ms: Option<i32>,
}

/// What one payload samples as, under `threshold_ms`.
///
/// The one place a payload is read outside an adapter here (ADR-0007
/// requirement 2). `declared` is the adapter's `status_name` candidates for
/// the `monitor` kind -- empty when it declares none, which is a source
/// saying nothing about where its state lives and therefore a miss.
///
/// **Warn replaces `up` and nothing else.** A monitor that is down stays down
/// however fast it answered: the rollup order is down > warn > up, so reading
/// a slow *down* as *warn* would be knobas making an outage look milder. A
/// `pending` or `maintenance` monitor is not being watched for latency either.
#[must_use]
pub fn sample_of(payload: &Value, declared: &[PayloadPath], threshold_ms: i64) -> Sample {
    let response_time_ms = response_time_of(payload);
    let word = knobas_core::payload::resolve_string(payload, declared);
    let state = word
        .as_deref()
        .and_then(|word| STATES.iter().copied().find(|known| *known == word));
    let over = matches!(response_time_ms, Some(ms) if i64::from(ms) > threshold_ms);
    Sample {
        state: match state {
            Some(UP) if over => Some(WARN),
            other => other,
        },
        response_time_ms,
    }
}

/// The reading a payload carries, in whole milliseconds, or nothing.
///
/// Everything unusable is `None` and never a zero: a payload with no
/// `response_time_ms`, one whose value is a string or an object, a negative
/// number (Uptime Kuma's `-1` for a check that did not answer, which the
/// adapter already drops -- read again here because this module is not
/// entitled to assume which source it is looking at), and one too large for
/// the column. `0` would be a claim about a check that did not happen.
fn response_time_of(payload: &Value) -> Option<i32> {
    let ms = payload.get("response_time_ms")?.as_f64()?;
    if !ms.is_finite() || ms < 0.0 || ms > f64::from(i32::MAX) {
        return None;
    }
    // `round`, not `trunc`: a reading of 1500.6 ms is nearer 1501, and the
    // threshold comparison happens after this so the two agree on one number.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "bounded to 0..=i32::MAX two lines above"
    )]
    Some(ms.round() as i32)
}

/// The candidate paths `descriptor` declares for a monitor's state, when it
/// declares the [`KIND`] at all.
///
/// `Some(vec![])` is *this source emits monitors and says nothing about where
/// their state lives*, which is a miss per monitor and still a row per
/// monitor. `None` is *this source emits no monitors*, and it is a **saved
/// round trip rather than a safety check** -- the rule
/// `run_locked`'s `!sweep_kinds.is_empty()` comment states about itself, and
/// stated here for the same reason. [`append`]'s roster read is filtered on
/// `kind = 'monitor'`, so a source with no monitors would read an empty
/// roster and write nothing anyway; what this saves is the query.
///
/// Measured, not assumed: breaking *either* guard alone leaves
/// `tests/samples.rs`' `a_source_that_emits_no_monitor_kind_writes_no_samples`
/// green, because each is sufficient on its own. The read's filter is the one
/// that is load-bearing in general -- spec #427 says "a source that emits
/// `monitor`", not "a source that emits only monitors" -- and
/// `a_source_that_emits_two_kinds_samples_only_its_monitors` is the fixture
/// that can tell the two apart.
#[must_use]
pub(crate) fn state_paths(
    descriptor: &knobas_source::SourceDescriptor,
) -> Option<Vec<PayloadPath>> {
    descriptor
        .entity_kinds
        .iter()
        .any(|kind| kind.id == KIND)
        .then(|| {
            descriptor
                .payload_paths
                .iter()
                .find(|paths| paths.kind == KIND)
                .map(|paths| paths.status_name.clone())
                .unwrap_or_default()
        })
}

/// Append one sample for every live monitor of `source_id`, inside the run's
/// own transaction.
///
/// Called from `run_locked` **after** the sweep and before the cursor update,
/// so that a monitor this very run tombstoned is already out of
/// `sync.live_item` and gets no row. Inside the transaction, so a run that
/// fails leaves no samples claiming to have seen something it never committed.
///
/// One `select` and one `insert … select unnest(…)`, whatever the roster size.
///
/// # Errors
///
/// [`sqlx::Error`] if either statement fails; the caller rolls the run back.
pub(crate) async fn append(
    tx: &mut Transaction<'_, Postgres>,
    source_id: &str,
    declared: &[PayloadPath],
) -> Result<u64, sqlx::Error> {
    let threshold = clamped(
        setting(&mut **tx, THRESHOLD_KEY).await?,
        DEFAULT_THRESHOLD_MS,
        THRESHOLD_RANGE,
    );

    let live: Vec<(String, Value)> = sqlx::query_as(
        "select entity_id, payload from sync.live_item where source_id = $1 and kind = $2",
    )
    .bind(source_id)
    .bind(KIND)
    .fetch_all(&mut **tx)
    .await?;
    if live.is_empty() {
        return Ok(0);
    }

    let mut ids: Vec<String> = Vec::with_capacity(live.len());
    let mut states: Vec<Option<String>> = Vec::with_capacity(live.len());
    let mut readings: Vec<Option<i32>> = Vec::with_capacity(live.len());
    for (entity_id, payload) in live {
        let sample = sample_of(&payload, declared, threshold);
        ids.push(entity_id);
        states.push(sample.state.map(ToOwned::to_owned));
        readings.push(sample.response_time_ms);
    }

    // `taken_at` is left to the column default, which is `now()` -- the
    // **transaction** timestamp, so every row of one poll carries one instant
    // and "the samples of this run" is an equality rather than a window.
    let written = sqlx::query(
        "insert into knobas.monitor_sample (entity_id, state, response_time_ms)
         select * from unnest($1::text[], $2::text[], $3::int[])",
    )
    .bind(&ids)
    .bind(&states)
    .bind(&readings)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(written)
}

/// Delete every sample older than the retention setting reaches back to.
///
/// `now` is a parameter and not a clock, so a fixture can place the horizon --
/// the shape `knobas_app::time::passive::prune` has, and for its reason.
///
/// No stamp beside it, which is the one way this differs from the observation
/// sweep: that one records what it swept because a *day read* has to know
/// whether its evidence is complete, and no reader of a sample asks that
/// question -- a bar draws the samples there are.
///
/// # Errors
///
/// [`sqlx::Error`] if the read of the setting or the delete fails.
pub async fn prune(pool: &PgPool, now: DateTime<Utc>) -> Result<u64, sqlx::Error> {
    let days = retention_days(pool).await?;
    let taken = sqlx::query("delete from knobas.monitor_sample where taken_at < $1")
        .bind(now - Duration::days(days))
        .execute(pool)
        .await?
        .rows_affected();
    Ok(taken)
}

/// Where *warn* begins today, in milliseconds.
///
/// # Errors
/// [`sqlx::Error`] if the read fails.
pub async fn threshold_ms(pool: &PgPool) -> Result<i64, sqlx::Error> {
    Ok(clamped(
        setting(pool, THRESHOLD_KEY).await?,
        DEFAULT_THRESHOLD_MS,
        THRESHOLD_RANGE,
    ))
}

/// Move where *warn* begins, and answer with what is now stored.
///
/// Clamped on the way in, the rule `BackupSchedule::clamped` follows: a
/// settings dialog and a hand-edited row are both places a number arrives
/// from, and only one of them has a spinner on it.
///
/// # Errors
/// [`sqlx::Error`] if the write fails.
pub async fn set_threshold_ms(pool: &PgPool, ms: i64) -> Result<i64, sqlx::Error> {
    let ms = ms.clamp(*THRESHOLD_RANGE.start(), *THRESHOLD_RANGE.end());
    store(pool, THRESHOLD_KEY, ms).await?;
    Ok(ms)
}

/// How many days of samples knobas keeps today.
///
/// # Errors
/// [`sqlx::Error`] if the read fails.
pub async fn retention_days(pool: &PgPool) -> Result<i64, sqlx::Error> {
    Ok(clamped(
        setting(pool, RETENTION_KEY).await?,
        DEFAULT_RETENTION_DAYS,
        RETENTION_RANGE,
    ))
}

/// Change how long samples are kept, and answer with what is now stored.
///
/// # Errors
/// [`sqlx::Error`] if the write fails.
pub async fn set_retention_days(pool: &PgPool, days: i64) -> Result<i64, sqlx::Error> {
    let days = days.clamp(*RETENTION_RANGE.start(), *RETENTION_RANGE.end());
    store(pool, RETENTION_KEY, days).await?;
    Ok(days)
}

/// One `knobas.setting` row as a whole number, or `None` when it is absent or
/// no longer decodes as one.
///
/// Absent and unreadable are the same answer on purpose, the rule
/// `backup::read_setting` records: a value that has stopped parsing is a thing
/// this feature cannot read, and the response to "I cannot read your
/// threshold" is the ratified default, not a failed sync.
async fn setting<'e, E>(db: E, key: &str) -> Result<Option<i64>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let stored: Option<Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(key)
            .fetch_optional(db)
            .await?;
    Ok(stored.and_then(|value| value.as_i64()))
}

async fn store(pool: &PgPool, key: &str, value: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into knobas.setting (key, value) values ($1, $2)
         on conflict (key) do update set value = excluded.value, updated_at = now()",
    )
    .bind(key)
    .bind(Value::from(value))
    .execute(pool)
    .await?;
    Ok(())
}

/// The stored number brought into range, or the default when there is none.
fn clamped(stored: Option<i64>, default: i64, range: std::ops::RangeInclusive<i64>) -> i64 {
    stored.map_or(default, |value| value.clamp(*range.start(), *range.end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_at(path: &str) -> Vec<PayloadPath> {
        vec![PayloadPath::of([path])]
    }

    fn monitor(state: Value, response_time_ms: Value) -> Value {
        serde_json::json!({ "state": state, "response_time_ms": response_time_ms })
    }

    /// The list this module holds and the list the column accepts are one
    /// list, and the file that says so is the migration.
    ///
    /// A word added here and not there is a failed insert on every poll that
    /// sees it -- green in every unit test, red only against a real database.
    #[test]
    fn the_state_words_are_the_ones_the_column_accepts() {
        let migration =
            include_str!("../../knobas-db/migrations/0021_a_sample_per_poll_per_monitor.sql");
        let check = migration
            .split("monitor_sample_state_chk")
            .nth(1)
            .expect("the constraint is in the migration");
        for state in STATES {
            assert!(
                check.contains(&format!("'{state}'")),
                "`{state}` is a state this module writes and the column refuses"
            );
        }
        assert_eq!(
            check.matches('\'').count(),
            STATES.len() * 2,
            "the column accepts a word this module never writes"
        );
    }

    #[test]
    fn a_slow_up_monitor_is_warn_and_a_quick_one_is_up() {
        let paths = state_at("state");
        assert_eq!(
            sample_of(&monitor("up".into(), 1501.into()), &paths, 1500),
            Sample {
                state: Some(WARN),
                response_time_ms: Some(1501)
            }
        );
        assert_eq!(
            sample_of(&monitor("up".into(), 1500.into()), &paths, 1500),
            Sample {
                state: Some(UP),
                response_time_ms: Some(1500)
            },
            "*at* the threshold is not over it"
        );
    }

    /// The rollup order is down > warn > up, so warn is never a milder
    /// reading of a broken monitor.
    #[test]
    fn only_an_up_monitor_becomes_warn() {
        let paths = state_at("state");
        for state in ["down", "pending", "maintenance"] {
            let sample = sample_of(&monitor(state.into(), 9000.into()), &paths, 1500);
            assert_eq!(
                sample.state,
                Some(state),
                "a {state} monitor answering slowly is still {state}"
            );
        }
    }

    /// Requirement 1 and 3 of ADR-0007, in one place: every unusable shape
    /// misses, and a miss is an absence rather than a wrong value.
    #[test]
    fn every_unusable_shape_is_a_miss() {
        let paths = state_at("state");
        let cases = [
            ("no state key at all", serde_json::json!({})),
            ("a null state", monitor(Value::Null, 35.into())),
            ("a state that is not a string", monitor(7.into(), 35.into())),
            ("a blank state", monitor("   ".into(), 35.into())),
            (
                "a word this module has no meaning for",
                monitor("bewildered".into(), 35.into()),
            ),
        ];
        for (why, payload) in cases {
            assert_eq!(
                sample_of(&payload, &paths, 1500).state,
                None,
                "{why} should miss"
            );
        }

        let readings = [
            ("absent", serde_json::json!({ "state": "up" })),
            ("null", monitor("up".into(), Value::Null)),
            ("a string", monitor("up".into(), "35".into())),
            ("Kuma's -1 sentinel", monitor("up".into(), (-1).into())),
            (
                "wider than the column",
                monitor("up".into(), serde_json::json!(4e9)),
            ),
        ];
        for (why, payload) in readings {
            let sample = sample_of(&payload, &paths, 1500);
            assert_eq!(sample.response_time_ms, None, "{why} should miss");
            assert_eq!(
                sample.state,
                Some(UP),
                "{why} leaves the state alone: no reading derives no warn"
            );
        }
    }

    /// The declaration decides where to look, not this module (#277).
    #[test]
    fn the_declared_path_is_what_is_read() {
        let payload = serde_json::json!({ "state": "up", "elsewhere": "down" });
        assert_eq!(
            sample_of(&payload, &state_at("elsewhere"), 1500).state,
            Some("down"),
            "the declaration points at `elsewhere` and that is what is read"
        );
        assert_eq!(
            sample_of(&payload, &[], 1500).state,
            None,
            "a source that declares nothing gets no state, not the obvious key"
        );
    }

    /// A reading is rounded once, and the threshold is compared against the
    /// rounded number, so the stored row and the derived state agree.
    #[test]
    fn a_reading_is_rounded_and_the_threshold_reads_the_rounded_number() {
        let paths = state_at("state");
        let sample = sample_of(
            &monitor("up".into(), serde_json::json!(1500.6)),
            &paths,
            1500,
        );
        assert_eq!(
            sample,
            Sample {
                state: Some(WARN),
                response_time_ms: Some(1501)
            }
        );
    }

    #[test]
    fn a_stored_setting_out_of_range_is_brought_back_into_it() {
        assert_eq!(clamped(None, 1500, THRESHOLD_RANGE), 1500);
        assert_eq!(clamped(Some(-9), 1500, THRESHOLD_RANGE), 0);
        assert_eq!(clamped(Some(i64::MAX), 1500, THRESHOLD_RANGE), 600_000);
        assert_eq!(clamped(Some(0), 90, RETENTION_RANGE), 1, "never zero days");
    }

    /// A descriptor's kinds decide whether a source is sampled at all, and its
    /// declarations decide what is read -- two questions with two answers.
    #[test]
    fn a_source_with_no_monitor_kind_has_no_state_paths() {
        let mut descriptor = knobas_source::SourceDescriptor {
            id: "fake".into(),
            adapter_kind: "fake".into(),
            name: "Fake".into(),
            capabilities: Vec::new(),
            adapter_version: "0".into(),
            auth_methods: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![knobas_source::KindInfo {
                id: "ticket".into(),
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "TI".into(),
                full_sync_exhaustive: true,
            }],
            config_schema: serde_json::json!({}),
            payload_paths: Vec::new(),
        };
        assert_eq!(state_paths(&descriptor), None);

        descriptor.entity_kinds.push(knobas_source::KindInfo {
            id: KIND.into(),
            label: "Monitor".into(),
            plural: "Monitors".into(),
            monogram: "MO".into(),
            full_sync_exhaustive: true,
        });
        assert_eq!(
            state_paths(&descriptor),
            Some(Vec::new()),
            "a source that emits monitors and declares no path is sampled, and misses"
        );

        descriptor.payload_paths.push(knobas_source::KindPaths {
            kind: KIND.into(),
            status_name: state_at("state"),
            ..knobas_source::KindPaths::default()
        });
        assert_eq!(state_paths(&descriptor), Some(state_at("state")));
    }
}
