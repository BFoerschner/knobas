//! A password source's run (ADR-0056): its code keys reach the OS keychain
//! only when the run commits, a run that walked every entry replaces them,
//! and its entries' titles stay out of `knobas.entity`.

use std::sync::{Arc, Mutex};

use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::instance::SourceInstance;
use knobas_source::passwords::{
    self, EntryHead, EntryRecord, FolderRecord, Offers, OtpAlgorithm, OtpInfo, OtpKey, SecretText,
};
use knobas_source::{AuthMethod, Source, SourceDescriptor, SourceError};
use knobas_sync::config::{self, AuthKind, InsertConfig};
use knobas_sync::run_log::{self, SyncOutcome, SyncTrigger};
use knobas_sync::scheduler::{AdapterRegistry, RunConnections, RunMode, SchedulerDeps, SyncEvents};

/// What the next run of the vault does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Next {
    /// Two entries with code keys.
    Both,
    /// Hands both keys over, then fails.
    FailAfterKeys,
    /// Only the second entry, with a new key: the first is gone.
    SecondOnly,
    /// An incremental run that saw only the second entry and did not walk
    /// the entries whole: its key joins the stored ones, nothing is dropped.
    Partial,
}

struct Vault {
    id: String,
    next: Next,
}

fn key(secret: &str) -> OtpKey {
    OtpKey {
        secret: SecretText::new(secret),
        digits: 6,
        period: 30,
        algorithm: OtpAlgorithm::Sha1,
    }
}

#[async_trait::async_trait]
impl Source for Vault {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "vault".into(),
            name: "Vault".into(),
            capabilities: vec![],
            adapter_version: "1".into(),
            auth_methods: vec![AuthMethod::UserPassword],
            accepts_account: false,
            min_sync_interval_secs: 60,
            write_ops: vec![],
            entity_kinds: passwords::kind_infos(true),
            config_schema: serde_json::json!({}),
            payload_paths: passwords::payload_paths(),
            roles: vec![knobas_source::role::RoleClaim::new(
                passwords::PASSWORDS,
                [passwords::READ_SECRET],
            )],
            source_actions: vec![],
            clone_from: vec![],
            url_forms: vec![],
            code_pages: None,
            web_sign_in: None,
            by_hand_by_default: true,
            passwords: Some(Offers {
                one_time_codes: true,
                ..Offers::default()
            }),
        }
    }

    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }

    async fn sync(
        &self,
        _cursor: Option<knobas_source::Cursor>,
        sink: &mut (dyn knobas_source::Sink + Send),
    ) -> Result<knobas_source::Cursor, SourceError> {
        let folder = FolderRecord {
            name: "Payments".into(),
            can_add: true,
            ..FolderRecord::default()
        };
        sink.item(passwords::folder_item(&self.id, "f1", &folder))
            .await?;
        let coded = EntryRecord {
            folder: Some(format!("{}:f1", self.id)),
            otp: Some(OtpInfo {
                format: "keepass".into(),
                digits: 6,
                period: 30,
                algorithm: OtpAlgorithm::Sha1,
            }),
            can_read: true,
            ..EntryRecord::default()
        };
        let entries: &[(&str, &str, &str)] = match self.next {
            Next::Both | Next::FailAfterKeys => &[
                ("e1", "payout-service database user", "AAAAAAAAAAAAAAAA"),
                ("e2", "wiki admin", "BBBBBBBBBBBBBBBB"),
            ],
            Next::SecondOnly => &[("e2", "wiki admin", "CCCCCCCCCCCCCCCC")],
            Next::Partial => &[("e2", "wiki admin", "DDDDDDDDDDDDDDDD")],
        };
        for (key_id, title, secret) in entries {
            let head = EntryHead {
                source: self.id.clone(),
                key: (*key_id).to_owned(),
                title: (*title).to_owned(),
                updated_at: None,
            };
            sink.item(passwords::entry_item(head, &coded)).await?;
            sink.otp_key(
                &knobas_source::EntityRef::new(&self.id, key_id),
                key(secret),
            )
            .await;
        }
        if self.next == Next::FailAfterKeys {
            return Err(SourceError::Unreachable("simulated: gone mid-walk".into()));
        }
        if self.next == Next::Partial {
            return Ok("tree".into());
        }
        sink.walked_whole(passwords::ENTRY_KIND).await;
        sink.walked_whole(passwords::FOLDER_KIND).await;
        Ok("tree".into())
    }

    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("no queued writes"))
    }

    async fn reachable_transitions(&self, _entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol("no workflow"))
    }

    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }
}

struct Registry(Mutex<Next>);

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![
            Vault {
                id: "vault".into(),
                next: Next::Both,
            }
            .descriptor(),
        ]
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Vault {
            id: instance.id,
            next: *self.0.lock().unwrap(),
        }))
    }
}

struct Quiet;
impl SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::scheduler::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::config::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

struct Connections(knobas_db::embedded::Connector);

#[async_trait::async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

async fn run(deps: &SchedulerDeps, id: &str) -> SyncOutcome {
    let run_id = run_log::start(&deps.pool, id, SyncTrigger::Manual)
        .await
        .unwrap();
    let result = knobas_sync::scheduler::execute_run(
        deps,
        id,
        run_id,
        RunMode::Incremental,
        None,
        std::time::Instant::now(),
    )
    .await;
    knobas_sync::scheduler::settle(deps, id, run_id, &result).await;
    result.outcome
}

#[tokio::test]
async fn a_runs_code_keys_reach_the_keychain_only_when_it_commits() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = format!("pw-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    config::insert(
        &pool,
        &InsertConfig {
            id: id.clone(),
            adapter_kind: "vault".into(),
            display_name: "Team passwords".into(),
            base_url: String::new(),
            auth_kind: AuthKind::Method(AuthMethod::UserPassword),
            config: serde_json::json!({}),
            sync_interval_secs: 300,
            enabled: true,
            sync_by_hand: true,
        },
    )
    .await
    .unwrap();
    let store = Arc::new(MemoryStore::new());
    store
        .put(
            &knobas_secrets::KeychainAccount::source(&id),
            &Secret::just(AuthMethod::UserPassword, "pw"),
        )
        .unwrap();
    let registry = Arc::new(Registry(Mutex::new(Next::Both)));
    let deps = SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(knobas_db::test_util::test_connector().await)),
        registry: registry.clone(),
        secrets: store.clone(),
        events: Arc::new(Quiet),
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
        quit: knobas_sync::scheduler::Quit::default(),
    };
    let stored = || {
        knobas_secrets::otp::read(store.as_ref(), &id)
            .unwrap()
            .into_iter()
            .map(|(entity, key)| (entity, key.secret.expose().to_owned()))
            .collect::<Vec<_>>()
    };

    assert_eq!(run(&deps, &id).await, SyncOutcome::Ok);
    assert_eq!(
        stored(),
        vec![
            (format!("{id}:e1"), "AAAAAAAAAAAAAAAA".to_owned()),
            (format!("{id}:e2"), "BBBBBBBBBBBBBBBB".to_owned()),
        ]
    );
    // The titles stay in the mirror: `knobas.entity` holds none of them.
    let titles: Vec<(String, String)> = sqlx::query_as(
        "select e.title, i.title from knobas.entity e join sync.item i on i.entity_id = e.id
          where i.source_id = $1 order by e.id",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(titles.len(), 3);
    for (entity_title, mirror_title) in &titles {
        assert_eq!(entity_title, "", "{mirror_title} reached knobas.entity");
        assert!(!mirror_title.is_empty());
    }

    // A run that fails after handing keys over stores none of them.
    *registry.0.lock().unwrap() = Next::FailAfterKeys;
    assert_ne!(run(&deps, &id).await, SyncOutcome::Ok);
    assert_eq!(stored().len(), 2, "the failed run's keys were kept");

    // An incremental run that did not walk the entries whole merges: e1's
    // key stays, e2's is the new one.
    *registry.0.lock().unwrap() = Next::Partial;
    assert_eq!(run(&deps, &id).await, SyncOutcome::Ok);
    assert_eq!(
        stored(),
        vec![
            (format!("{id}:e1"), "AAAAAAAAAAAAAAAA".to_owned()),
            (format!("{id}:e2"), "DDDDDDDDDDDDDDDD".to_owned()),
        ]
    );

    // A run that walked every entry replaces the set: e1 is gone.
    *registry.0.lock().unwrap() = Next::SecondOnly;
    assert_eq!(run(&deps, &id).await, SyncOutcome::Ok);
    assert_eq!(
        stored(),
        vec![(format!("{id}:e2"), "CCCCCCCCCCCCCCCC".to_owned())]
    );
    // And no activity line carries a key or a title.
    let lines: Vec<(serde_json::Value,)> =
        sqlx::query_as("select detail from knobas.activity where actor = $1")
            .bind(format!("sync:{id}"))
            .fetch_all(&pool)
            .await
            .unwrap();
    for (detail,) in lines {
        let text = detail.to_string();
        assert!(
            !text.contains("CCCC") && !text.contains("wiki admin"),
            "{text}"
        );
    }
}

/// A **direct write**'s answer (ADR-0056) reaches the mirror without a run:
/// the item is written under the run's checks, its title stays out of
/// `knobas.entity`, no run is logged, and an item of another source is refused.
#[tokio::test]
async fn a_direct_writes_item_reaches_the_mirror_without_a_run() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = format!("pw-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    config::insert(
        &pool,
        &InsertConfig {
            id: id.clone(),
            adapter_kind: "vault".into(),
            display_name: "Team passwords".into(),
            base_url: String::new(),
            auth_kind: AuthKind::Method(AuthMethod::UserPassword),
            config: serde_json::json!({}),
            sync_interval_secs: 300,
            enabled: true,
            sync_by_hand: true,
        },
    )
    .await
    .unwrap();
    let vault = Vault {
        id: id.clone(),
        next: Next::Both,
    };
    let record = EntryRecord {
        username: Some("grafana_admin".into()),
        can_read: true,
        can_modify: true,
        ..EntryRecord::default()
    };
    let head = |source: &str| EntryHead {
        source: source.to_owned(),
        key: "e9".into(),
        title: "grafana".into(),
        updated_at: None,
    };
    let mut conn = knobas_db::test_util::test_connector()
        .await
        .connect()
        .await
        .unwrap();
    knobas_sync::refresh::write_direct(
        &mut conn,
        &vault,
        passwords::entry_item(head(&id), &record),
    )
    .await
    .unwrap();
    let row: (String, String, serde_json::Value) = sqlx::query_as(
        "select e.title, i.title, i.payload from knobas.entity e join sync.item i on i.entity_id = e.id
          where i.source_id = $1",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "", "the title reached knobas.entity");
    assert_eq!(row.1, "grafana");
    assert_eq!(row.2["username"], "grafana_admin");
    let runs: (i64,) = sqlx::query_as("select count(*) from knobas.sync_run where source_id = $1")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(runs.0, 0, "a direct write started a run");

    let other = knobas_sync::refresh::write_direct(
        &mut conn,
        &vault,
        passwords::entry_item(head("someone-else"), &record),
    )
    .await;
    assert!(other.is_err(), "an item of another source was written");
}
