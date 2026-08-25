//! Add → test → save → re-enter → delete, which is the sources view's whole
//! job (§3), plus the ordering rules interfaces §3 lays down for the keychain.

use std::sync::Arc;

use knobas_app::sources::{self, NewSource, Registry, SecretInput, SourceDraft, SourcePatch};
use knobas_secrets::{MemoryStore, SecretStore};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceDescriptor, SourceError};
use knobas_sync::config::AuthState;
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

/// A registry whose adapters refuse to connect.
///
/// The compiled-in ones cannot: the mock always connects, and driving Jira at a
/// dead port would wait out `knobas_http`'s retry budget and make an app test
/// depend on another stream's timing. Without this the whole *failed*
/// credential path in `set_secret` is unreachable — and a mutation that clears
/// the backoff after a rejected password survived the entire suite because of
/// it. `crud` takes `&dyn AdapterRegistry` so this can exist.
/// A function rather than a value: `SourceError` is not `Clone` (frozen SPI),
/// and each `build` needs its own.
struct RefusingRegistry(fn() -> SourceError);

impl AdapterRegistry for RefusingRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Registry::builtin().descriptors()
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Refusing {
            id: instance.id,
            error: self.0,
        }))
    }
}

struct Refusing {
    id: String,
    error: fn() -> SourceError,
}

#[async_trait::async_trait]
impl Source for Refusing {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            ..knobas_source_mock::descriptor_template()
        }
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Err((self.error)())
    }
    async fn sync(
        &self,
        _cursor: Option<knobas_source::Cursor>,
        _sink: &mut (dyn knobas_source::Sink + Send),
    ) -> Result<knobas_source::Cursor, SourceError> {
        Err((self.error)())
    }
    async fn write(&self, _op: knobas_source::WriteOp) -> Result<(), SourceError> {
        Err(SourceError::Protocol("read-only".into()))
    }
}

struct Fixture {
    pool: PgPool,
    secrets: Arc<dyn SecretStore>,
    registry: Registry,
    id: String,
}

async fn fixture() -> Fixture {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    Fixture {
        pool,
        secrets: Arc::new(MemoryStore::new()),
        registry: Registry::builtin(),
        // `crud-<hex>` -- lowercase, dashes, under 32 characters, which is what
        // `validate_instance_id` allows.
        id: format!("crud-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
    }
}

fn a_new_source(id: &str) -> NewSource {
    NewSource {
        id: id.to_owned(),
        adapter_kind: "mock".to_owned(),
        display_name: "Tidewater Jira".to_owned(),
        base_url: "https://jira.example.invalid".to_owned(),
        auth_kind: AuthMethod::Pat,
        config: serde_json::json!({ "flavor": "datacenter" }),
        secret: SecretInput {
            value: "pat-one".to_owned(),
        },
        sync_interval_secs: 300,
        enabled: true,
    }
}

#[tokio::test]
async fn adding_a_source_writes_the_secret_first_and_returns_a_summary() {
    let f = fixture().await;
    let summary = sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id))
        .await
        .unwrap();

    assert_eq!(summary.id, f.id);
    assert_eq!(summary.adapter_kind, "mock");
    assert_eq!(summary.display_name, "Tidewater Jira");
    assert_eq!(summary.base_url, "https://jira.example.invalid");
    assert_eq!(summary.sync_interval_secs, 300);
    assert!(summary.enabled);
    assert_eq!(summary.config["flavor"], "datacenter");
    assert_eq!(
        summary.health.state,
        AuthState::Unknown,
        "nothing has tested it yet"
    );
    assert!(summary.last_run.is_none());
    assert_eq!(summary.item_count, 0);
    assert!(
        !summary.kinds.is_empty(),
        "the summary carries the adapter's kind metadata"
    );
    assert!(summary.next_run_at.is_some(), "a new enabled source is due");

    assert_eq!(f.secrets.get(&f.id).unwrap().unwrap().value, "pat-one");
    assert_eq!(f.secrets.get(&f.id).unwrap().unwrap().kind, AuthMethod::Pat);
}

/// interfaces §3, Create: `put` the secret, then insert. If the insert fails,
/// delete the secret -- "never the other way round: a config row with no secret
/// is a source that silently 401s".
#[tokio::test]
async fn a_failed_insert_leaves_no_orphaned_keychain_item() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id))
        .await
        .unwrap();

    // Same id again: the primary key refuses it.
    let err = sources::crud::add(&f.pool, &f.secrets, &f.registry, {
        let mut second = a_new_source(&f.id);
        second.secret = SecretInput {
            value: "pat-two".into(),
        };
        second
    })
    .await
    .expect_err("a duplicate id is a conflict");
    assert!(matches!(err, sources::SourcesError::Conflict(_)), "{err:?}");

    assert_eq!(
        f.secrets.get(&f.id).unwrap().unwrap().value,
        "pat-one",
        "the failed attempt must not have overwritten or deleted the live secret"
    );
}

/// The other half of the rule: a *fresh* id whose insert fails leaves nothing
/// in the keychain either. `a_failed_insert_leaves_no_orphaned_keychain_item`
/// only shows the live secret survived a conflict; this shows the rollback
/// happens at all.
///
/// The insert is made to fail by closing the pool under it -- a database
/// failure that is *not* a unique violation, which is exactly the branch that
/// must clean up. A malformed row would not do: everything the form can get
/// wrong is refused by validation before the keychain is touched.
#[tokio::test]
async fn a_failed_insert_for_a_new_id_removes_the_secret_it_just_wrote() {
    let f = fixture().await;
    f.pool.close().await;

    let err = sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id))
        .await
        .expect_err("a closed pool cannot insert");
    assert!(matches!(err, sources::SourcesError::Db(_)), "{err:?}");
    assert!(
        f.secrets.get(&f.id).unwrap().is_none(),
        "the secret written before the failed insert was not removed"
    );
}

#[tokio::test]
async fn an_unknown_adapter_kind_is_refused_before_anything_is_written() {
    let f = fixture().await;
    let mut input = a_new_source(&f.id);
    input.adapter_kind = "nosuch".into();

    let err = sources::crud::add(&f.pool, &f.secrets, &f.registry, input)
        .await
        .unwrap_err();
    assert!(
        matches!(err, sources::SourcesError::UnknownAdapter(_)),
        "{err:?}"
    );
    assert!(
        f.secrets.get(&f.id).unwrap().is_none(),
        "nothing reached the keychain"
    );
    assert!(
        knobas_sync::config::get(&f.pool, &f.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn an_invalid_id_or_interval_is_refused_with_a_reason() {
    let f = fixture().await;
    for bad in ["", "Jira", "jira:eu", "note", "with space", &"a".repeat(33)] {
        let mut input = a_new_source(&f.id);
        input.id = bad.to_owned();
        let err = sources::crud::add(&f.pool, &f.secrets, &f.registry, input)
            .await
            .unwrap_err();
        assert!(
            matches!(err, sources::SourcesError::Invalid(_)),
            "{bad:?} produced {err:?}"
        );
        assert!(
            f.secrets.get(bad).unwrap().is_none(),
            "{bad:?} reached the keychain"
        );
    }
    let mut input = a_new_source(&f.id);
    input.sync_interval_secs = 5;
    assert!(matches!(
        sources::crud::add(&f.pool, &f.secrets, &f.registry, input)
            .await
            .unwrap_err(),
        sources::SourcesError::Invalid(_)
    ));
}

/// interfaces §3, Re-enter: `put` (overwrite) → `test_connection` → write
/// `auth_state` + `auth_checked_at` → clear `backoff_until`.
#[tokio::test]
async fn re_entering_a_secret_overwrites_it_tests_it_and_releases_the_backoff() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id))
        .await
        .unwrap();
    knobas_sync::config::set_health(&f.pool, &f.id, AuthState::Unauthorized, Some("401"), None)
        .await
        .unwrap();
    knobas_sync::config::set_backoff(
        &f.pool,
        &f.id,
        chrono::Utc::now() + chrono::Duration::hours(1),
    )
    .await
    .unwrap();

    let health = sources::crud::set_secret(
        &f.pool,
        &f.secrets,
        &f.registry,
        &f.id,
        SecretInput {
            value: "pat-two".into(),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        health.state,
        AuthState::Ok,
        "the mock connects, so the credential is good"
    );
    assert!(health.checked_at.is_some());
    assert_eq!(
        f.secrets.get(&f.id).unwrap().unwrap().value,
        "pat-two",
        "one item, overwritten"
    );
    let cfg = knobas_sync::config::get(&f.pool, &f.id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        cfg.backoff_until.is_none(),
        "a fresh credential earns an immediate retry"
    );
    assert!(
        knobas_sync::config::due(&f.pool)
            .await
            .unwrap()
            .iter()
            .any(|d| d.id == f.id),
        "and the source is schedulable again"
    );
}

/// interfaces §3, Test (draft): the typed secret is held **in memory only**;
/// nothing is written until Save.
#[tokio::test]
async fn testing_a_draft_writes_nothing_at_all() {
    let f = fixture().await;
    let report = sources::crud::test(
        &f.pool,
        &f.secrets,
        &f.registry,
        SourceDraft {
            source_id: None,
            adapter_kind: "mock".into(),
            base_url: "https://jira.example.invalid".into(),
            auth_kind: AuthMethod::Pat,
            config: serde_json::json!({}),
            secret: Some(SecretInput {
                value: "typed-but-not-saved".into(),
            }),
        },
    )
    .await
    .unwrap();

    assert!(report.ok);
    assert!(report.error.is_none());
    assert!(report.elapsed_ms < 60_000);
    assert!(
        knobas_sync::config::list(&f.pool)
            .await
            .unwrap()
            .iter()
            .all(|c| c.id != f.id)
    );
    assert!(
        f.secrets.get(&f.id).unwrap().is_none(),
        "a draft never reaches the keychain"
    );
    // ...and nothing at all was written under the *draft's* own name either.
    assert!(f.secrets.get("").unwrap().is_none());
    assert!(f.secrets.get("mock").unwrap().is_none());
}

/// A draft for a saved source with `secret: None` re-tests the stored one --
/// which is how *Test connection* works on the sources view without asking the
/// user to retype a PAT.
#[tokio::test]
async fn a_draft_for_a_saved_source_re_tests_the_stored_secret() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id))
        .await
        .unwrap();

    let report = sources::crud::test(
        &f.pool,
        &f.secrets,
        &f.registry,
        SourceDraft {
            source_id: Some(f.id.clone()),
            adapter_kind: "mock".into(),
            base_url: "https://jira.example.invalid".into(),
            auth_kind: AuthMethod::Pat,
            config: serde_json::json!({}),
            secret: None,
        },
    )
    .await
    .unwrap();
    assert!(report.ok);

    // A saved source whose secret has been removed reports that rather than
    // pretending it tested something: `missing_secret` is a different offer
    // from "your credential was rejected" (interfaces §3, "Missing").
    f.secrets.delete(&f.id).unwrap();
    let err = sources::crud::test(
        &f.pool,
        &f.secrets,
        &f.registry,
        SourceDraft {
            source_id: Some(f.id.clone()),
            adapter_kind: "mock".into(),
            base_url: "https://jira.example.invalid".into(),
            auth_kind: AuthMethod::Pat,
            config: serde_json::json!({}),
            secret: None,
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            err,
            sources::SourcesError::Secret(knobas_secrets::SecretError::NotFound)
        ),
        "{err:?}"
    );
}

/// interfaces §3, Delete: config row, then the secret, ignoring absence.
#[tokio::test]
async fn deleting_a_source_removes_its_secret_and_can_purge_its_items() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id))
        .await
        .unwrap();

    sources::crud::delete(&f.pool, &f.secrets, &f.id, false)
        .await
        .unwrap();
    assert!(
        knobas_sync::config::get(&f.pool, &f.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        f.secrets.get(&f.id).unwrap().is_none(),
        "the keychain item goes with it"
    );

    let err = sources::crud::delete(&f.pool, &f.secrets, &f.id, false)
        .await
        .unwrap_err();
    assert!(matches!(err, sources::SourcesError::NotFound(_)), "{err:?}");
}

#[tokio::test]
async fn updating_a_source_cannot_change_its_id_or_its_kind() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id))
        .await
        .unwrap();

    let summary = sources::crud::update(
        &f.pool,
        &f.registry,
        &f.id,
        SourcePatch {
            display_name: Some("Renamed".into()),
            base_url: None,
            config: None,
            sync_interval_secs: Some(600),
            enabled: Some(false),
        },
    )
    .await
    .unwrap();

    assert_eq!(summary.display_name, "Renamed");
    assert_eq!(summary.sync_interval_secs, 600);
    assert!(!summary.enabled);
    assert_eq!(
        summary.base_url, "https://jira.example.invalid",
        "an absent field is left as it was"
    );
    assert_eq!(
        summary.id, f.id,
        "the id is the entity namespace: immutable (P10)"
    );
    assert_eq!(summary.adapter_kind, "mock");
    assert!(
        summary.next_run_at.is_none(),
        "a disabled source has no next run"
    );

    assert!(matches!(
        sources::crud::update(&f.pool, &f.registry, "nope", SourcePatch::default())
            .await
            .unwrap_err(),
        sources::SourcesError::NotFound(_)
    ));
    // An interval below the floor is refused on update too, not only on add.
    assert!(matches!(
        sources::crud::update(
            &f.pool,
            &f.registry,
            &f.id,
            SourcePatch {
                sync_interval_secs: Some(5),
                ..SourcePatch::default()
            },
        )
        .await
        .unwrap_err(),
        sources::SourcesError::Invalid(_)
    ));
}

#[tokio::test]
async fn the_summary_counts_the_items_a_source_actually_mirrors() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id))
        .await
        .unwrap();
    let entity = format!("{}:X-1", f.id);
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 't')")
        .bind(&entity)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload)
         values ($1, $2, 'ticket', 't', '{}'::jsonb)",
    )
    .bind(&entity)
    .bind(&f.id)
    .execute(&f.pool)
    .await
    .unwrap();

    let all = sources::crud::list(&f.pool, &f.registry).await.unwrap();
    let mine = all.iter().find(|s| s.id == f.id).unwrap();
    assert_eq!(mine.item_count, 1);

    // Every other source's count is its own -- a `count(*)` with no grouping
    // would give all of them this one's.
    let other = all.iter().find(|s| s.id != f.id);
    if let Some(other) = other {
        assert_ne!(
            (other.id.as_str(), other.item_count),
            (f.id.as_str(), 1),
            "the counts are per source"
        );
    }
    assert!(
        all.windows(2).all(|w| w[0].id <= w[1].id),
        "id order, so the sources view does not reshuffle between polls"
    );
}

/// The one rule with no exceptions: no path in knobas hands a stored secret
/// back. `SecretInput` also has to redact in `Debug`, or the first
/// `tracing::debug!(?input)` anybody writes puts a PAT in a log file.
#[test]
fn nothing_in_the_ipc_surface_reads_a_secret_back() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/commands/sources.rs"
    ))
    .unwrap();
    assert!(
        !source.contains("secrets.get") && !source.contains("spawn::get"),
        "a command that reads a secret must not exist"
    );

    let shown = format!(
        "{:?}",
        SecretInput {
            value: "hunter2".into()
        }
    );
    assert!(
        !shown.contains("hunter2"),
        "SecretInput leaked in Debug: {shown}"
    );

    // And the summary the sources view *does* get back carries no secret field
    // at all -- checked on the serialized shape, because that is what crosses
    // the bridge.
    let summary = serde_json::to_value(sources::SourceSummary {
        id: "s".into(),
        adapter_kind: "mock".into(),
        display_name: "S".into(),
        base_url: String::new(),
        enabled: true,
        sync_interval_secs: 300,
        config: serde_json::json!({}),
        health: knobas_sync::config::CredentialHealth {
            source_id: "s".into(),
            state: AuthState::Ok,
            checked_at: None,
            detail: None,
            secret_expires_at: None,
        },
        last_run: None,
        next_run_at: None,
        item_count: 0,
        kinds: Vec::new(),
    })
    .unwrap();
    let keys: Vec<&str> = summary
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert!(
        !keys.iter().any(|k| k.contains("secret")),
        "SourceSummary grew a secret-shaped field: {keys:?}"
    );
}

/// **P7, on the path a user actually reaches.** Retyping a password that is
/// still wrong must not release the backoff.
///
/// Clearing it would put the source straight back in front of a system that
/// just refused it, on a timer — which for a Jira DC is how an account earns a
/// CAPTCHA lockout. The credential is still stored (the user asked for that,
/// and a stored-but-rejected secret is what `unauthorized` means), the health
/// says so, and the schedule stays held off.
#[tokio::test]
async fn a_credential_that_is_still_wrong_does_not_release_the_backoff() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id))
        .await
        .unwrap();

    let held_until = chrono::Utc::now() + chrono::Duration::hours(1);
    knobas_sync::config::set_backoff(&f.pool, &f.id, held_until)
        .await
        .unwrap();

    let refusing = RefusingRegistry(|| SourceError::Unauthorized);
    let health = sources::crud::set_secret(
        &f.pool,
        &f.secrets,
        &refusing,
        &f.id,
        SecretInput {
            value: "still-wrong".into(),
        },
    )
    .await
    .unwrap();

    assert_eq!(health.state, AuthState::Unauthorized);
    let cfg = knobas_sync::config::get(&f.pool, &f.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        cfg.backoff_until.map(|t| t.timestamp()),
        Some(held_until.timestamp()),
        "a rejected credential must leave the backoff exactly where it was"
    );
    assert!(
        knobas_sync::config::due(&f.pool)
            .await
            .unwrap()
            .iter()
            .all(|d| d.id != f.id),
        "and the source must not be schedulable again"
    );
    assert_eq!(
        f.secrets.get(&f.id).unwrap().unwrap().value,
        "still-wrong",
        "the user asked for it to be stored; `unauthorized` means stored and rejected"
    );
}

/// The same path with a source that could not be *reached*: the credential is
/// not accused, and the backoff still stands.
#[tokio::test]
async fn an_unreachable_source_does_not_blame_the_credential_or_release_the_backoff() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id))
        .await
        .unwrap();
    let held_until = chrono::Utc::now() + chrono::Duration::hours(1);
    knobas_sync::config::set_backoff(&f.pool, &f.id, held_until)
        .await
        .unwrap();

    let refusing = RefusingRegistry(|| SourceError::Unreachable("dns".to_owned()));
    let health = sources::crud::set_secret(
        &f.pool,
        &f.secrets,
        &refusing,
        &f.id,
        SecretInput {
            value: "probably-fine".into(),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        health.state,
        AuthState::Unreachable,
        "a network failure says nothing about the PAT"
    );
    assert!(
        knobas_sync::config::get(&f.pool, &f.id)
            .await
            .unwrap()
            .unwrap()
            .backoff_until
            .is_some()
    );
}

/// *Test connection* on a **saved** source uses the source's stored
/// configuration, not the draft's.
///
/// The draft describes the form on screen. Building from it would let a green
/// tick stand over a configuration no scheduled run ever attempts — the
/// divergence the widened `auth_kind` opened. Shown by giving the draft an
/// adapter kind that does not exist: if the draft decided, this would be
/// `UnknownAdapter`; because the stored row decides, it succeeds.
#[tokio::test]
async fn testing_a_saved_source_uses_its_stored_configuration() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id))
        .await
        .unwrap();

    let report = sources::crud::test(
        &f.pool,
        &f.secrets,
        &f.registry,
        SourceDraft {
            source_id: Some(f.id.clone()),
            adapter_kind: "nosuch".into(),
            base_url: "https://somewhere-else.invalid".into(),
            auth_kind: AuthMethod::OAuth,
            config: serde_json::json!({ "flavor": "cloud" }),
            secret: None,
        },
    )
    .await
    .expect("the stored row decides, and it names a kind that exists");
    assert!(report.ok);

    // ...and a draft for a source that is not saved at all is refused rather
    // than tested against nothing.
    let gone = format!("gone-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let err = sources::crud::test(
        &f.pool,
        &f.secrets,
        &f.registry,
        SourceDraft {
            source_id: Some(gone.clone()),
            adapter_kind: "mock".into(),
            base_url: String::new(),
            auth_kind: AuthMethod::Pat,
            config: serde_json::json!({}),
            secret: Some(SecretInput { value: "x".into() }),
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&err, sources::SourcesError::NotFound(id) if *id == gone),
        "{err:?}"
    );
}
