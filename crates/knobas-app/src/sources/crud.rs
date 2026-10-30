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

use super::live::LiveSources;
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

/// Refuse a base URL carrying a username or password *before* anything is
/// written (#1640, SPI-10).
///
/// `knobas_http::refuse_userinfo` and nothing local, for [`check_id`]'s
/// reason: it is the check every client built for the source makes, so a URL
/// the form accepts is one every sync can use. Its message does not repeat the
/// URL, so the password goes no further than the form.
fn check_base_url(base_url: &str) -> Result<(), SourcesError> {
    knobas_http::refuse_userinfo(base_url).map_err(|error| {
        // The bare sentence: `to_string` would put `protocol: ` before it
        // in the dialog.
        SourcesError::Invalid(match error {
            knobas_source::SourceError::Protocol { message, .. } => message,
            other => other.to_string(),
        })
    })
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
pub(super) struct Credential {
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
    pub(super) fn keeping(typed: &SecretInput, stored: Option<knobas_secrets::Secret>) -> Self {
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
pub(super) fn instance_from(
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
/// and a *Deny* on the prompt all arrive as `SecretError::Backend`, and the
/// Monitors tab must survive one: it reads the roster out of the database and
/// would otherwise refuse the whole tab over a credential it needs only to
/// decide which two buttons to draw. `submit_write` must survive one too, and
/// it needs more than an empty list: [`instance_offers`] tells it the
/// keychain gave no answer, so a pause pressed while it is locked is queued
/// rather than refused (#1944). `backup`'s
/// `a_keychain_that_refuses_one_source_still_settles_the_others` is the same
/// failure met from the other side: a refusal is this machine learning
/// nothing, and turning that into a per-machine verdict is the bug.
///
/// So it is infallible on purpose: there is no answer this can give that a
/// caller should propagate, and returning a `Result` is how the next caller
/// comes to propagate one. A caller that has to tell "offers nothing" from
/// "the keychain gave no answer" asks [`instance_offers`].
pub async fn instance_write_ops(
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    cfg: &knobas_sync::config::SourceConfigRow,
) -> Vec<String> {
    match instance_offers(secrets, registry, cfg).await {
        InstanceOffers::Ops(ops) => ops,
        InstanceOffers::KeychainSilent => Vec::new(),
    }
}

/// What [`instance_offers`] learned about an instance's write ops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceOffers {
    /// The instance's own `write_ops`, empty for one that has no credential
    /// or whose adapter would not build.
    Ops(Vec<String>),
    /// The keychain would not open, so this machine learned nothing about the
    /// instance (#1944). Not a verdict: the sync waits the same fault out.
    KeychainSilent,
}

/// [`instance_write_ops`], keeping apart the one state that is no answer at
/// all: a keychain fault. A missing credential is an answer (a Kuma with no
/// account offers no pause), and so is an adapter that refuses to build.
///
/// `submit_write` is the caller that needs the difference. A refusal there is
/// a verdict on the write, and a locked keychain gave none (#1944).
pub async fn instance_offers(
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    cfg: &knobas_sync::config::SourceConfigRow,
) -> InstanceOffers {
    let credential = match cfg.auth_kind.method() {
        None => Credential::default(),
        Some(_) => match knobas_secrets::spawn::get(
            secrets,
            &knobas_secrets::KeychainAccount::source(&cfg.id),
        )
        .await
        {
            Ok(Some(stored)) => stored.into(),
            Ok(None) => return InstanceOffers::Ops(Vec::new()),
            Err(_) => return InstanceOffers::KeychainSilent,
        },
    };
    let built = knobas_sync::scheduler::build_off_runtime(
        registry,
        instance_from(
            &cfg.id,
            &cfg.adapter_kind,
            &cfg.display_name,
            &cfg.base_url,
            cfg.auth_kind.method(),
            credential,
            cfg.config.clone(),
        ),
    )
    .await;
    InstanceOffers::Ops(
        built
            .map(|source| source.descriptor().write_ops)
            .unwrap_or_default(),
    )
}

/// Create a source: secret first, then the row.
///
/// **The order is the point** (interfaces §3): if the insert fails the secret
/// is removed again, and never the other way round -- a configuration row with
/// no secret is a source that silently 401s on every schedule, with nothing on
/// screen to say why.
///
/// # Errors
/// [`SourcesError::Invalid`] for a bad id or interval, or a base URL with a
/// username or password in it; [`SourcesError::UnknownAdapter`];
/// [`SourcesError::Conflict`] for a duplicate
/// id; [`SourcesError::Secret`] / [`SourcesError::Db`] for a failing store.
pub async fn add(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &dyn AdapterRegistry,
    input: NewSource,
) -> Result<SourceSummary, SourcesError> {
    check_id(&input.id)?;
    check_base_url(&input.base_url)?;
    let template = template_for(registry, &input.adapter_kind)?;
    let interval =
        config::check_interval(input.sync_interval_secs, template.min_sync_interval_secs)
            .map_err(SourcesError::Invalid)?;
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
    let mut misses = knobas_sync::role_check::misses(pool).await?;
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
        let roles = template
            .as_ref()
            .map(|t| t.roles.clone())
            .unwrap_or_default();
        out.push(SourceSummary {
            role_misses: still_claimed(misses.remove(&id).unwrap_or_default(), &roles),
            roles,
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
/// [`SourcesError::Invalid`] for an interval below the floor or a base URL
/// with a username or password in it, [`SourcesError::NotFound`] if there is
/// no such source.
pub async fn update(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    id: &str,
    patch: SourcePatch,
) -> Result<SourceSummary, SourcesError> {
    if let Some(base_url) = &patch.base_url {
        check_base_url(base_url)?;
    }
    if let Some(secs) = patch.sync_interval_secs {
        // The floor is the stored source's adapter's, so the row is read
        // first. Its kind never changes (P10), so the read cannot go stale
        // before the write below.
        let kind = config::get(pool, id)
            .await?
            .ok_or_else(|| SourcesError::NotFound(id.to_owned()))?
            .adapter_kind;
        let floor = template_for(registry, &kind)?.min_sync_interval_secs;
        config::check_interval(secs, floor).map_err(SourcesError::Invalid)?;
    }
    // Where the source is, or how it is configured: what an adapter that
    // would not build was waiting on, which the write queue records as a
    // wait for a person, as it does a refused credential (#1628).
    let reaches_it_anew = patch.base_url.is_some() || patch.config.is_some();
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
    if reaches_it_anew {
        release_credential_waits(pool, id).await;
    }

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
    live: &LiveSources,
    id: &str,
    purge_items: bool,
) -> Result<(), SourcesError> {
    if !config::delete(pool, id, purge_items).await? {
        return Err(SourcesError::NotFound(id.to_owned()));
    }
    // Dropped once the row is gone, so no read builds it again in between.
    live.forget(id);
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
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
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
    // The live reads' instance holds the credential it was built with
    // (#2240): the next read builds it again over this one.
    live.forget(id);
    release_credential_waits(pool, id).await;

    let source = knobas_sync::scheduler::build_off_runtime(
        registry,
        instance_from(
            id,
            &cfg.adapter_kind,
            &cfg.display_name,
            &cfg.base_url,
            Some(method),
            credential,
            cfg.config.clone(),
        ),
    )
    .await?;
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

/// A person acted on what source `id`'s writes wait for: the writes waiting
/// on its credential are owed again, and the next flush sends them. That is a
/// refused credential (#1628) and a keychain that would not open (#1944):
/// a save is the person having unlocked it, and the write should not sit on
/// its rung for up to an hour after.
///
/// [`set_secret`] calls it for any credential saved -- the secret, the
/// account beside it, or both -- whether or not the test that follows
/// passes: Kuma's test reads with the API key and never logs in with the
/// account its writes use, so a passing test is not the write side's answer,
/// and a failing one leaves the source's health needing a human, which the
/// tick skips anyway. [`update`] calls it for a new address or
/// configuration, because an adapter that would not build waits under the
/// same reason.
///
/// A failure is logged, not raised: the credential is stored by now, and an
/// error here would tell the reader their save failed. The writes stay
/// waiting, and *Flush now* still sends them.
async fn release_credential_waits(pool: &PgPool, id: &str) {
    if let Err(error) = knobas_sync::write_queue::release_credential_waits(pool, id).await {
        tracing::warn!(source = %id, %error, "the write queue kept its credential's writes waiting");
    }
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
        // `Moved` answers a write (#822), never a connection test; it says
        // nothing about the credential either; nor does `Unconfirmed`, which
        // answers a once-sent write (#1125).
        Se::Unreachable(_)
        | Se::Unconfirmed { .. }
        | Se::Protocol { .. }
        | Se::Sink(_)
        | Se::Moved { .. } => AuthState::Unreachable,
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
    registry: &Arc<dyn AdapterRegistry>,
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
    let template = template_for(registry.as_ref(), &kind)?;

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
    let source = knobas_sync::scheduler::build_off_runtime(
        registry,
        instance_from(id, &kind, display_name, &base_url, auth, credential, config),
    )
    .await?;

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
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    entity_id: &str,
) -> Result<Vec<String>, SourcesError> {
    let source = adapter_for(pool, secrets, registry, live, entity_id).await?;
    Ok(source.reachable_transitions(entity_id).await?)
}

/// The writes among `asks` their sources' credentials cannot make, each with
/// its reason (#2040): what a surface that draws a write from the template
/// `write_ops` asks after drawing it, to draw the refused ones disabled.
///
/// Each ask goes to its entity's own source, built once for all of its asks
/// as [`reachable_transitions`] builds it, and the sources are asked side by
/// side. The answers come back in the order asked.
///
/// **Infallible on purpose**, for [`instance_write_ops`]' reason: a surface
/// deciding how to draw a button must not be an error path. Everything that
/// is no verdict refuses nothing, and the write is drawn as before: an
/// entity no configured source owns, a keychain that would not answer, an
/// adapter that would not build, a read the source failed, and an answer
/// about a write nobody asked about.
pub async fn refused_writes(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    asks: &[knobas_source::grants::WriteAsk],
) -> Vec<knobas_source::grants::RefusedWrite> {
    let mut by_source: Vec<(String, Vec<knobas_source::grants::WriteAsk>)> = Vec::new();
    for ask in asks {
        let Ok(parsed) = knobas_core::entity::EntityRef::parse(&ask.entity) else {
            continue;
        };
        match by_source.iter_mut().find(|(id, _)| *id == parsed.namespace) {
            Some((_, of)) => of.push(ask.clone()),
            None => by_source.push((parsed.namespace, vec![ask.clone()])),
        }
    }
    let mut asked = tokio::task::JoinSet::new();
    for (id, of) in by_source {
        let (pool, secrets, registry, live) = (
            pool.clone(),
            secrets.clone(),
            registry.clone(),
            live.clone(),
        );
        let work = async move {
            // Kept where the adapter says the answer moves (#2241): Gitea's
            // probe is asked once per repository, then behind the answer.
            let answer = match live.source(&pool, &secrets, &registry, &of[0].entity).await {
                Ok(lent) => live.refused(&lent, &of).await.map_err(SourcesError::from),
                Err(error) => Err(error),
            };
            answer.unwrap_or_else(|error| {
                tracing::debug!(source = %id, %error, "refused_writes: no verdict");
                Vec::new()
            })
        };
        // A test's request counter counts each source's asks as the call's.
        #[cfg(feature = "test-util")]
        let work = knobas_http::counted::inherit(work);
        asked.spawn(work);
    }
    let answered: Vec<_> = asked.join_all().await.into_iter().flatten().collect();
    asks.iter()
        .filter_map(|ask| answered.iter().find(|r| r.refuses(ask)).cloned())
        .collect()
}

/// The source view of `entity_id` (#661, ADR-0020): the entity's own adapter
/// asked live, the way [`reachable_transitions`] asks it, and nothing stored
/// in the database. A view the adapter says stands at a fixed commit is kept
/// in memory and answered from there the next time (ADR-0032, #2239: see
/// [`LiveSources`]); the app's work below is done on every read.
///
/// **One thing is the app's to do to the answer**, and it is rule 7: an
/// entity-list row keeps its `entity_id` only where the mirror holds that
/// entity. The adapter names what a row *would* be -- a commit's parent, by
/// the id Gitea would give it -- and cannot know what this mirror synced; a
/// row whose address opened *Not in the mirror* would be a link to nowhere.
/// A withdrawn entity is still held (`sync.item` keeps it, and its detail
/// opens), so the check reads `sync.item` and not `sync.live_item`.
///
/// **The second is a row's commit hint** (#665): a row that names a commit by
/// its repository's URL and sha, because the adapter that drew it -- TeamCity,
/// listing a build's changes -- cannot see another source's mirror. The app
/// can: [`commit_page`] composes the commit's page from the two, and the
/// paste resolver's own statement
/// ([`knobas_core::web_url::resolve_stored`]) finds the entity whose page it
/// is. Found, the row gets that entity's id; not found, it stays
/// plain text. Either way the hint is cleared, so it never reaches the shell.
///
/// # Errors
///
/// As [`reachable_transitions`]: the build's failures, and whatever the
/// adapter's read maps to -- which the shell draws as *Showing the mirror*
/// under the generic projection (rule 2). `Ok(None)` is an adapter that draws
/// nothing for this kind, and is not an error.
pub async fn view(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    entity_id: &str,
    section: Option<&str>,
) -> Result<Option<knobas_source::view::SourceView>, SourcesError> {
    let source = live.source(pool, secrets, registry, entity_id).await?;
    let Some(mut view) = live.view(&source, entity_id, section).await? else {
        return Ok(None);
    };
    resolve_commit_hints(pool, registry.as_ref(), &mut view).await?;
    let named: Vec<String> = view
        .sections
        .iter()
        .flat_map(|s| &s.blocks)
        .filter_map(|block| match block {
            knobas_source::view::Block::Entities { rows, .. } => Some(rows),
            _ => None,
        })
        .flatten()
        .filter_map(|row| row.entity_id.clone())
        .collect();
    if named.is_empty() {
        return Ok(Some(view));
    }
    let held: std::collections::HashSet<String> =
        sqlx::query_scalar("select entity_id from sync.item where entity_id = any($1)")
            .bind(&named)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
    for block in view.sections.iter_mut().flat_map(|s| &mut s.blocks) {
        if let knobas_source::view::Block::Entities { rows, .. } = block {
            for row in rows {
                if row.entity_id.as_ref().is_some_and(|id| !held.contains(id)) {
                    row.entity_id = None;
                }
            }
        }
    }
    Ok(Some(view))
}

/// The bytes of the file at `path` in `entity_id`'s tree, at `commit` (a
/// file block's, handed back), read whole up to `cap` (#2256): routed like
/// [`view`], and kept like it where the adapter says the read is fixed. Not
/// touched by the app: no rule applies to bytes.
///
/// # Errors
///
/// As [`view`]. `Ok(None)` is an adapter that reads no bytes.
pub async fn file_bytes(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    entity_id: &str,
    path: &str,
    commit: Option<&str>,
    cap: u64,
) -> Result<Option<Arc<knobas_source::view::FileBytes>>, SourcesError> {
    let source = live.source(pool, secrets, registry, entity_id).await?;
    Ok(live.bytes(&source, entity_id, path, commit, cap).await?)
}

/// A pipeline's settings, read live from its own source (ADR-0022, #851),
/// routed like [`view`].
///
/// **One thing is the app's to do to the answer**: a repository the
/// pipeline builds names where it fetches from, and the adapter cannot see
/// another source's mirror. Where the mirror holds the repository whose page
/// that URL is -- the URL made canonical over the configured clone addresses
/// (#683), its `.git` taken off, and looked up by the paste resolver's own
/// statement -- the repository gets that entity's id, so the header links it.
/// Found nothing, it stays a name.
///
/// # Errors
///
/// As [`view`]. `Ok(None)` is a source that keeps no pipelines.
pub async fn pipeline_settings(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    entity_id: &str,
) -> Result<Option<knobas_source::pipeline::PipelineSettings>, SourcesError> {
    let source = adapter_for(pool, secrets, registry, live, entity_id).await?;
    let Some(mut settings) = source.pipeline_settings(entity_id).await? else {
        return Ok(None);
    };
    if settings.repositories.iter().any(|r| r.url.is_some()) {
        let addresses = clone_addresses(pool, registry.as_ref()).await?;
        for repository in &mut settings.repositories {
            let Some(url) = repository.url.as_deref() else {
                continue;
            };
            let Some(page) = repository_page(&knobas_source::clone_urls::canonical_repo_url(
                url, &addresses,
            )) else {
                continue;
            };
            let found = knobas_core::web_url::resolve_stored(pool, &page).await?;
            repository.entity_id = found
                .filter(|found| found.kind == "repo")
                .map(|found| found.entity_id);
        }
    }
    Ok(Some(settings))
}

/// A saved view's query read into the search grammar by its own source's
/// adapter (#1208, `Source::translate_query`), routed like [`view`].
/// `entity_id` is the saved view's item; `payload` is what its adapter
/// emitted for it.
///
/// # Errors
///
/// As [`view`]. `Ok(None)` is a source that translates no query.
pub async fn translate_query(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    entity_id: &str,
    payload: &serde_json::Value,
) -> Result<Option<knobas_source::query::Translation>, SourcesError> {
    let source = adapter_for(pool, secrets, registry, live, entity_id).await?;
    Ok(source.translate_query(payload).await?)
}

/// The mirrored commit of each of `shas` in the repositories pipeline
/// `entity_id` builds (#1144), for a sha two mirrored repositories share.
///
/// The repositories are the pipeline's settings, read live as
/// [`pipeline_settings`] reads them. Each one's URL is made canonical over the
/// configured clone addresses, and the commit's page is composed from it and
/// the sha the way a build view's commit row is ([`commit_page`], #665), then
/// looked up by the paste resolver's statement. A sha maps only where exactly
/// one mirrored commit answers across the pipeline's repositories. A pipeline
/// with two repositories that both hold the sha maps it to nothing.
///
/// # Errors
///
/// As [`pipeline_settings`].
pub async fn pipeline_commits(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    entity_id: &str,
    shas: &[String],
) -> Result<std::collections::HashMap<String, String>, SourcesError> {
    let mut found = std::collections::HashMap::new();
    let source = adapter_for(pool, secrets, registry, live, entity_id).await?;
    let Some(settings) = source.pipeline_settings(entity_id).await? else {
        return Ok(found);
    };
    let urls: Vec<&str> = settings
        .repositories
        .iter()
        .filter_map(|r| r.url.as_deref())
        .collect();
    if urls.is_empty() {
        return Ok(found);
    }
    let addresses = clone_addresses(pool, registry.as_ref()).await?;
    let repositories: Vec<String> = urls
        .iter()
        .map(|url| knobas_source::clone_urls::canonical_repo_url(url, &addresses))
        .collect();
    for sha in shas {
        let mut hits = std::collections::BTreeSet::new();
        for repository in &repositories {
            let Some(page) = commit_page(repository, sha) else {
                continue;
            };
            if let Some(hit) = knobas_core::web_url::resolve_stored(pool, &page).await?
                && hit.kind == "commit"
            {
                hits.insert(hit.entity_id);
            }
        }
        if hits.len() == 1 {
            found.insert(sha.clone(), hits.pop_first().expect("one hit"));
        }
    }
    Ok(found)
}

/// How long [`shared_commits`] waits for all its pipelines' settings together
/// (#1144). The live read only sharpens a link, and the run page's runs wait
/// on it, so a slow or unreachable source costs at most this before those
/// runs answer without the link. One HTTP call alone may take 45 s
/// (`knobas_http::SEND_BUDGET`), and a settings read makes several.
const COMMIT_READ_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// [`pipeline_commits`] for each pipeline of `shared` (its shas two mirrored
/// commits hold, `knobas_core::builds::shared_shas`), one live read per
/// pipeline, all of them within [`COMMIT_READ_BUDGET`]. A pipeline whose
/// settings cannot be read, or not in time, answers nothing and is logged:
/// its runs then link no commit, which is never a failure of the read that
/// asked.
pub async fn shared_commits<'a>(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    shared: std::collections::BTreeMap<&'a str, Vec<String>>,
) -> knobas_core::builds::ResolvedCommits<'a> {
    let mut resolved = knobas_core::builds::ResolvedCommits::new();
    let deadline = tokio::time::Instant::now() + COMMIT_READ_BUDGET;
    for (pipeline, wanted) in shared {
        let read = pipeline_commits(pool, secrets, registry, live, pipeline, &wanted);
        match tokio::time::timeout_at(deadline, read).await {
            Err(_) => {
                tracing::warn!(pipeline, "a run's repository took too long to read");
            }
            Ok(Ok(found)) => {
                resolved.insert(pipeline, found);
            }
            Ok(Err(error)) => {
                tracing::warn!(pipeline, %error, "a run's repository could not be read");
            }
        }
    }
    resolved
}

/// The runs of pipeline `entity_id` older than run `before`, asked of its own
/// source (ADR-0022, #851), routed like [`view`]. The items are the source's
/// answer as it is: nothing is stored, and the caller reads them through the
/// run kind's declarations.
///
/// # Errors
///
/// As [`view`]. A source that keeps no pipelines answers no runs.
pub async fn older_runs(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    entity_id: &str,
    before: &str,
    count: u32,
) -> Result<Vec<knobas_source::SyncItem>, SourcesError> {
    let source = adapter_for(pool, secrets, registry, live, entity_id).await?;
    Ok(source
        .older_runs(entity_id, before, count)
        .await?
        .unwrap_or_default())
}

/// The chain the run `entity_id` is in, asked of its own source (ADR-0022,
/// #857), routed like [`view`]. Nothing is stored.
///
/// # Errors
///
/// As [`view`]. A source that keeps no pipelines answers no chain.
pub async fn run_chain(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    entity_id: &str,
) -> Result<Vec<knobas_source::pipeline::RunChainStep>, SourcesError> {
    let source = adapter_for(pool, secrets, registry, live, entity_id).await?;
    Ok(source.run_chain(entity_id).await?.unwrap_or_default())
}

/// The live adapter of run `entity_id`'s source, for the run page's reads
/// (ADR-0022; #854, #855, #856), routed like [`view`]: by the id's namespace,
/// with no mirror check, so a run older than the mirror's window reads too.
/// The adapter refuses an id that is not one of its runs.
///
/// # Errors
///
/// As [`view`]'s routing: an id that is not one, a source not configured, a
/// credential the keychain cannot give.
pub async fn run_source(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    entity_id: &str,
) -> Result<Arc<dyn knobas_source::Source>, SourcesError> {
    adapter_for(pool, secrets, registry, live, entity_id).await
}

/// A repository's web page from a clone URL: its `.git` and trailing slash
/// taken off, `None` for a remote that is not `http`/`https` ([`commit_page`]'s
/// rule).
fn repository_page(repo_url: &str) -> Option<String> {
    let repo = repo_url.trim().trim_end_matches('/');
    let repo = repo
        .strip_suffix(".git")
        .unwrap_or(repo)
        .trim_end_matches('/');
    crate::commands::entity::is_web_url(repo).then(|| repo.to_owned())
}

/// Every row's commit hint, resolved to the mirrored commit where the mirror
/// holds its page, and cleared (see [`view`]). One resolver read per hinted
/// row; a build lists a handful of changes.
///
/// A resolved id is the mirror's own, so the rule-7 check after this keeps it.
async fn resolve_commit_hints(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    view: &mut knobas_source::view::SourceView,
) -> Result<(), SourcesError> {
    let hinted = view
        .sections
        .iter()
        .flat_map(|s| &s.blocks)
        .any(|b| matches!(b, knobas_source::view::Block::Entities { rows, .. } if rows.iter().any(|r| r.commit.is_some())));
    if !hinted {
        return Ok(());
    }
    let addresses = clone_addresses(pool, registry).await?;
    for block in view.sections.iter_mut().flat_map(|s| &mut s.blocks) {
        let knobas_source::view::Block::Entities { rows, .. } = block else {
            continue;
        };
        for row in rows {
            let Some(hint) = row.commit.take() else {
                continue;
            };
            if row.entity_id.is_some() {
                continue;
            }
            let repo_url =
                knobas_source::clone_urls::canonical_repo_url(&hint.repo_url, &addresses);
            let Some(page) = commit_page(&repo_url, &hint.sha) else {
                continue;
            };
            let found = knobas_core::web_url::resolve_stored(pool, &page).await?;
            row.entity_id = found.map(|found| found.entity_id);
        }
    }
    Ok(())
}

/// Every configured source's other clone addresses (#683), from the sources
/// whose adapter declares [`knobas_source::clone_urls::CONFIG_KEY`] in its
/// `config_schema`. The template descriptor, as `paths::declared_paths`
/// reads it: whether an adapter keeps the key is a property of its kind. The
/// app reads the key the SPI spells and names no adapter; a source whose
/// adapter does not declare it contributes nothing, whatever its config holds.
async fn clone_addresses(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
) -> Result<Vec<knobas_source::clone_urls::CloneAddress>, SourcesError> {
    let kinds: Vec<String> = registry
        .descriptors()
        .into_iter()
        .filter(|d| knobas_source::clone_urls::declared(&d.config_schema))
        .map(|d| d.adapter_kind)
        .collect();
    if kinds.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        "select s.base_url, a.address
           from knobas.source_config s,
                jsonb_array_elements_text(s.config->$2) as a(address)
          where s.kind = any($1) and jsonb_typeof(s.config->$2) = 'array'
          order by s.id",
    )
    .bind(&kinds)
    .bind(knobas_source::clone_urls::CONFIG_KEY)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(base_url, address)| knobas_source::clone_urls::CloneAddress { base_url, address })
        .collect())
}

/// The web page of commit `sha` in the repository a git remote URL names:
/// `<repository>/commit/<sha>`, the shape Gitea (and GitHub) give a commit's
/// page, from a clone URL with its `.git` and trailing slash taken off.
///
/// `None` for a remote that is not an `http`/`https` URL -- an `ssh://` or
/// scp-like `git@host:path` remote has no web page to look up -- and for a
/// blank sha. A host whose commit pages have another shape simply misses:
/// the row stays plain text, the failure direction ADR-0020 rule 7 asks for.
fn commit_page(repo_url: &str, sha: &str) -> Option<String> {
    let sha = sha.trim();
    let repo = repo_url.trim().trim_end_matches('/');
    let repo = repo
        .strip_suffix(".git")
        .unwrap_or(repo)
        .trim_end_matches('/');
    (crate::commands::entity::is_web_url(repo)
        && !sha.is_empty()
        && sha.chars().all(|c| c.is_ascii_hexdigit()))
    .then(|| format!("{repo}/commit/{sha}"))
}

/// The adapter that owns `entity_id`, as [`LiveSources::source`] lends it:
/// built once per source and reused (#2240).
async fn adapter_for(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    entity_id: &str,
) -> Result<Arc<dyn knobas_source::Source>, SourcesError> {
    Ok(live
        .source(pool, secrets, registry, entity_id)
        .await?
        .source)
}

/// The credential health of every source -- the cheap poll the top strip's
/// monograms read (interfaces §2.2).
///
/// # Errors
/// [`SourcesError::Db`] if the query fails.
pub async fn health(pool: &PgPool) -> Result<Vec<CredentialHealth>, SourcesError> {
    Ok(config::health_all(pool).await?)
}

/// The misses of the roles `roles` still claims: a role the adapter stopped
/// claiming is drawn for nobody, so its old miss says nothing.
fn still_claimed(
    misses: Vec<knobas_source::role::data::RoleMiss>,
    roles: &[knobas_source::role::RoleClaim],
) -> Vec<knobas_source::role::data::RoleMiss> {
    misses
        .into_iter()
        .filter(|miss| roles.iter().any(|claim| claim.role == miss.role))
        .collect()
}

/// What a read asked for a role is scoped to: [`role_scope`]'s answer, which
/// the entity list read takes as its fillers (#1317).
pub use knobas_core::entity_list::RoleScope;

/// The ids of the sources that **fill** `role`, id order.
///
/// A configured source fills a role when its adapter's template claims it
/// and no miss of that role is recorded on its declared paths (ADR-0028,
/// #1199). The registry is the one source of claims: a source whose adapter
/// kind no template answers fills nothing, and so do the items of a source
/// with no configuration row, which no row names. Read per call, the
/// registry and `sync.role_check` both, and cached nowhere, so a sync's new
/// miss or a source just added is in the next read.
///
/// # Errors
///
/// The database's.
pub async fn fillers(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    role: &str,
) -> Result<Vec<String>, sqlx::Error> {
    Ok(role_scope(pool, registry, role).await?.sources)
}

/// A role view's *Sync now* (#2106): one incremental run of each **enabled**
/// source that fills `role`, as [`fillers`] answers it, in id order.
///
/// The surface names a role and never the sources: who fills it is the
/// mirror's answer, read per call. A source already running is joined, not
/// started twice ([`Scheduler::trigger`]'s in-flight rule), so a second click
/// costs nothing; either way the answer names the run, and ADR-0005 says that
/// run comes with an ending on `sync:state`. Every run is `Manual`, which is
/// an incremental sync from the stored cursor.
///
/// A turned-off source is skipped: turning a source off means knobas does not
/// talk to it. A source whose credential needs a human is enabled and is run,
/// as its own page's *Sync now* runs it.
///
/// [`Scheduler::trigger`]: knobas_sync::scheduler::Scheduler::trigger
///
/// # Errors
///
/// [`SourcesError::Db`] from the reads, [`SourcesError::Trigger`] from a
/// trigger (the scheduler shutting down, a filler removed since the read, or
/// the run's record failing).
pub async fn sync_role(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    scheduler: &knobas_sync::scheduler::Scheduler,
    role: &str,
) -> Result<Vec<super::RoleRun>, SourcesError> {
    let fill = fillers(pool, registry, role).await?;
    let enabled: std::collections::HashSet<String> = config::list(pool)
        .await?
        .into_iter()
        .filter(|row| row.enabled)
        .map(|row| row.id)
        .collect();
    let mut runs = Vec::new();
    for source_id in fill.into_iter().filter(|id| enabled.contains(id)) {
        let run_id = scheduler
            .trigger(&source_id, knobas_sync::SyncTrigger::Manual, None)
            .await?;
        runs.push(super::RoleRun { source_id, run_id });
    }
    Ok(runs)
}

/// [`fillers`], with the role's kinds among them.
///
/// A built-in role's kinds are its minimum's, required and optional: each
/// kind it names by id, and each kind a filler declares on a frame it names
/// (CI's runs; a ticket system's saved views), but a kind declared on the
/// pipeline frame, which is the `pipelines` read's (#1344). A custom role's
/// kinds are those its fillers emit that are no built-in role's the source
/// claims, CI's pipelines included. An unknown role has no
/// fillers and no kinds.
///
/// # Errors
///
/// The database's.
pub async fn role_scope(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    role: &str,
) -> Result<RoleScope, sqlx::Error> {
    let rows = config::list(pool).await?;
    // The misses as the source list reads them, so the two cannot disagree.
    let mut misses = knobas_sync::role_check::misses(pool).await?;
    let templates = registry.descriptors();
    let filling: Vec<(String, &SourceDescriptor)> = rows
        .into_iter()
        .filter_map(|row| {
            let template = templates
                .iter()
                .find(|t| t.adapter_kind == row.adapter_kind)?;
            let claims = template.roles.iter().any(|claim| claim.role == role);
            let missed = still_claimed(misses.remove(&row.id).unwrap_or_default(), &template.roles)
                .iter()
                .any(|miss| miss.role == role);
            (claims && !missed).then_some((row.id, template))
        })
        .collect();
    let kinds = role_kinds(role, filling.iter().map(|(_, template)| *template));
    Ok(RoleScope {
        sources: filling.into_iter().map(|(id, _)| id).collect(),
        kinds,
    })
}

/// The kinds of `role` among `fillers`, sorted: a built-in role's own (see
/// [`builtin_kinds`]), or a custom role's, which are the kinds a filler emits
/// that are no built-in role's it claims.
fn role_kinds<'a>(role: &str, fillers: impl Iterator<Item = &'a SourceDescriptor>) -> Vec<String> {
    use knobas_source::role::minimum;
    let builtin_role = minimum(role);
    let mut kinds = std::collections::BTreeSet::new();
    for template in fillers {
        if let Some(wanted) = builtin_role {
            kinds.extend(entity_list_kinds(wanted, template));
            continue;
        }
        let builtin: std::collections::BTreeSet<String> = template
            .roles
            .iter()
            .filter_map(|claim| minimum(&claim.role))
            .flat_map(|other| builtin_kinds(other, template))
            .collect();
        kinds.extend(
            template
                .entity_kinds
                .iter()
                .map(|kind| kind.id.clone())
                .filter(|kind| !builtin.contains(kind)),
        );
    }
    kinds.into_iter().collect()
}

/// The kinds of `min` that the entity list reads for the role: those of
/// [`builtin_kinds`] but a kind declared on the pipeline frame. A pipeline is
/// the `pipelines` read's (narrowed to the same fillers), and CI's rows on
/// the entity list are its runs, which the job's Builds section draws one per
/// pipeline (#1344).
fn entity_list_kinds(
    min: &knobas_source::role::Minimum,
    template: &SourceDescriptor,
) -> Vec<String> {
    let pipelines: Vec<&String> = template
        .payload_paths
        .iter()
        .filter(|paths| paths.frame == Some(knobas_source::Frame::Pipeline))
        .map(|paths| &paths.kind)
        .collect();
    builtin_kinds(min, template)
        .into_iter()
        .filter(|kind| !pipelines.contains(&kind))
        .collect()
}

/// The kinds of `min` that `template` emits or may emit: every kind it names
/// by id, and every kind `template` declares on a frame it names.
fn builtin_kinds(min: &knobas_source::role::Minimum, template: &SourceDescriptor) -> Vec<String> {
    use knobas_source::role::KindOf;
    min.kinds
        .iter()
        .chain(min.optional_kinds)
        .flat_map(|wanted| match wanted.kind {
            KindOf::Id(id) => vec![id.to_owned()],
            KindOf::Frame(frame) => template
                .payload_paths
                .iter()
                .filter(|paths| paths.frame == Some(frame))
                .map(|paths| paths.kind.clone())
                .collect(),
        })
        .collect()
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
        roles: template.roles.clone(),
        role_misses: still_claimed(
            knobas_sync::role_check::misses(pool)
                .await?
                .remove(&id)
                .unwrap_or_default(),
            &template.roles,
        ),
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
