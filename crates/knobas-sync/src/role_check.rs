//! The data half of the role check, kept per source (#1199, ADR-0028 *The
//! checks*): whether the records a source's syncs emitted fill each built-in
//! role it claims, and where they missed.
//!
//! The engine feeds a [`DataCheck`] from its sink while a run writes, and
//! [`record`] keeps what it found in `sync.role_check` (migration `0051`),
//! inside the run's transaction. A source's **first sync** is judged; after
//! that the check runs again only while some claimed role has not been
//! judged (its kinds came back empty so far) or stands missed, so that a
//! later sync whose records resolve the path fills the role again. A role
//! once seen filled is not judged again: an incremental run's handful of
//! records is no corpus, and one parentless ticket in it would take the
//! Tickets surface away.
//!
//! A miss stops nothing. The sync commits, its items stay live, searchable
//! and linkable; only the shell's surface for that role leaves the source
//! out, and the health line names the path ([`misses`]).

use std::collections::{BTreeMap, HashMap};

use knobas_source::role::data::{DataCheck, RoleMiss, RoleVerdict};
use sqlx::{PgConnection, PgPool};

/// What is kept of one claimed role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Judged {
    /// Records of every kind the role needs were seen, and none missed.
    Filled,
    /// A declared path missed: the shell does not draw the role.
    Missed(RoleMiss),
}

/// What one run's verdict does to what is kept of its role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Keep,
    Set(Judged),
    /// Back to not judged: the miss went, but not every kind was seen yet.
    Clear,
}

/// The rule, as a function of what was kept and what this run saw.
///
/// * A miss is kept, whatever stood before.
/// * A standing miss goes once a run's records of the kind it named pass,
///   to *filled* if every kind was seen and to *not judged* otherwise.
/// * A role not judged yet is filled once a run sees every kind it needs
///   without a miss.
/// * Otherwise nothing changes: a run that saw nothing of a kind is no
///   evidence about it.
#[must_use]
pub fn change(kept: Option<&Judged>, verdict: &RoleVerdict) -> Change {
    if let Some(miss) = &verdict.miss {
        return match kept {
            Some(Judged::Missed(old)) if old == miss => Change::Keep,
            _ => Change::Set(Judged::Missed(miss.clone())),
        };
    }
    match kept {
        Some(Judged::Missed(old)) if verdict.seen.contains(&old.kind) => {
            if verdict.complete {
                Change::Set(Judged::Filled)
            } else {
                Change::Clear
            }
        }
        None if verdict.complete => Change::Set(Judged::Filled),
        _ => Change::Keep,
    }
}

/// Whether a run must check: some role `check` judges has not been seen
/// filled.
#[must_use]
pub fn wanted(check: &DataCheck, kept: &BTreeMap<String, Judged>) -> bool {
    check
        .roles()
        .any(|role| kept.get(role) != Some(&Judged::Filled))
}

type Row = (String, Option<String>, Option<String>, Option<String>);

fn judged((role, kind, field, path): Row) -> (String, Judged) {
    let judged = match (kind, field, path) {
        (Some(kind), Some(field), Some(path)) => Judged::Missed(RoleMiss {
            role: role.clone(),
            kind,
            field,
            path,
        }),
        // `role_check_miss_whole` keeps the three together.
        _ => Judged::Filled,
    };
    (role, judged)
}

/// What is kept for `source_id`, by role.
///
/// # Errors
///
/// The database's.
pub async fn load(
    conn: &mut PgConnection,
    source_id: &str,
) -> Result<BTreeMap<String, Judged>, sqlx::Error> {
    let rows: Vec<Row> = sqlx::query_as(
        "select role, missed_kind, missed_field, missed_path
           from sync.role_check where source_id = $1",
    )
    .bind(source_id)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(judged).collect())
}

/// Apply one run's verdicts. Writes nothing for a source with no
/// `knobas.source_config` row (a test's or an import's unconfigured run): a
/// verdict is about a configured source.
///
/// # Errors
///
/// The database's.
pub async fn record(
    conn: &mut PgConnection,
    source_id: &str,
    kept: &BTreeMap<String, Judged>,
    verdicts: &[RoleVerdict],
) -> Result<(), sqlx::Error> {
    for verdict in verdicts {
        let miss = match change(kept.get(&verdict.role), verdict) {
            Change::Keep => continue,
            Change::Clear => {
                sqlx::query("delete from sync.role_check where source_id = $1 and role = $2")
                    .bind(source_id)
                    .bind(&verdict.role)
                    .execute(&mut *conn)
                    .await?;
                continue;
            }
            Change::Set(Judged::Filled) => None,
            Change::Set(Judged::Missed(miss)) => Some(miss),
        };
        sqlx::query(
            "insert into sync.role_check
                    (source_id, role, missed_kind, missed_field, missed_path)
             select $1, $2, $3, $4, $5
              where exists (select 1 from knobas.source_config where id = $1)
             on conflict (source_id, role) do update
                set missed_kind = excluded.missed_kind,
                    missed_field = excluded.missed_field,
                    missed_path = excluded.missed_path,
                    checked_at = now()",
        )
        .bind(source_id)
        .bind(&verdict.role)
        .bind(miss.as_ref().map(|m| &m.kind))
        .bind(miss.as_ref().map(|m| &m.field))
        .bind(miss.as_ref().map(|m| &m.path))
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Every standing miss, by source: what the source list carries, and what
/// the shell leaves a source out of a role's surface for.
///
/// # Errors
///
/// The database's.
pub async fn misses(pool: &PgPool) -> Result<HashMap<String, Vec<RoleMiss>>, sqlx::Error> {
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "select source_id, role, missed_kind, missed_field, missed_path
           from sync.role_check
          where missed_path is not null
          order by source_id, role",
    )
    .fetch_all(pool)
    .await?;
    let mut out: HashMap<String, Vec<RoleMiss>> = HashMap::new();
    for (source_id, role, kind, field, path) in rows {
        out.entry(source_id).or_default().push(RoleMiss {
            role,
            kind,
            field,
            path,
        });
    }
    Ok(out)
}
