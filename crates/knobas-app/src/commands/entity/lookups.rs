//! What Code draws beside a row, the entity a pasted URL names, and the recent activity lines.

use super::*;

// -- what Code draws beside a row --------------------------------------------

pub use knobas_core::code_facts::CodeFacts;

/// [`code_facts`] over a pool, for a test: [`knobas_core::code_facts::read`].
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn code_facts_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    entity_ids: &[String],
) -> Result<Vec<CodeFacts>, IpcError> {
    let declarations = declared_paths(pool, registry).await?;
    Ok(knobas_core::code_facts::read(pool, &declarations, entity_ids).await?)
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
    code_facts_inner(&pool, lifecycle.registry(), &entity_ids).await
}

// -- the URL a paste names --------------------------------------------------

pub use knobas_core::web_url::UrlMatch;

/// `url` read as every form every adapter declares, as `$2` of
/// [`knobas_core::web_url::RESOLVE_URL`]: no adapter is named here, and no
/// form is applied anywhere but in that statement.
pub(crate) fn form_readings(registry: &dyn AdapterRegistry, url: &str) -> Vec<FormReading> {
    registry
        .descriptors()
        .into_iter()
        .flat_map(|descriptor| {
            let adapter_kind = descriptor.adapter_kind;
            descriptor
                .url_forms
                .iter()
                .flat_map(|form| form.read(url))
                .map(|reading| FormReading {
                    adapter_kind: adapter_kind.clone(),
                    reading,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// One element of [`knobas_core::web_url::RESOLVE_URL`]'s `$2`: a reading, and
/// the adapter kind whose sources it may name. Its keys are the statement's
/// `jsonb_to_recordset` columns.
#[derive(Debug, serde::Serialize)]
pub(crate) struct FormReading {
    adapter_kind: String,
    #[serde(flatten)]
    reading: knobas_source::url_forms::Reading,
}

/// The entity a URL names, or `None` -- *"a URL in, an entity reference or a
/// miss out"* (spec #491).
///
/// **Never a fetch from a source** (spec #491 story 16): a
/// paste while the tunnel is down is a miss and not a hang, so this is one
/// read on the local mirror and nothing else. `registry` is where the
/// declared forms come from (#1475); it is read in memory.
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
pub async fn resolve_url_inner(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    url: &str,
) -> Result<Option<UrlMatch>, IpcError> {
    if !is_web_url(url) {
        return Err(IpcError::invalid(format!(
            "{url:?} is not an http or https URL"
        )));
    }

    let forms = form_readings(registry, url);
    Ok(knobas_core::web_url::resolve(pool, url, &forms).await?)
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
    resolve_url_inner(&pool, lifecycle.registry(), &url).await
}
