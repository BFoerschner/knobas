//! What an id is, in one read for many (the query audit of PR #2479,
//! 2026-11-12): the kind, title and state chip a page draws beside an id it
//! holds, without the payload, body, links, activity and builds the detail
//! read carries.
//!
//! The Week's rows, Standup's linked tickets, the write queue's titles,
//! Docs' reach steps and a job address's candidates each asked `get_entity`
//! once per id for one of these fields: 8 KB a ticket and 19 KB a page on
//! average, up to 179 KB, where the brief is about 120 bytes.

use sqlx::{PgPool, Row};

use crate::CoreError;

/// One id, as a page draws it beside the id: what kind of thing it is, its
/// title and its state chip. The same columns, read by the same rules, as
/// the detail read's row ([`crate::detail`]), so a brief and a detail of
/// one id agree.
#[derive(Debug, Clone, PartialEq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct EntityBrief {
    pub entity_id: String,
    pub kind: String,
    pub source_id: String,
    /// Raw source text. Render it as text, never as markup (gotcha 7).
    pub title: String,
    /// The state chip, by `crate::declared_state!` as every row reads it;
    /// `None` on the ADR-0007 miss.
    pub state: Option<crate::payload::StateChip>,
}

/// One row per id of `$1` the mirror holds, withdrawn or not, as the detail
/// read finds one: a `sync.item` row with its `knobas.entity`. `$2` is the
/// declarations.
const BRIEFS: &str = concat!(
    r#"
select i.entity_id, i.source_id, i.kind, i.title,
       "#,
    crate::declared_state!("$2"),
    r#" as state
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
 where i.entity_id = any($1)
"#
);

/// The brief of each of `entity_ids` the mirror holds, in the order asked; an
/// id it does not hold is left out, and an id asked twice is answered once.
///
/// # Errors
///
/// [`CoreError::Db`] for a query failure, or a state chip that does not
/// decode.
pub async fn read(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    entity_ids: &[String],
) -> Result<Vec<EntityBrief>, CoreError> {
    let rows = sqlx::query(BRIEFS)
        .bind(entity_ids)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?;
    let mut found = std::collections::HashMap::with_capacity(rows.len());
    for row in &rows {
        let brief = EntityBrief {
            entity_id: row.get("entity_id"),
            kind: row.get("kind"),
            source_id: row.get("source_id"),
            title: row.get("title"),
            state: crate::entity_list::state_of(row)?,
        };
        found.insert(brief.entity_id.clone(), brief);
    }
    Ok(entity_ids
        .iter()
        .filter_map(|id| found.remove(id))
        .collect())
}
