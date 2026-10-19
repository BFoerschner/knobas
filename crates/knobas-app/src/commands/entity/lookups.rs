//! What Code draws beside a row, the entity a pasted URL names, and the recent activity lines.

use super::*;

// -- what Code draws beside a row --------------------------------------------

/// What Code draws beside one mirrored row (#966, #973, #986): a pull
/// request's, branch's or commit's author, whether it is mine, and a
/// repository's language and teams.
///
/// Not on [`EntityRow`]: a room line and a launcher row are one wire shape
/// over two structs (`commands::search`'s pin), read by a dozen statements,
/// and these four facts are Code's alone. Code asks for the rows it holds.
#[derive(Debug, Clone, serde::Serialize)]
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
    knobas_core::declared_string!("$2", "language"),
    r#" as language,
       coalesce((select array_agg(v.value) from "#,
    knobas_core::declared_list!("$2", "teams"),
    r#" v), '{}') as teams
  from sync.live_item i
  left join lateral (select nullif(btrim(s.config->>'username'), '') as me
                       from knobas.source_config s where s.id = i.source_id) me on true
 where i.entity_id = any($1)
"#
);

/// [`code_facts`] over a pool, for a test.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn code_facts_inner(
    pool: &PgPool,
    entity_ids: &[String],
) -> Result<Vec<CodeFacts>, IpcError> {
    let declarations = declared_paths(pool).await?;
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

/// What Code draws beside each of `entity_ids` (#966): an id the mirror does
/// not hold live is left out.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn code_facts(
    lifecycle: State<'_, Lifecycle>,
    entity_ids: Vec<String>,
) -> Result<Vec<CodeFacts>, IpcError> {
    let pool = lifecycle.pool()?;
    code_facts_inner(&pool, &entity_ids).await
}

// -- the URL a paste names --------------------------------------------------

/// What a pasted URL resolved to: the entity, and the kind that says where it
/// opens (issue #496).
///
/// Deliberately not an [`EntityRow`] and not an [`EntityDetail`]. A paste
/// answers one question -- *which entity is this link* -- and the frontend
/// then navigates to that entity's own address, at which point the detail is
/// fetched by the read that already exists. Answering with a row here would
/// make every paste pay for a title and a path nothing on the way draws, and
/// would put a second statement in the tree that has to keep in step with
/// `sync.live_item`'s columns.
///
/// `kind` rather than only the id because an entity id names a *namespace*
/// and a key (`gitea:acme/svc#142`), not a kind, and the address an entity
/// opens at is `#/<kind>/<id>` -- or, for an asset or a route, the Tree's own
/// encoder. The frontend has one function that turns the pair into an
/// address; it has nothing that turns an id alone into one.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "test-util", derive(serde::Deserialize))]
pub struct UrlMatch {
    pub entity_id: String,
    pub kind: String,
}

/// The one statement that reads `sync.item.web_url` (#496).
///
/// **`sync.item`, not `sync.live_item`**, and for the two reasons
/// [`DETAIL`] reaches past that view as well: a withdrawn entity still opens
/// and says it is gone (spec #491 story 15 -- *"a stale link explains
/// itself"*), and a disabled source's items still open. It is the **third**
/// reader to do so, and `CONTEXT.md`'s **Live item** entry is the list of
/// them: a fourth owes a reason of its own and a line there. Resolving to a
/// tombstone and letting the detail's banner do the explaining is a better
/// answer than *Not in the mirror*, which would send the reader to the
/// browser to discover the same thing.
///
/// Both sides of the comparison go through `web_url_normalized!` -- the
/// stored column and the pasted parameter -- so there is exactly one rule and
/// it cannot be applied to one side only. `crates/knobas-core/src/web_url.rs`
/// carries the rule and the reasoning; migration `0023` carries the index.
///
/// `where i.web_url is not null` is the index's own predicate, repeated so the
/// planner may use the partial index. `order by i.entity_id limit 1`: nothing
/// stops two mirrored items reporting one address, and an ambiguous paste
/// must answer the same entity on every run rather than whichever row the
/// plan reached first.
pub const RESOLVE_URL: &str = concat!(
    "select i.entity_id, i.kind
       from sync.item i
      where i.web_url is not null
        and ",
    knobas_core::web_url_normalized!("i.web_url"),
    " = ",
    knobas_core::web_url_normalized!("$1::text"),
    "
      order by i.entity_id
      limit 1"
);

/// The entity a URL names, or `None` -- *"a URL in, an entity reference or a
/// miss out"* (spec #491).
///
/// **Never a fetch from a source** (spec #491 story 16): a
/// paste while the tunnel is down is a miss and not a hang, so this is one
/// read on the local mirror and nothing else.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `url` is not an absolute
/// `http`/`https` URL. That is not a miss and must not read as one: a miss is
/// *the mirror does not hold this link*, and offering *Open in browser* on a
/// `file:///` or a half-typed word would offer the reader a refusal
/// (`app/src/lib/shell/open-external.ts` allows those two schemes and no
/// others). The frontend recognises the URL before it calls, so this is a
/// guard rather than a path.
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn resolve_url_inner(pool: &PgPool, url: &str) -> Result<Option<UrlMatch>, IpcError> {
    if !is_web_url(url) {
        return Err(IpcError::invalid(format!(
            "{url:?} is not an http or https URL"
        )));
    }

    let row = sqlx::query(RESOLVE_URL)
        .bind(url)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|row| UrlMatch {
        entity_id: row.get("entity_id"),
        kind: row.get("kind"),
    }))
}

/// Whether `url` is an absolute `http`/`https` URL.
///
/// The scheme is the only part checked, and it is checked
/// case-insensitively because a scheme is case-insensitive and the
/// normalisation down-cases it anyway. Everything past it is the
/// normalisation's business: a URL with no host simply matches no row, which
/// is the miss `web_url.rs` describes rather than a second opinion about what a
/// well-formed URL is.
pub(crate) fn is_web_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// The entity a pasted URL names, or `null` for one the mirror does not hold.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, and whatever [`resolve_url_inner`] refuses
/// with.
#[tauri::command]
pub async fn resolve_url(
    lifecycle: State<'_, Lifecycle>,
    url: String,
) -> Result<Option<UrlMatch>, IpcError> {
    let pool = lifecycle.pool()?;
    resolve_url_inner(&pool, &url).await
}

/// The `limit` most recent activity lines, globally or for one entity.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity` is not an entity id,
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn recent_activity_inner(
    pool: &PgPool,
    limit: u32,
    entity: Option<&EntityRef>,
) -> Result<Vec<ActivityRow>, IpcError> {
    Ok(knobas_core::activity::recent(pool, i64::from(limit), entity).await?)
}

/// The `limit` most recent activity-log lines, newest first.
///
/// `entityId` scopes the read to one entity's history; omitting it is the
/// global stream the status bar reads.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Invalid`](crate::IpcErrorCode::Invalid) for
/// a malformed `entityId`, [`Internal`](crate::IpcErrorCode::Internal) for a
/// query failure.
#[tauri::command]
pub async fn recent_activity(
    lifecycle: State<'_, Lifecycle>,
    limit: u32,
    entity_id: Option<String>,
) -> Result<Vec<ActivityRow>, IpcError> {
    let pool = lifecycle.pool()?;
    let entity = entity_id
        .as_deref()
        .map(EntityRef::parse)
        .transpose()
        .map_err(IpcError::invalid)?;
    recent_activity_inner(&pool, limit, entity.as_ref()).await
}
