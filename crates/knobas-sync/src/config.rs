//! `knobas.source_config`: the schedule, the credential verdict, and the
//! backoff the scheduler obeys.
//!
//! Spec §3: one configuration per source, a per-source sync schedule, and
//! credential health (PAT expiry countdown, 401 detection). The secret itself
//! is **never** here -- interfaces §3 puts it in the OS keychain, keyed by the
//! source id, and this module has no field that could hold one.
//!
//! [`AuthState`] and [`CredentialHealth`] are [`crate::health`]'s, re-exported
//! rather than redeclared: they are the payload of the `source:health` event
//! and have a TypeScript mirror pinned against them, so a second spelling of
//! either would be a second wire format.
//!
//! # Why `next_run_at` is not a column
//!
//! P7: the interval means "seconds after the previous run *finished*", so the
//! next time is a function of `knobas.sync_run`'s newest `finished_at`, the
//! interval, and the persisted `backoff_until`. A stored column would be a
//! second truth that a crashed run, a manual sync or an edited interval
//! immediately falsifies.

use std::time::Duration;

use chrono::{DateTime, Utc};
use knobas_source::AuthMethod;
use sqlx::PgPool;

pub use crate::health::{AuthState, CredentialHealth};

/// The floor on a sync interval. A source is a remote system with a rate
/// limit; letting the Add-source form store `1` is how knobas gets an account
/// blocked.
pub const MIN_SYNC_INTERVAL_SECS: u32 = 60;

/// P7's ladder, in seconds: 1 → 2 → 5 → 15 → 60 minutes, then flat.
pub const BACKOFF_LADDER_SECS: [i64; 5] = [60, 120, 300, 900, 3600];

/// How long to wait after `consecutive_failures` failed runs in a row.
///
/// Clamped at the top rung: a source that has been unreachable for a week is
/// retried hourly for ever, which is cheap and keeps it recovering by itself
/// the moment the network comes back. Clamped at the bottom too -- a caller
/// that computed zero (or, through its own arithmetic bug, a negative) still
/// gets the first rung rather than an instant retry loop.
#[must_use]
pub fn backoff_after(consecutive_failures: i64) -> Duration {
    let rung = consecutive_failures.max(1) - 1;
    let rung = usize::try_from(rung).unwrap_or(usize::MAX);
    let secs = BACKOFF_LADDER_SECS[rung.min(BACKOFF_LADDER_SECS.len() - 1)];
    Duration::from_secs(secs.unsigned_abs())
}

/// Reject an interval below the floor.
///
/// # Errors
/// A message the IPC layer turns into `IpcErrorCode::Invalid`.
pub fn check_interval(secs: u32) -> Result<u32, &'static str> {
    if secs < MIN_SYNC_INTERVAL_SECS {
        return Err("a sync interval below 60 seconds would hammer the source");
    }
    Ok(secs)
}

/// Parse a stored `auth_state`.
///
/// Anything unrecognised reads as [`AuthState::Unknown`] -- a row written by a
/// newer knobas must not stop this one from listing the source. Driven by
/// `ALL`, so a new variant is readable the moment it is writable.
#[must_use]
fn auth_state_from_db(raw: &str) -> AuthState {
    AuthState::ALL
        .iter()
        .copied()
        .find(|state| state.as_str() == raw)
        .unwrap_or(AuthState::Unknown)
}

/// How a source authenticates, including "it does not".
///
/// [`AuthKind::None`] exists for the compiled-in mock, which reaches nothing
/// and needs no credential. The IPC surface never produces it -- `NewSource`
/// carries a plain [`AuthMethod`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
    None,
    Method(AuthMethod),
}

impl AuthKind {
    /// The spelling stored in `source_config.auth_kind`.
    ///
    /// **No wildcard arm**: adding an [`AuthMethod`] variant must stop this
    /// module compiling until the variant has a stored spelling, rather than
    /// silently landing in the column as another one.
    #[must_use]
    pub fn as_db(self) -> &'static str {
        match self {
            AuthKind::None => "none",
            AuthKind::Method(AuthMethod::UserPassword) => "user_password",
            AuthKind::Method(AuthMethod::Pat) => "pat",
            AuthKind::Method(AuthMethod::ApiToken) => "api_token",
            AuthKind::Method(AuthMethod::OAuth) => "oauth",
        }
    }

    #[must_use]
    pub fn from_db(raw: &str) -> AuthKind {
        match raw {
            "user_password" => AuthKind::Method(AuthMethod::UserPassword),
            "pat" => AuthKind::Method(AuthMethod::Pat),
            "api_token" => AuthKind::Method(AuthMethod::ApiToken),
            "oauth" => AuthKind::Method(AuthMethod::OAuth),
            _ => AuthKind::None,
        }
    }

    /// The method a stored secret would be for, or `None` when the source
    /// needs no credential.
    #[must_use]
    pub fn method(self) -> Option<AuthMethod> {
        match self {
            AuthKind::None => None,
            AuthKind::Method(m) => Some(m),
        }
    }
}

/// One configured source, as the scheduler and the sources view see it.
#[derive(Debug, Clone)]
pub struct SourceConfigRow {
    pub id: String,
    pub adapter_kind: String,
    pub display_name: String,
    pub base_url: String,
    pub auth_kind: AuthKind,
    pub config: serde_json::Value,
    pub sync_interval_secs: u32,
    pub enabled: bool,
    pub cursor: Option<String>,
    pub health: CredentialHealth,
    pub backoff_until: Option<DateTime<Utc>>,
}

/// What `add_source` writes. Deliberately holds **no secret**: the keychain
/// item is written first, by the caller, and this row records only what may
/// live in Postgres (§14).
#[derive(Debug, Clone)]
pub struct InsertConfig {
    pub id: String,
    pub adapter_kind: String,
    pub display_name: String,
    pub base_url: String,
    pub auth_kind: AuthKind,
    pub config: serde_json::Value,
    pub sync_interval_secs: u32,
    pub enabled: bool,
}

/// What `update_source` may change. No `id` and no `adapter_kind`: the id is
/// the entity namespace and is immutable (P10).
#[derive(Debug, Clone, Default)]
pub struct PatchConfig {
    pub display_name: Option<String>,
    pub base_url: Option<String>,
    pub config: Option<serde_json::Value>,
    pub sync_interval_secs: Option<u32>,
    pub enabled: Option<bool>,
}

/// A source the scheduler should run now.
#[derive(Debug, Clone)]
pub struct DueSource {
    pub id: String,
    /// No run has ever finished for it, so this run is the initial sync --
    /// which is what the first-run wizard shows progress for.
    pub first_run: bool,
}

/// Every column this module reads, named. Never `select *`: `sync.live_item`
/// and this table are both read into `FromRow` structs elsewhere, and the
/// project rule is that a `tsvector` must never be able to wander into one.
const COLUMNS: &str = "id, kind as adapter_kind, display_name, base_url, auth_kind, config,
                       sync_interval_secs, enabled, cursor,
                       auth_state, auth_checked_at, auth_detail, secret_expires_at, backoff_until";

/// The raw shape of a `source_config` row, before the enums are parsed.
#[derive(sqlx::FromRow)]
struct RawRow {
    id: String,
    adapter_kind: String,
    display_name: String,
    base_url: String,
    auth_kind: String,
    config: serde_json::Value,
    sync_interval_secs: i32,
    enabled: bool,
    cursor: Option<String>,
    auth_state: String,
    auth_checked_at: Option<DateTime<Utc>>,
    auth_detail: Option<String>,
    secret_expires_at: Option<DateTime<Utc>>,
    backoff_until: Option<DateTime<Utc>>,
}

impl From<RawRow> for SourceConfigRow {
    fn from(raw: RawRow) -> Self {
        SourceConfigRow {
            health: CredentialHealth {
                source_id: raw.id.clone(),
                state: auth_state_from_db(&raw.auth_state),
                checked_at: raw.auth_checked_at,
                detail: raw.auth_detail,
                secret_expires_at: raw.secret_expires_at,
            },
            id: raw.id,
            adapter_kind: raw.adapter_kind,
            display_name: raw.display_name,
            base_url: raw.base_url,
            auth_kind: AuthKind::from_db(&raw.auth_kind),
            config: raw.config,
            // The column is `int` and the interval is never negative; a
            // corrupted row clamps rather than panicking a scheduler tick.
            sync_interval_secs: u32::try_from(raw.sync_interval_secs)
                .unwrap_or(MIN_SYNC_INTERVAL_SECS),
            enabled: raw.enabled,
            cursor: raw.cursor,
            backoff_until: raw.backoff_until,
        }
    }
}

/// Every configured source, id order.
///
/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn list(pool: &PgPool) -> Result<Vec<SourceConfigRow>, sqlx::Error> {
    let sql = format!("select {COLUMNS} from knobas.source_config order by id");
    let rows: Vec<RawRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One source, or `None`.
///
/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn get(pool: &PgPool, id: &str) -> Result<Option<SourceConfigRow>, sqlx::Error> {
    let sql = format!("select {COLUMNS} from knobas.source_config where id = $1");
    let row: Option<RawRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Into::into))
}

/// Insert a new source and return the row as stored.
///
/// # Errors
/// [`sqlx::Error`] -- a duplicate id surfaces as a unique-violation
/// `Database` error, which the IPC layer maps to `IpcErrorCode::Conflict`.
pub async fn insert(pool: &PgPool, new: &InsertConfig) -> Result<SourceConfigRow, sqlx::Error> {
    let sql = format!(
        "insert into knobas.source_config
             (id, kind, display_name, base_url, auth_kind, config, sync_interval_secs, enabled)
         values ($1, $2, $3, $4, $5, $6, $7, $8)
         returning {COLUMNS}"
    );
    let row: RawRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(&new.id)
        .bind(&new.adapter_kind)
        .bind(&new.display_name)
        .bind(&new.base_url)
        .bind(new.auth_kind.as_db())
        .bind(&new.config)
        .bind(i32::try_from(new.sync_interval_secs).unwrap_or(i32::MAX))
        .bind(new.enabled)
        .fetch_one(pool)
        .await?;
    Ok(row.into())
}

/// Apply a patch; `None` fields are left as they are.
///
/// One statement rather than a built one: `coalesce($n, column)` says "keep it"
/// for every absent field, so there is no dynamic SQL here at all.
///
/// # Errors
/// [`sqlx::Error`] if the update fails.
pub async fn patch(
    pool: &PgPool,
    id: &str,
    patch: &PatchConfig,
) -> Result<Option<SourceConfigRow>, sqlx::Error> {
    let sql = format!(
        "update knobas.source_config set
             display_name       = coalesce($2, display_name),
             base_url           = coalesce($3, base_url),
             config             = coalesce($4, config),
             sync_interval_secs = coalesce($5, sync_interval_secs),
             enabled            = coalesce($6, enabled)
         where id = $1
         returning {COLUMNS}"
    );
    let row: Option<RawRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(patch.display_name.as_deref())
        .bind(patch.base_url.as_deref())
        .bind(patch.config.as_ref())
        .bind(
            patch
                .sync_interval_secs
                .map(|s| i32::try_from(s).unwrap_or(i32::MAX)),
        )
        .bind(patch.enabled)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Into::into))
}

/// Purge one source's mirror: drop its `sync.item` rows and tombstone the
/// entities they named.
///
/// **One statement, in one place, because two callers apply it and they must
/// not drift.** [`delete`] runs it inside the delete's own transaction, and the
/// scheduler runs it again when a run that was still in flight at delete time
/// finally settles -- that run's late commit writes the mirror back, and worse,
/// its entity upsert sets `deleted_at = excluded.deleted_at`, which un-does the
/// tombstone and puts the purged entities back in `sync.live_item` for ever
/// (#127). A second copy of this CTE is a second thing to keep in step with
/// `knobas.entity`'s tombstone rule.
///
/// Tombstoned, never deleted, for the reason [`delete`] gives: links, notes and
/// activity rows point at these entities and `sync.item.entity_id` cascades.
const PURGE_ITEMS: &str = r#"with gone as (
     delete from sync.item where source_id = $1 returning entity_id
   )
   update knobas.entity e
      set deleted_at = coalesce(e.deleted_at, now())
     from gone
    where e.id = gone.entity_id"#;

/// Apply [`PURGE_ITEMS`] on its own, outside any transaction of the caller's.
///
/// The scheduler's half of #127: `delete_source` purged already, but the run
/// that was in flight at the time had not committed yet, so the purge has to
/// happen once more after it does.
///
/// # Errors
/// [`sqlx::Error`] if the statement fails.
pub async fn purge_items(pool: &PgPool, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query(PURGE_ITEMS).bind(id).execute(pool).await?;
    Ok(())
}

/// Delete a source's configuration, optionally purging its synced mirror.
///
/// The entities are **tombstoned, never deleted**: links, notes and activity
/// rows point at them, and `sync.item.entity_id` cascades from
/// `knobas.entity`, so deleting entities would take the mirror with it and
/// strand every reference (interfaces §3, Delete). Returns whether a
/// configuration row was there to remove.
///
/// The source's `knobas.sync_run` history is deliberately **kept**: there is no
/// FK, and deleting a source must not rewrite what happened (interfaces §1).
///
/// # Errors
/// [`sqlx::Error`] if any statement fails; all of them share one transaction.
pub async fn delete(pool: &PgPool, id: &str, purge_items: bool) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    if purge_items {
        sqlx::query(PURGE_ITEMS).bind(id).execute(&mut *tx).await?;
    }
    let removed = sqlx::query("delete from knobas.source_config where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(removed > 0)
}

/// Record a credential verdict, and say whether it *moved*.
///
/// One statement, because the answer depends on the row's value **before** the
/// update and Postgres' `RETURNING` yields the new one. A CTE reading the row
/// alongside the `UPDATE` sees the statement's snapshot -- the pre-update
/// values -- which is exactly the comparison "emit `source:health` on a health
/// change only" (interfaces §2.3) needs.
///
/// `auth_checked_at` moves on **every** call, changed or not: the sources view
/// shows "checked 4 min ago", and freshness is not a change.
const SET_HEALTH: &str = r#"
with old as (
  select auth_state, auth_detail from knobas.source_config where id = $1
),
upd as (
  update knobas.source_config
     set auth_state        = $2,
         auth_detail       = $3,
         auth_checked_at   = now(),
         -- A connection test that learned nothing about expiry must not erase
         -- what an earlier one learned.
         secret_expires_at = coalesce($4, secret_expires_at)
   where id = $1
   returning auth_state, auth_checked_at, auth_detail, secret_expires_at
)
select upd.auth_state, upd.auth_checked_at, upd.auth_detail, upd.secret_expires_at,
       (old.auth_state is distinct from upd.auth_state
        or old.auth_detail is distinct from upd.auth_detail) as changed
  from upd cross join old
"#;

#[derive(sqlx::FromRow)]
struct HealthRow {
    auth_state: String,
    auth_checked_at: Option<DateTime<Utc>>,
    auth_detail: Option<String>,
    secret_expires_at: Option<DateTime<Utc>>,
    changed: bool,
}

/// Write the verdict; `Ok(None)` means no such source.
///
/// The `bool` is **changed**, and it is the only thing that may fire
/// `source:health`.
///
/// # Errors
/// [`sqlx::Error`] if the statement fails.
pub async fn set_health(
    pool: &PgPool,
    id: &str,
    state: AuthState,
    detail: Option<&str>,
    secret_expires_at: Option<DateTime<Utc>>,
) -> Result<Option<(CredentialHealth, bool)>, sqlx::Error> {
    let row: Option<HealthRow> = sqlx::query_as(SET_HEALTH)
        .bind(id)
        .bind(state.as_str())
        .bind(detail)
        .bind(secret_expires_at)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| {
        (
            CredentialHealth {
                source_id: id.to_owned(),
                state: auth_state_from_db(&r.auth_state),
                checked_at: r.auth_checked_at,
                detail: r.auth_detail,
                secret_expires_at: r.secret_expires_at,
            },
            r.changed,
        )
    }))
}

/// The cheap poll the top strip's sync monograms use (interfaces §2.2).
///
/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn health_all(pool: &PgPool) -> Result<Vec<CredentialHealth>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        auth_state: String,
        auth_checked_at: Option<DateTime<Utc>>,
        auth_detail: Option<String>,
        secret_expires_at: Option<DateTime<Utc>>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "select id, auth_state, auth_checked_at, auth_detail, secret_expires_at
           from knobas.source_config order by id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| CredentialHealth {
            source_id: r.id,
            state: auth_state_from_db(&r.auth_state),
            checked_at: r.auth_checked_at,
            detail: r.auth_detail,
            secret_expires_at: r.secret_expires_at,
        })
        .collect())
}

/// Hold a source off until `until`. Persisted, so a dead source is not
/// hammered again on the next app start (interfaces §1, point 3).
///
/// # Errors
/// [`sqlx::Error`] if the update fails.
pub async fn set_backoff(pool: &PgPool, id: &str, until: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query("update knobas.source_config set backoff_until = $2 where id = $1")
        .bind(id)
        .bind(until)
        .execute(pool)
        .await?;
    Ok(())
}

/// Release a source: a successful run, or a re-entered credential.
///
/// # Errors
/// [`sqlx::Error`] if the update fails.
pub async fn clear_backoff(pool: &PgPool, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("update knobas.source_config set backoff_until = null where id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Which sources are due right now.
///
/// The whole P7 schedule in one statement:
///
/// * `next = last finished + interval`, or **now** if nothing has finished yet
///   (a never-synced source runs at once -- that is the first-run sync);
/// * clamped up by the persisted `backoff_until`. **`greatest` ignores NULLs**
///   in PostgreSQL (unlike the SQL standard), which is precisely what is wanted
///   here: no backoff means the interval decides, and a backoff on a
///   never-finished source still holds it off;
/// * `enabled = false` and the two states that need a human are excluded
///   outright -- no request, no backoff churn (interfaces §3, "Missing").
///
/// A run *in flight* must not reset the schedule -- a source with an open run
/// on top of a finished one is not never-synced, or the scheduler would start
/// a second run of something already running and call it a first sync. What
/// actually guarantees that is `max()` ignoring NULLs, so the explicit
/// `finished_at is not null` is **redundant for correctness**: deleting it
/// changes no result, and a mutation test confirms it changes none.
///
/// It is kept for the index rather than the semantics -- it is the predicate
/// of `sync_run_running_idx`'s complement, and it says in the query what the
/// lateral is for. The behaviour itself is pinned by
/// `a_run_still_in_flight_does_not_reset_the_schedule`, which asserts the
/// outcome and not this clause.
const DUE: &str = r#"
select c.id,
       f.last_finished_at is null as first_run
  from knobas.source_config c
  left join lateral (
      select max(finished_at) as last_finished_at
        from knobas.sync_run
       where source_id = c.id and finished_at is not null
  ) f on true
 where c.enabled
   and c.auth_state not in ('unauthorized', 'missing_secret')
   and coalesce(
         greatest(
           f.last_finished_at + make_interval(secs => c.sync_interval_secs::double precision),
           c.backoff_until
         ),
         now()
       ) <= now()
 order by c.id
"#;

/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn due(pool: &PgPool) -> Result<Vec<DueSource>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        first_run: bool,
    }
    let rows: Vec<Row> = sqlx::query_as(DUE).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|r| DueSource {
            id: r.id,
            first_run: r.first_run,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two states P7 excludes from scheduling are excluded *by name* in
    /// [`DUE`], which no Rust type checks. Pinned from the enum side so that
    /// renaming a variant's stored spelling cannot silently make a
    /// `unauthorized` source schedulable again.
    #[test]
    fn the_states_the_schedule_excludes_are_spelled_the_way_the_enum_spells_them() {
        for state in [AuthState::Unauthorized, AuthState::MissingSecret] {
            assert!(
                DUE.contains(&format!("'{}'", state.as_str())),
                "{state:?} needs a human, so the schedule must exclude it: {DUE}"
            );
        }
        for state in [AuthState::Ok, AuthState::Unreachable, AuthState::Unknown] {
            assert!(
                !DUE.contains(&format!("'{}'", state.as_str())),
                "{state:?} is retryable and must stay schedulable"
            );
        }
    }

    /// Every stored spelling reads back as the variant that wrote it, and an
    /// unknown one degrades rather than panicking a scheduler tick.
    #[test]
    fn an_auth_state_round_trips_and_an_unrecognised_one_reads_as_unknown() {
        for state in AuthState::ALL {
            assert_eq!(auth_state_from_db(state.as_str()), *state);
        }
        assert_eq!(
            auth_state_from_db("written_by_a_newer_knobas"),
            AuthState::Unknown
        );
        assert_eq!(auth_state_from_db(""), AuthState::Unknown);
    }

    #[test]
    fn an_auth_kind_round_trips_and_an_unrecognised_one_reads_as_none() {
        for kind in [
            AuthKind::None,
            AuthKind::Method(AuthMethod::UserPassword),
            AuthKind::Method(AuthMethod::Pat),
            AuthKind::Method(AuthMethod::ApiToken),
            AuthKind::Method(AuthMethod::OAuth),
        ] {
            assert_eq!(AuthKind::from_db(kind.as_db()), kind);
        }
        assert_eq!(AuthKind::from_db("saml"), AuthKind::None);
    }
}
