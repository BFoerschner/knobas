//! The inbox.

use super::*;

// -- the inbox (issue #45) --------------------------------------------------
//
// Four commands, all here in the existing `entity` module: inbox items are
// derived from entities and its two write commands act on them, and the
// `commands/` + `ipc/` module layout is frozen -- no new module on either
// side. There is deliberately **no fifth command that acts on a source**: an
// inbox action that changes something at a source is a `WriteOp` through
// `submit_write` above, which is #43's command over #42's queue, and the inbox
// introduces no write path of its own.

/// The identity behind `@me`, loaded the way search loads it.
///
/// `knobas_search::Vocabulary` is where "who am I" lives -- the union of every
/// enabled source's configured username -- and the inbox is an identity
/// feature, so it reads that and not a second mechanism. The kind catalog is
/// the default one because the identity is the only field wanted here; loading
/// it costs the same single query every search already makes.
pub(super) async fn identity_of(pool: &PgPool) -> Result<Vec<String>, IpcError> {
    Ok(
        knobas_search::Vocabulary::load(pool, knobas_search::KindCatalog::default())
            .await
            .map_err(IpcError::internal)?
            .identity,
    )
}

/// [`inbox_items`], against a pool and a clock.
///
/// # Errors
///
/// `internal` if the derivation, the identity read or the source listing
/// fails.
pub async fn inbox_items_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
    shelf: knobas_core::inbox::Shelf,
) -> Result<Vec<crate::inbox::InboxEntry>, IpcError> {
    let identity = identity_of(pool).await?;
    crate::inbox::stream(pool, registry, &identity, now, shelf).await
}

/// The inbox: one actionable stream, newest first.
///
/// `shelf` is `stream` -- what needs you now -- or `snoozed`, what you
/// deferred and when it comes back. Both are the same derivation with the same
/// predicate, asked for one shelf or the other.
///
/// Every entry carries the actions its source can really perform: an op the
/// adapter does not declare is absent rather than offered and failing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database or the sync engine is still coming up, `internal` for a read
/// failure.
#[tauri::command]
pub async fn inbox_items(
    lifecycle: State<'_, Lifecycle>,
    shelf: knobas_core::inbox::Shelf,
) -> Result<Vec<crate::inbox::InboxEntry>, IpcError> {
    let pool = lifecycle.pool()?;
    inbox_items_inner(&pool, lifecycle.registry(), Utc::now(), shelf).await
}

/// How many items need you now -- the number the top strip shows.
///
/// **Snoozed items are not in it**, because the number means "needs me now";
/// neither are items marked done. It is the stream's own statement, counted,
/// so the badge cannot disagree with the view it opens.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, `internal` for a read failure.
#[tauri::command]
pub async fn inbox_count(lifecycle: State<'_, Lifecycle>) -> Result<i64, IpcError> {
    let pool = lifecycle.pool()?;
    inbox_count_inner(&pool, lifecycle.registry(), Utc::now()).await
}

/// [`inbox_count`], against a pool and a clock.
///
/// # Errors
///
/// `internal` if the count or the identity read fails.
pub async fn inbox_count_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
) -> Result<i64, IpcError> {
    let identity = identity_of(pool).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    knobas_core::inbox::count(pool, &identity, now, &declarations)
        .await
        .map_err(IpcError::internal)
}

/// [`snooze_inbox_item`], against a pool and a clock.
///
/// # Errors
///
/// As [`snooze_inbox_item`].
pub async fn snooze_inbox_item_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
    item_key: &str,
    until: DateTime<Utc>,
) -> Result<ActivityRow, IpcError> {
    let identity = identity_of(pool).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    crate::inbox::answer(
        pool,
        &identity,
        now,
        item_key,
        crate::inbox::Answer::Snooze(until),
        &declarations,
    )
    .await
}

/// Not now -- come back on this date (#45, stories 13-15).
///
/// `until` is an absolute moment, because the presets (*tomorrow*, *next
/// Monday*, *after the credential expires*) are the caller's arithmetic and a
/// date picker is the general case of the same argument. Snoozing something
/// already snoozed moves its date.
///
/// Refuses an item that is on neither shelf: the key comes from a list the
/// webview has been holding, and an answer to something since resolved at the
/// source would write a durable row about work that no longer exists.
///
/// Writes one activity line and emits `activity:new`, because *every* inbox
/// action is recorded.
///
/// # Errors
///
/// `not_found` for an item on neither shelf,
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// `internal` for a write failure.
#[tauri::command]
pub async fn snooze_inbox_item<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    item_key: String,
    until: DateTime<Utc>,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    let written =
        snooze_inbox_item_inner(&pool, lifecycle.registry(), Utc::now(), &item_key, until).await?;
    announce(&app, written);
    Ok(())
}

/// [`complete_inbox_item`], against a pool and a clock.
///
/// # Errors
///
/// As [`complete_inbox_item`].
pub async fn complete_inbox_item_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
    item_key: &str,
) -> Result<ActivityRow, IpcError> {
    let identity = identity_of(pool).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    crate::inbox::answer(
        pool,
        &identity,
        now,
        item_key,
        crate::inbox::Answer::Done,
        &declarations,
    )
    .await
}

/// I handled this (#45, story 16).
///
/// The item leaves the stream, and **comes back if its subject moves again** --
/// *done* is stored as the moment it was answered and compared against the
/// item's own, so marking a mention done hides it until somebody says
/// something new rather than muting the ticket for ever.
///
/// Writes one activity line and emits `activity:new`.
///
/// # Errors
///
/// As [`snooze_inbox_item`].
#[tauri::command]
pub async fn complete_inbox_item<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    item_key: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    let written =
        complete_inbox_item_inner(&pool, lifecycle.registry(), Utc::now(), &item_key).await?;
    announce(&app, written);
    Ok(())
}

/// Which inbox categories may raise a desktop notification (#290).
///
/// Empty for a profile nobody has switched one on in, which is what makes the
/// feature opt-in per kind (spec #272, story 71).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read
/// fails.
#[tauri::command]
pub async fn notification_kinds(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<knobas_core::inbox::Category>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::inbox::notification_kinds(&pool).await
}

/// Set which inbox categories may notify, and answer with what is now stored.
///
/// Takes the words rather than the enum, the shape [`unlink`] takes its id in
/// and for the same reason: a value Tauri itself cannot deserialize is
/// rejected with a bare string and no [`IpcErrorCode`](crate::IpcErrorCode),
/// so a category this build does not know reads to the settings section as a
/// window that broke rather than as `invalid` with a sentence in it.
///
/// **Nothing here asks about the OS permission.** Requesting it is the
/// settings surface's, on the click that switches the first kind on (story
/// 72); this command records a *preference*, and a preference that could only
/// be stored while some other system said yes would be a setting that
/// silently forgot itself.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for a word that is
/// not an inbox category,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_notification_kinds(
    lifecycle: State<'_, Lifecycle>,
    kinds: Vec<String>,
) -> Result<Vec<knobas_core::inbox::Category>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::inbox::set_notification_kinds(&pool, &kinds).await
}

/// Show a desktop notification whose click comes back as
/// `notification:clicked` (#339).
///
/// A shim over [`crate::notify::Notifier`], where the rules live and are
/// tested: one wait per address, sixteen at once, a click emitted with the
/// address and a clear emitted as nothing. Off the async runtime with
/// `spawn_blocking` because the show blocks until the OS has the desktop
/// notification -- milliseconds, but the wait behind it is a thread of its
/// own and the runtime is not where either belongs.
///
/// Here rather than in a module pair of its own for the reason #290's
/// setting is: the inbox lives in `entity`, and §10.8's layout is frozen.
///
/// # Errors
///
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) with the
/// platform's refusal. On macOS the one to expect is `UNUserNotificationCenter`'s
/// *no bundle identifier* from a bare `tauri dev` binary, logged at `warn` so
/// a dev terminal says why nothing appeared.
#[tauri::command]
pub async fn notify(
    notifier: State<'_, std::sync::Arc<crate::notify::Notifier>>,
    draft: crate::notify::NotificationDraft,
) -> Result<(), IpcError> {
    let notifier = std::sync::Arc::clone(&notifier);
    let address = draft.address.clone();
    let shown = tauri::async_runtime::spawn_blocking(move || notifier.notify(draft))
        .await
        .map_err(|error| {
            IpcError::internal(format!("the desktop notification task ended: {error}"))
        })?;
    match shown {
        Ok(_) => Ok(()),
        Err(error) => {
            tracing::warn!(address, %error, "notification not shown");
            Err(IpcError::internal(error))
        }
    }
}
