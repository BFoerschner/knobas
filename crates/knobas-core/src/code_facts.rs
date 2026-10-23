//! What Code draws beside a mirrored row (#966, #973, #986), read in one
//! statement over the rows Code holds. Moved here from the app's command
//! module by #1870.

use sqlx::{PgPool, Row};

use crate::CoreError;

/// What Code draws beside one mirrored row (#966, #973, #986): a pull
/// request's, branch's or commit's author, whether it is mine, and a
/// repository's language and teams.
///
/// Not on [`EntityRow`]: a room line and a launcher row are one wire shape
/// over two structs (`commands::search`'s pin), read by a dozen statements,
/// and these four facts are Code's alone. Code asks for the rows it holds.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct CodeFacts {
    /// The row asked about.
    pub entity_id: String,
    /// The row's §4.1 author, as its source spells the account: Gitea's
    /// login of a pull request's opener, of a commit's author, of a branch
    /// tip's author. `None` where the source named none.
    pub author: Option<String>,
    /// Whether `author` is its own source's configured username, trimmed,
    /// compared as the attention read compares it (#712). `false` for a
    /// source with no username, whose rows are nobody's.
    pub mine: bool,
    /// A repository's declared language; `None` on a miss (Gitea's `""`).
    pub language: Option<String>,
    /// A repository's declared teams, in the record's order; empty on a miss.
    pub teams: Vec<String>,
}

/// One row per live item among `$1`; `$2` is the declarations.
const CODE_FACTS: &str = concat!(
    r#"
select i.entity_id, i.author, coalesce(i.author = me.me, false) as mine,
       "#,
    crate::declared_string!("$2", "language"),
    r#" as language,
       coalesce((select array_agg(v.value) from "#,
    crate::declared_list!("$2", "teams"),
    r#" v), '{}') as teams
  from sync.live_item i
  left join lateral (select nullif(btrim(s.config->>'username'), '') as me
                       from knobas.source_config s where s.id = i.source_id) me on true
 where i.entity_id = any($1)
"#
);

/// What Code draws beside each of `entity_ids`, read through
/// `declarations`: an id the mirror does not hold live is left out.
///
/// # Errors
///
/// [`CoreError::Db`] for a query failure.
pub async fn read(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    entity_ids: &[String],
) -> Result<Vec<CodeFacts>, CoreError> {
    let rows = sqlx::query(CODE_FACTS)
        .bind(entity_ids)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|row| CodeFacts {
            entity_id: row.get("entity_id"),
            author: row.get("author"),
            mine: row.get("mine"),
            language: row.get("language"),
            teams: row.get("teams"),
        })
        .collect())
}
