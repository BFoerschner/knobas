//! The `refused_writes` read's app half (#2040): each ask goes to its
//! entity's own source, the answers come back together, and anything that
//! is not a verdict -- a source whose read failed, an entity no source owns,
//! an answer about a write nobody asked about -- refuses nothing.
//!
//! The adapter is a fake, because what is under test is the routing and not
//! any adapter's reading of its permissions: TeamCity's is witnessed in
//! `knobas-source-teamcity`'s `grants.rs` and, against the real server, in
//! `live_teamcity_seeded.rs`.

use std::sync::{Arc, Mutex};

use knobas_app::sources::{self, NewSource, Registry, SecretInput};
use knobas_secrets::{MemoryStore, SecretStore};
use knobas_source::grants::{RefusedWrite, WriteAsk};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceDescriptor, SourceError};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

/// Every batch of asks a source was given, by source id.
type Asked = Arc<Mutex<Vec<(String, Vec<WriteAsk>)>>>;

struct NarrowRegistry(Asked);

impl AdapterRegistry for NarrowRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Registry::for_process().descriptors()
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Narrow {
            id: instance.id,
            asked: self.0.clone(),
        }))
    }
}

/// Refuses every `rerun_build`, fails its read when asked about `<id>:down`,
/// and also answers a refusal of `<id>:unasked`, which nobody asked about.
struct Narrow {
    id: String,
    asked: Asked,
}

#[async_trait::async_trait]
impl Source for Narrow {
    fn descriptor(&self) -> SourceDescriptor {
        let mut d = Registry::for_process()
            .descriptors()
            .into_iter()
            .find(|d| d.adapter_kind == "mock")
            .expect("the mock is compiled in");
        d.id = self.id.clone();
        d
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }
    async fn sync(
        &self,
        cursor: Option<knobas_source::Cursor>,
        _sink: &mut (dyn knobas_source::Sink + Send),
    ) -> Result<knobas_source::Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }
    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
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
    async fn refused_writes(&self, asks: &[WriteAsk]) -> Result<Vec<RefusedWrite>, SourceError> {
        self.asked
            .lock()
            .unwrap()
            .push((self.id.clone(), asks.to_vec()));
        if asks.iter().any(|a| a.entity.ends_with(":down")) {
            return Err(SourceError::Unreachable("connection refused".into()));
        }
        let mut out: Vec<_> = asks
            .iter()
            .filter(|a| a.op == "rerun_build")
            .map(|a| RefusedWrite::of(a, format!("{} may not re-run", self.id)))
            .collect();
        out.push(RefusedWrite {
            entity: format!("{}:unasked", self.id),
            op: "rerun_build".into(),
            field: None,
            reason: "nobody asked".into(),
        });
        Ok(out)
    }
}

struct Fixture {
    pool: PgPool,
    secrets: Arc<dyn SecretStore>,
    asked: Asked,
}

async fn fixture(ids: &[&str]) -> Fixture {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let asked: Asked = Arc::default();
    let f = Fixture {
        pool,
        secrets: Arc::new(MemoryStore::new()),
        asked,
    };
    for id in ids {
        sources::crud::add(
            &f.pool,
            &f.secrets,
            &NarrowRegistry(f.asked.clone()),
            NewSource {
                id: (*id).to_owned(),
                adapter_kind: "mock".to_owned(),
                display_name: "Narrow".to_owned(),
                base_url: "https://narrow.example.invalid".to_owned(),
                auth_kind: AuthMethod::Pat,
                config: serde_json::json!({}),
                secret: SecretInput::of("pat"),
                sync_interval_secs: 300,
                enabled: true,
            },
        )
        .await
        .unwrap();
    }
    f
}

fn ask(entity: &str, op: &str) -> WriteAsk {
    WriteAsk {
        entity: entity.to_owned(),
        op: op.to_owned(),
        field: None,
    }
}

async fn refused(f: &Fixture, asks: &[WriteAsk]) -> Vec<RefusedWrite> {
    sources::crud::refused_writes(
        &f.pool,
        &f.secrets,
        &crate::support::shared(NarrowRegistry(f.asked.clone())),
        asks,
    )
    .await
}

fn unique(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    )
}

/// Each source is asked once, about its own entities only, and the answers
/// come back together in the order asked.
#[tokio::test]
async fn each_ask_goes_to_its_entitys_source_once_and_the_answers_come_back_together() {
    let (a, b) = (unique("narrow-a"), unique("narrow-b"));
    let f = fixture(&[&a, &b]).await;
    let asks = [
        ask(&format!("{a}:build:1"), "rerun_build"),
        ask(&format!("{b}:build:2"), "rerun_build"),
        ask(&format!("{a}:buildType:X"), "run_build"),
    ];
    let answer = refused(&f, &asks).await;
    assert_eq!(
        answer,
        [
            RefusedWrite::of(&asks[0], format!("{a} may not re-run")),
            RefusedWrite::of(&asks[1], format!("{b} may not re-run")),
        ]
    );
    let mut asked = f.asked.lock().unwrap().clone();
    asked.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(
        asked,
        [
            (a.clone(), vec![asks[0].clone(), asks[2].clone()]),
            (b.clone(), vec![asks[1].clone()]),
        ]
    );
}

/// No verdict refuses nothing: a source whose read fails, an entity no
/// configured source owns, and something that is not an entity id at all
/// each leave their writes as they were drawn, and the others are still
/// answered.
#[tokio::test]
async fn what_is_no_verdict_refuses_nothing_and_the_rest_are_still_answered() {
    let (up, down) = (unique("narrow-up"), unique("narrow-down"));
    let f = fixture(&[&up, &down]).await;
    let asks = [
        ask(&format!("{down}:down"), "rerun_build"),
        ask(&format!("{down}:build:3"), "rerun_build"),
        ask("nobody-configured-this:build:1", "rerun_build"),
        ask("not an entity id", "rerun_build"),
        ask(&format!("{up}:build:4"), "rerun_build"),
    ];
    let answer = refused(&f, &asks).await;
    assert_eq!(
        answer,
        [RefusedWrite::of(&asks[4], format!("{up} may not re-run"))]
    );
}
