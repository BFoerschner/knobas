//! Add → test → save → re-enter → delete (§3), with the keychain ordering that
//! keeps a half-created source from existing.
//!
//! Plain functions over a pool and a store, not commands: every decision lives
//! here so it can be tested, and `commands/sources.rs` is decode-call-map.

use std::sync::Arc;

use knobas_secrets::SecretStore;
use knobas_source::SourceDescriptor;
use knobas_source::instance::SourceInstance;
use knobas_sync::config::{self, AuthKind, CredentialHealth, InsertConfig, PatchConfig};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

use super::{
    ConnectionReport, NewSource, SecretInput, SourceDraft, SourcePatch, SourceSummary, SourcesError,
};

/// Reject an id that cannot be an entity namespace *before* anything is
/// written.
///
/// `knobas_source::instance::validate_instance_id` and nothing local: the id is
/// baked into every entity id, link and activity row this source ever writes
/// (P10), and the SPI's contract battery holds adapters to that same function.
/// A second copy of the rule here is how a source passes the form and is
/// refused by every sync.
fn check_id(id: &str) -> Result<(), SourcesError> {
    knobas_source::instance::validate_instance_id(id)
        .map_err(|error| SourcesError::Invalid(error.to_string()))
}

/// The descriptor template for one adapter kind.
///
/// Read off [`AdapterRegistry::descriptors`] rather than off the concrete
/// `Registry`, so that everything in this module works against the trait --
/// which is what makes a *failing* adapter injectable. See the module note on
/// `&dyn AdapterRegistry`.
fn template_for(
    registry: &dyn AdapterRegistry,
    kind: &str,
) -> Result<SourceDescriptor, SourcesError> {
    registry
        .descriptors()
        .into_iter()
        .find(|template| template.adapter_kind == kind)
        .ok_or_else(|| SourcesError::UnknownAdapter(kind.to_owned()))
}

/// What the keychain holds for one source, or what a draft typed: the
/// ordinary secret and the optional account beside it (issue #452).
///
/// A pair rather than two parameters on [`instance_from`], because the two
/// always travel together -- they are one keychain item -- and because a
/// seventh and eighth positional `Option` on that function is exactly how a
/// caller comes to pass the account where the secret goes.
#[derive(Default)]
struct Credential {
    secret: Option<String>,
    account: Option<knobas_source::instance::Account>,
}

impl From<knobas_secrets::Secret> for Credential {
    fn from(stored: knobas_secrets::Secret) -> Self {
        Self {
            secret: Some(stored.value),
            account: stored.account,
        }
    }
}

impl Credential {
    /// **Absent means keep**, for both halves (issue #452), applied once.
    ///
    /// `typed` is what a form submitted and `stored` is what the keychain
    /// holds; the answer is what the source should authenticate with. The rule
    /// reads in both directions and both are met by real readers: adding an
    /// account to an Uptime Kuma cannot retype an API key Kuma showed once, and
    /// replacing an expired key must not throw away the account beside it.
    ///
    /// **One function because it is one rule.** [`set_secret`] resolves it in
    /// order to *store* the answer and [`test`] in order to *try* it, and a
    /// second copy is how a *Test connection* comes to go green over a
    /// credential the saved source does not have.
    ///
    /// The secret may still come back `None` -- a source that needs none, or a
    /// draft for one not yet saved. What a caller may not do is read that as
    /// "clear it"; `set_secret` refuses by name instead.
    fn keeping(typed: &SecretInput, stored: Option<knobas_secrets::Secret>) -> Self {
        let (kept_value, kept_account) = match stored {
            Some(kept) => (Some(kept.value), kept.account),
            None => (None, None),
        };
        Self {
            secret: typed.value.clone().or(kept_value),
            account: typed.account.clone().map(Into::into).or(kept_account),
        }
    }
}

/// The instance an adapter is built from.
///
/// One place, so `crud` and the scheduler route and shape instances
/// identically -- an adapter that behaves differently depending on which of the
/// two built it is a bug nobody would look for.
fn instance_from(
    id: &str,
    kind: &str,
    display_name: &str,
    base_url: &str,
    auth: Option<knobas_source::AuthMethod>,
    credential: Credential,
    config: serde_json::Value,
) -> SourceInstance {
    SourceInstance {
        id: id.to_owned(),
        kind: kind.to_owned(),
        display_name: display_name.to_owned(),
        base_url: base_url.to_owned(),
        auth,
        secret: credential.secret,
        account: credential.account,
        config,
    }
}

/// The write ops **this instance** offers, which is not always what its
/// adapter kind offers (issue #452).
///
/// `list_adapters` serves a descriptor per adapter *kind*, and for every
/// source but one that is the whole answer: a Jira offers what the Jira
/// adapter offers. Uptime Kuma is the exception the SPI always allowed and
/// nothing had yet used -- `Source::descriptor` is a method on a *built*
/// instance -- because whether it can write at all depends on a credential:
/// with only the API key it reads, and with the account beside it it can also
/// pause and resume (spec #427, story 69). So the honest answer needs the
/// keychain, and this is the one place that asks for it.
///
/// **Answers an empty list rather than failing**, whatever went wrong -- a
/// missing credential, a keychain that refused to answer, a configuration the
/// adapter rejects, an adapter kind no longer compiled in. A surface deciding
/// which buttons to draw must not be an error path, and "offers nothing" is
/// the right drawing for every one of those states.
///
/// **The keychain refusal is the one worth naming**, because swallowing it is
/// a decision rather than an oversight. A locked keychain, `errSecAuthFailed`
/// and a *Deny* on the prompt all arrive as `SecretError::Backend`, and both
/// callers of this are surfaces that must survive one: the Monitors tab reads
/// the roster out of the database and would otherwise refuse the whole tab
/// over a credential it needs only to decide which two buttons to draw, and
/// `submit_write` would refuse a Jira *comment* -- an op the adapter *kind*
/// declares, needing no credential to be queueable at all -- because a
/// keychain it never had to open would not open. `backup`'s
/// `a_keychain_that_refuses_one_source_still_settles_the_others` is the same
/// failure met from the other side: a refusal is this machine learning
/// nothing, and turning that into a per-machine verdict is the bug.
///
/// So it is infallible on purpose: there is no answer this can give that a
/// caller should propagate, and returning a `Result` is how the next caller
/// comes to propagate one.
pub async fn instance_write_ops(
    secrets: &Arc<dyn SecretStore>,
    registry: &dyn AdapterRegistry,
    cfg: &knobas_sync::config::SourceConfigRow,
) -> Vec<String> {
    let credential = match cfg.auth_kind.method() {
        None => Credential::default(),
        Some(_) => match knobas_secrets::spawn::get(
            secrets,
            &knobas_secrets::KeychainAccount::source(&cfg.id),
        )
        .await
        {
            Ok(Some(stored)) => stored.into(),
            Ok(None) | Err(_) => return Vec::new(),
        },
    };
    let built = registry.build(instance_from(
        &cfg.id,
        &cfg.adapter_kind,
        &cfg.display_name,
        &cfg.base_url,
        cfg.auth_kind.method(),
        credential,
        cfg.config.clone(),
    ));
    built
        .map(|source| source.descriptor().write_ops)
        .unwrap_or_default()
}

/// Create a source: secret first, then the row.
///
/// **The order is the point** (interfaces §3): if the insert fails the secret
/// is removed again, and never the other way round -- a configuration row with
/// no secret is a source that silently 401s on every schedule, with nothing on
/// screen to say why.
///
/// # Errors
/// [`SourcesError::Invalid`] for a bad id or interval;
/// [`SourcesError::UnknownAdapter`]; [`SourcesError::Conflict`] for a duplicate
/// id; [`SourcesError::Secret`] / [`SourcesError::Db`] for a failing store.
pub async fn add(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &dyn AdapterRegistry,
    input: NewSource,
) -> Result<SourceSummary, SourcesError> {
    check_id(&input.id)?;
    let interval = config::check_interval(input.sync_interval_secs)
        .map_err(|why| SourcesError::Invalid(why.to_owned()))?;
    let template = template_for(registry, &input.adapter_kind)?;
    // **Before the keychain write**, not after. `put` overwrites, so an *Add*
    // that reused a live source's id would replace that source's working
    // credential with the one being rejected -- and then the cleanup below
    // would delete it. Two ways to break a source the user never touched, both
    // from a typo in a form.
    if config::get(pool, &input.id).await?.is_some() {
        return Err(SourcesError::Conflict(format!(
            "a source with id {:?} already exists",
            input.id
        )));
    }

    // The Add-source form always types a credential; `SecretInput::value` is
    // an `Option` for the *re-enter* path, where absent means "keep the one
    // that is stored" -- and there is nothing stored for a source that does
    // not exist yet.
    let typed = input.secret.value.clone().ok_or_else(|| {
        SourcesError::Invalid("a new source needs the credential typed in".to_owned())
    })?;
    knobas_secrets::spawn::put(
        secrets,
        &knobas_secrets::KeychainAccount::source(&input.id),
        knobas_secrets::Secret {
            kind: input.auth_kind,
            value: typed,
            account: input.secret.account.clone().map(Into::into),
        },
    )
    .await?;

    let inserted = config::insert(
        pool,
        &InsertConfig {
            id: input.id.clone(),
            adapter_kind: input.adapter_kind.clone(),
            display_name: input.display_name,
            base_url: input.base_url,
            auth_kind: AuthKind::Method(input.auth_kind),
            config: input.config,
            sync_interval_secs: interval,
            enabled: input.enabled,
        },
    )
    .await;

    let row = match inserted {
        Ok(row) => row,
        Err(error) => {
            let classified = classify_insert(error, &input.id);
            if keeps_the_secret(&classified) {
                tracing::warn!(source_id = %input.id, "another add won the race; keeping its secret");
            } else if let Err(cleanup) = knobas_secrets::spawn::delete(
                secrets,
                &knobas_secrets::KeychainAccount::source(&input.id),
            )
            .await
            {
                tracing::warn!(source_id = %input.id, %cleanup, "could not remove the orphaned secret");
            }
            return Err(classified);
        }
    };
    summarize(pool, row, &template).await
}

/// Whether a failed insert must **leave** the keychain item it just wrote.
///
/// Normally it must not: an item for a source that does not exist is an orphan,
/// and a retry would find it in the way. A [`Conflict`](SourcesError::Conflict)
/// is the exception, and the reason is the ordering rule itself (interfaces
/// §3). 23505 here means another add won the race between the existence check
/// and this insert, so a source with this id now *does* exist -- and the item
/// in the keychain is the one it will authenticate with. Deleting it leaves
/// that source with no credential at all: a config row with no secret, which
/// silently 401s on every schedule with nothing on screen to say why.
///
/// A free function because the branch is otherwise unreachable from a test:
/// the check above makes a real 23505 need two adds interleaving inside one
/// statement's worth of time. The decision is what matters, so the decision is
/// what is pinned -- see `the_conflict_branch_keeps_the_winners_secret`.
fn keeps_the_secret(error: &SourcesError) -> bool {
    matches!(error, SourcesError::Conflict(_))
}

/// A unique-violation on the primary key is a duplicate id, which is a
/// conflict the form can explain; anything else is a database failure.
fn classify_insert(error: sqlx::Error, id: &str) -> SourcesError {
    let unique = error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "23505");
    if unique {
        SourcesError::Conflict(format!("a source with id {id:?} already exists"))
    } else {
        SourcesError::Db(error)
    }
}

/// Every configured source, id order -- one row of the sources view each.
///
/// # Errors
/// [`SourcesError::Db`] if any read fails.
pub async fn list(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
) -> Result<Vec<SourceSummary>, SourcesError> {
    let rows = config::list(pool).await?;
    let counts = item_counts(pool).await?;
    let statuses = knobas_sync::scheduler::status_all(pool).await?;
    // Once, not once per row: `descriptors()` builds every template.
    let templates = registry.descriptors();

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        // A row whose adapter is no longer compiled in still appears -- the
        // user has to be able to see it in order to delete it. It simply
        // declares no kinds.
        let template = templates
            .iter()
            .find(|t| t.adapter_kind == row.adapter_kind)
            .cloned();
        let id = row.id.clone();
        out.push(SourceSummary {
            item_count: counts.get(&id).copied().unwrap_or(0),
            next_run_at: statuses
                .iter()
                .find(|s| s.source_id == id)
                .and_then(|s| s.next_run_at),
            last_run: knobas_sync::run_log::last_finished(pool, &id).await?,
            kinds: template.map(|t| t.entity_kinds).unwrap_or_default(),
            auth_kind: row.auth_kind.method(),
            id: row.id,
            adapter_kind: row.adapter_kind,
            display_name: row.display_name,
            base_url: row.base_url,
            enabled: row.enabled,
            sync_interval_secs: row.sync_interval_secs,
            config: row.config,
            health: row.health,
        });
    }
    Ok(out)
}

/// How many mirror rows each source holds.
///
/// One grouped statement rather than a count per row: it is the only read here
/// that would otherwise scale with the number of sources *and* scan the mirror.
async fn item_counts(pool: &PgPool) -> Result<std::collections::HashMap<String, i64>, sqlx::Error> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as("select source_id, count(*) from sync.item group by source_id")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().collect())
}

/// Apply an edit. `None` fields are left as they are.
///
/// # Errors
/// [`SourcesError::Invalid`] for an interval below the floor,
/// [`SourcesError::NotFound`] if there is no such source.
pub async fn update(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    id: &str,
    patch: SourcePatch,
) -> Result<SourceSummary, SourcesError> {
    if let Some(secs) = patch.sync_interval_secs {
        config::check_interval(secs).map_err(|why| SourcesError::Invalid(why.to_owned()))?;
    }
    let row = config::patch(
        pool,
        id,
        &PatchConfig {
            display_name: patch.display_name,
            base_url: patch.base_url,
            config: patch.config,
            sync_interval_secs: patch.sync_interval_secs,
            enabled: patch.enabled,
        },
    )
    .await?
    .ok_or_else(|| SourcesError::NotFound(id.to_owned()))?;

    let template = template_for(registry, &row.adapter_kind)?;
    summarize(pool, row, &template).await
}

/// Remove a source: the configuration row first, then its keychain item.
///
/// That order and not the other one: a keychain item with no configuration is
/// an orphan nothing will ever read, while a configuration with no secret is a
/// source that 401s on every schedule with nothing on screen to say why. The
/// keychain delete treats absence as success, so a half-deleted source can
/// always be finished off.
///
/// # Errors
/// [`SourcesError::NotFound`] if there was no such source.
pub async fn delete(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    id: &str,
    purge_items: bool,
) -> Result<(), SourcesError> {
    if !config::delete(pool, id, purge_items).await? {
        return Err(SourcesError::NotFound(id.to_owned()));
    }
    knobas_secrets::spawn::delete(secrets, &knobas_secrets::KeychainAccount::source(id)).await?;
    Ok(())
}

/// Re-enter a credential: overwrite it, test it, record the verdict, release
/// the backoff.
///
/// The order is interfaces §3's. Testing *before* storing would leave the user
/// having retyped a working PAT that nothing kept; clearing the backoff before
/// testing would schedule a run for a credential that is still wrong.
///
/// # Errors
/// [`SourcesError::NotFound`], [`SourcesError::UnknownAdapter`],
/// [`SourcesError::Secret`], [`SourcesError::Source`] if the adapter refuses
/// the configuration outright.
pub async fn set_secret(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &dyn AdapterRegistry,
    id: &str,
    secret: SecretInput,
) -> Result<CredentialHealth, SourcesError> {
    let cfg = config::get(pool, id)
        .await?
        .ok_or_else(|| SourcesError::NotFound(id.to_owned()))?;
    let method = cfg
        .auth_kind
        .method()
        .ok_or_else(|| SourcesError::Invalid(format!("source {id:?} needs no credential")))?;

    // **Absent means keep, for both halves** (issue #452). The form submits
    // what the reader typed, and a reader adding an account to a Kuma has no
    // way to retype an API key Kuma showed them once -- story 69's "adding the
    // account flips both without re-entering the key" is that sentence. The
    // other direction is the same rule read the other way: re-entering an
    // expired key must not silently throw away the account beside it, which is
    // what "absent means clear" would do to the one credential the form cannot
    // show. Removing an account is therefore not something this call can do;
    // deleting and re-adding the source is, and it is the honest cost of a
    // form that may never read a credential back.
    let credential = Credential::keeping(
        &secret,
        knobas_secrets::spawn::get(secrets, &knobas_secrets::KeychainAccount::source(id)).await?,
    );
    let Some(value) = credential.secret.clone() else {
        return Err(SourcesError::Invalid(format!(
            "source {id:?} has no stored credential to keep -- type one in"
        )));
    };

    knobas_secrets::spawn::put(
        secrets,
        &knobas_secrets::KeychainAccount::source(id),
        knobas_secrets::Secret {
            kind: method,
            value,
            account: credential.account.clone(),
        },
    )
    .await?;

    let source = registry.build(instance_from(
        id,
        &cfg.adapter_kind,
        &cfg.display_name,
        &cfg.base_url,
        Some(method),
        credential,
        cfg.config.clone(),
    ))?;
    let outcome = source.test_connection().await;

    // A good check records **no** detail. What the adapter had to say about
    // the far end is its connection note, which belongs to the *Test
    // connection* result (`ConnectionReport::detail`) and is never stored;
    // credential health carries what the last check said went wrong, and a
    // check that went right has nothing to put there (#326). The failure arm
    // keeps the error text, which is that column's own business.
    let (state, detail, expires) = match &outcome {
        Ok(info) => (
            knobas_sync::config::AuthState::Ok,
            None,
            info.secret_expires_at,
        ),
        Err(error) => (auth_state_of(error), Some(error.to_string()), None),
    };
    let health = config::set_health(pool, id, state, detail.as_deref(), expires)
        .await?
        .ok_or_else(|| SourcesError::NotFound(id.to_owned()))?
        .0;

    if releases_the_backoff(state) {
        config::clear_backoff(pool, id).await?;
    }
    Ok(health)
}

/// Whether a re-entered credential earns an immediate retry.
///
/// Only one that *worked*. Releasing the backoff after a failed test would put
/// the source straight back in front of a remote system that just refused it --
/// and for `unauthorized` that is P7's whole point: repeated rejected
/// credentials are how a Jira DC account earns a CAPTCHA lockout.
///
/// A free function, and pinned as one, because the compiled-in adapters cannot
/// reach the failing branch from a test: the mock always connects, and driving
/// Jira at a dead port would make this test wait out `knobas_http`'s retry
/// budget and depend on another stream's timing. The decision is what matters
/// and the decision is what is asserted.
fn releases_the_backoff(state: knobas_sync::config::AuthState) -> bool {
    state == knobas_sync::config::AuthState::Ok
}

/// How a failed `test_connection` reads in the sources view.
///
/// `Protocol` and `Sink` say nothing about the credential, so they leave the
/// state at `unreachable` rather than accusing a PAT that may be fine --
/// "knobas could not talk to it" is the honest sentence for a dialect mismatch.
fn auth_state_of(error: &knobas_source::SourceError) -> knobas_sync::config::AuthState {
    use knobas_source::SourceError as Se;
    use knobas_sync::config::AuthState;
    match error {
        Se::Unauthorized { .. } => AuthState::Unauthorized,
        Se::Unreachable(_) | Se::Protocol { .. } | Se::Sink(_) => AuthState::Unreachable,
    }
}

/// Test a connection. **Writes nothing** -- not to Postgres, not to the
/// keychain.
///
/// The typed secret in a draft lives in memory for the length of this call;
/// only *Save* stores one (interfaces §3, "Test (draft)"). A draft naming a
/// saved source with no typed secret re-tests the stored one, which is how the
/// sources view offers *Test connection* without asking for a PAT again.
///
/// # Errors
/// [`SourcesError::UnknownAdapter`], [`SourcesError::Secret`] when a saved
/// source's credential is gone, [`SourcesError::Source`] if the adapter refuses
/// the configuration. A *failed connection* is not an error: it is a
/// [`ConnectionReport`] with `ok: false`, because the form has to render it.
pub async fn test(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &dyn AdapterRegistry,
    draft: SourceDraft,
) -> Result<ConnectionReport, SourcesError> {
    // **A saved source is tested against what it is saved as.** The draft's
    // `base_url`, `auth_kind`, `config` and `adapter_kind` describe the form on
    // screen; for a source that already exists, the scheduled run will use the
    // stored row. Testing the form instead would let *Test connection* pass
    // against a configuration no run ever attempts -- a green tick over a
    // source that 401s every five minutes.
    let saved = match &draft.source_id {
        Some(id) => Some(
            config::get(pool, id)
                .await?
                .ok_or_else(|| SourcesError::NotFound(id.clone()))?,
        ),
        None => None,
    };
    let (kind, base_url, auth, config) = match &saved {
        Some(row) => (
            row.adapter_kind.clone(),
            row.base_url.clone(),
            row.auth_kind.method(),
            row.config.clone(),
        ),
        None => (
            draft.adapter_kind.clone(),
            draft.base_url.clone(),
            Some(draft.auth_kind),
            draft.config.clone(),
        ),
    };
    let template = template_for(registry, &kind)?;

    // **The same rule `set_secret` applies**, through the same function: a
    // typed half wins and an absent one keeps what is stored (issue #452). A
    // typed secret is the point of *Test* before *Save* -- it is held in memory
    // for this call and written nowhere -- and a saved source with nothing
    // typed is tested with the credential its scheduled run will use, account
    // included. A *Test* that resolved this differently would go green over a
    // credential the saved source does not have.
    let stored = match &draft.source_id {
        Some(id) if auth.is_some() => {
            knobas_secrets::spawn::get(secrets, &knobas_secrets::KeychainAccount::source(id))
                .await?
        }
        _ => None,
    };
    let had_stored = stored.is_some();
    let credential = Credential::keeping(&draft.secret.clone().unwrap_or_default(), stored);
    // Absent is an error rather than an anonymous attempt for a **saved**
    // source: "you never entered one" is the `missing_secret` offer, not a 401.
    if credential.secret.is_none() && !had_stored && draft.source_id.is_some() && auth.is_some() {
        return Err(knobas_secrets::SecretError::NotFound.into());
    }

    // The instance id only matters to an adapter that emits items; nothing here
    // does. A draft for a saved source uses its real id all the same, so an
    // adapter that validates it sees what it will see in production.
    let id = draft.source_id.as_deref().unwrap_or(&template.adapter_kind);
    let display_name = saved
        .as_ref()
        .map_or(kind.as_str(), |row| &row.display_name);
    let source = registry.build(instance_from(
        id,
        &kind,
        display_name,
        &base_url,
        auth,
        credential,
        config,
    ))?;

    let started = std::time::Instant::now();
    let outcome = source.test_connection().await;
    let elapsed_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);

    Ok(match outcome {
        Ok(info) => ConnectionReport {
            ok: true,
            account: info.account,
            server_version: info.server_version,
            secret_expires_at: info.secret_expires_at,
            error: None,
            code: None,
            elapsed_ms,
            detail: info.detail,
            discovered: info.discovered,
        },
        Err(error) => ConnectionReport {
            ok: false,
            account: None,
            server_version: None,
            secret_expires_at: None,
            error: Some(error.to_string()),
            code: Some(crate::IpcError::from_source_error(&error, draft.source_id.as_deref()).code),
            elapsed_ms,
            // A test that did not connect has no note about the far end;
            // `error` is its one line.
            detail: None,
            // A test that failed learned nothing worth filling a form with.
            discovered: std::collections::BTreeMap::new(),
        },
    })
}

/// What this entity's workflow offers from where it stands right now (#498).
///
/// The **source is the entity's namespace** and is not asked for, on
/// `write_queue::submit`'s reasoning: interfaces §4.1 makes the instance id and
/// the `EntityRef` namespace one string, so a second argument could only agree
/// or contradict, and a contradiction would ask one source about another's
/// ticket.
///
/// **Nothing is stored and nothing is cached.** A *reachable transition*
/// (`CONTEXT.md`) is an answer about now: it is read when a detail opens, it is
/// offered, and it is gone. A cache would be knobas holding a copy of a
/// workflow's state, which is the thing the glossary entry rules out and which
/// `WriteOp::Transition`'s write-time resolution exists because knobas cannot
/// have.
///
/// **In `crud` and not in a module of its own**, because the part that must not
/// drift is already extracted: [`instance_from`] and [`Credential::keeping`]
/// are what make the adapter built here identical to the one `set_secret`,
/// `test` and a scheduled run build, and both are private to this module. What
/// is *not* shared is the reading around them, and deliberately -- `set_secret`
/// stores a typed credential first and records the verdict after, `test`
/// resolves a draft against a saved row, and this one reads a stored row and
/// nothing else. Lifting those into one function would need a parameter per
/// difference; lifting this one out of the module would need both helpers
/// widened, for one caller.
///
/// # Errors
///
/// [`SourcesError::NotFound`] for an entity whose namespace is not a
/// configured source, [`SourcesError::Invalid`] for something that is not an
/// entity id at all, [`SourcesError::Secret`] when the keychain will not
/// answer, [`SourcesError::UnknownAdapter`] and [`SourcesError::Source`] from
/// the build -- and, the ordinary case, whatever the adapter's own read maps
/// to: a source with no workflow refuses with
/// [`SourceError::Protocol`](knobas_source::SourceError::Protocol), and a dead
/// credential is
/// [`Unauthorized`](knobas_source::SourceError::Unauthorized). Every one of
/// them reaches the shell as a rejection, and the select falls back to the
/// corpus-observed offer rather than showing an empty list.
pub async fn reachable_transitions(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &dyn AdapterRegistry,
    entity_id: &str,
) -> Result<Vec<String>, SourcesError> {
    let parsed = knobas_core::entity::EntityRef::parse(entity_id)
        .map_err(|error| SourcesError::Invalid(error.to_string()))?;
    let cfg = config::get(pool, &parsed.namespace)
        .await?
        .ok_or_else(|| SourcesError::NotFound(parsed.namespace.clone()))?;
    let method = cfg.auth_kind.method();
    // The keychain is asked only where the configuration says a credential is
    // needed at all, which is `build_source`'s rule: a source that
    // authenticates nothing -- the mock, `--demo` against an empty keychain --
    // must not be turned into a keychain prompt by a select opening.
    let stored = match method {
        None => None,
        Some(_) => {
            knobas_secrets::spawn::get(
                secrets,
                &knobas_secrets::KeychainAccount::source(&parsed.namespace),
            )
            .await?
        }
    };
    let source = registry.build(instance_from(
        &parsed.namespace,
        &cfg.adapter_kind,
        &cfg.display_name,
        &cfg.base_url,
        method,
        Credential::keeping(&SecretInput::default(), stored),
        cfg.config.clone(),
    ))?;
    Ok(source.reachable_transitions(entity_id).await?)
}

/// The credential health of every source -- the cheap poll the top strip's
/// monograms read (interfaces §2.2).
///
/// # Errors
/// [`SourcesError::Db`] if the query fails.
pub async fn health(pool: &PgPool) -> Result<Vec<CredentialHealth>, SourcesError> {
    Ok(config::health_all(pool).await?)
}

async fn summarize(
    pool: &PgPool,
    row: config::SourceConfigRow,
    template: &SourceDescriptor,
) -> Result<SourceSummary, SourcesError> {
    let id = row.id.clone();
    let (count,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = $1")
        .bind(&id)
        .fetch_one(pool)
        .await?;
    Ok(SourceSummary {
        item_count: count,
        next_run_at: knobas_sync::scheduler::status_for(pool, &id)
            .await?
            .and_then(|s| s.next_run_at),
        last_run: knobas_sync::run_log::last_finished(pool, &id).await?,
        kinds: template.entity_kinds.clone(),
        auth_kind: row.auth_kind.method(),
        id: row.id,
        adapter_kind: row.adapter_kind,
        display_name: row.display_name,
        base_url: row.base_url,
        enabled: row.enabled,
        sync_interval_secs: row.sync_interval_secs,
        config: row.config,
        health: row.health,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one branch in [`add`]'s failure path that a race decides.
    #[test]
    fn the_conflict_branch_keeps_the_winners_secret() {
        assert!(
            keeps_the_secret(&SourcesError::Conflict("taken".to_owned())),
            "the racer that lost must not delete the winner's credential"
        );
        for other in [
            SourcesError::Db(sqlx::Error::PoolClosed),
            SourcesError::Invalid("bad".to_owned()),
            SourcesError::NotFound("gone".to_owned()),
            SourcesError::UnknownAdapter("nope".to_owned()),
        ] {
            assert!(
                !keeps_the_secret(&other),
                "{other:?} leaves an item nothing owns"
            );
        }
    }

    /// A failed *Test connection* must not release the source's backoff, and
    /// the two states that need a human must not be reached by mistake either.
    #[test]
    fn only_a_working_credential_releases_the_backoff() {
        use knobas_sync::config::AuthState;
        assert!(releases_the_backoff(AuthState::Ok));
        for refused in [
            AuthState::Unauthorized,
            AuthState::Unreachable,
            AuthState::MissingSecret,
            AuthState::Unknown,
        ] {
            assert!(
                !releases_the_backoff(refused),
                "{refused:?} must not put the source straight back in front of \
                 the system that just refused it"
            );
        }
    }

    /// A protocol or sink failure says nothing about the credential.
    ///
    /// Calling a dialect mismatch `unauthorized` would put *Re-enter password*
    /// on a row whose PAT is fine, and send the user round a loop that cannot
    /// end.
    #[test]
    fn a_failed_test_only_blames_the_credential_when_the_source_did() {
        use knobas_source::SourceError as Se;
        use knobas_sync::config::AuthState;
        assert_eq!(auth_state_of(&Se::unauthorized()), AuthState::Unauthorized);
        assert_eq!(
            auth_state_of(&Se::Unreachable("dns".to_owned())),
            AuthState::Unreachable
        );
        assert_eq!(
            auth_state_of(&Se::protocol("bad json")),
            AuthState::Unreachable
        );
        assert_eq!(
            auth_state_of(&Se::Sink("disk full".to_owned())),
            AuthState::Unreachable
        );
    }

    /// A unique violation is what `Conflict` is made of, and nothing else is.
    #[test]
    fn only_a_unique_violation_reads_as_a_duplicate_id() {
        assert!(matches!(
            classify_insert(sqlx::Error::PoolClosed, "x"),
            SourcesError::Db(_)
        ));
        assert!(matches!(
            classify_insert(sqlx::Error::RowNotFound, "x"),
            SourcesError::Db(_)
        ));
    }
}
