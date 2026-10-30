//! **Live reads, kept cheap** (#2239, #2240; ADR-0032), at the app's seam:
//! `sources::crud`'s reads and writes over a [`SourcesState`]'s
//! [`LiveSources`].
//!
//! * One source per source (#2240): ten reads build it once and ask the
//!   keychain once; a new credential, an edited address and a removal are
//!   each seen by the next read.
//! * The read cache (#2239): a view the adapter says is fixed is read once
//!   per entity and section; a live one every time; the cache drops the
//!   least recently used past its bound; nothing it holds reaches the
//!   database.
//!
//! The adapter is a fake that answers what it was built with and counts what
//! it is asked, because what is under test is the app's keeping of sources
//! and views, not an adapter's drawing: against the real Gitea, the read
//! cache's round trips are `tests/read_cache_live.rs`'s.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use knobas_app::sources::live::LiveSources;
use knobas_app::sources::{self, NewSource, Registry, SecretInput, SourcePatch, SourcesError};
use knobas_secrets::{KeychainAccount, MemoryStore, Secret, SecretError, SecretStore};
use knobas_source::instance::SourceInstance;
use knobas_source::view::{Block, SourceView, Standing, ViewSection};
use knobas_source::{AuthMethod, Source, SourceDescriptor, SourceError};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

/// What the fake saw: builds, and every `(entity, section)` read.
#[derive(Default)]
struct Seen {
    builds: AtomicUsize,
    reads: Mutex<Vec<(String, Option<String>)>>,
    /// What a moving key reads as: bumped by a test, as a push moves a branch.
    version: AtomicUsize,
    /// Every read fails while set.
    failing: std::sync::atomic::AtomicBool,
    /// How many times `refused_writes` was asked.
    grants: AtomicUsize,
}

impl Seen {
    fn builds(&self) -> usize {
        self.builds.load(Ordering::SeqCst)
    }
    fn reads(&self) -> usize {
        self.reads.lock().unwrap().len()
    }
}

struct Building(Arc<Seen>);

impl AdapterRegistry for Building {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Registry::for_process().descriptors()
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        self.0.builds.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Fake {
            instance,
            seen: self.0.clone(),
        }))
    }
}

/// Draws one text block naming the address and secret it was built with,
/// [`MARK`] and, for a key `big:<n>:…`, `n` bytes of padding. A key under
/// `commit/` is fixed, anything else live.
struct Fake {
    instance: SourceInstance,
    seen: Arc<Seen>,
}

/// In every view the fake draws, so the database can be searched for it.
const MARK: &str = "kept-in-memory-only-7f3a";
/// How long the fake takes over a key under `commit/slow`: a source further
/// away than any the spec's budget allows for.
const SLOW: std::time::Duration = std::time::Duration::from_millis(400);
/// The spec's budget for a second open of a file at one sha (#2234, Seam 1).
const CACHED_BUDGET: std::time::Duration = std::time::Duration::from_millis(150);

fn key(entity: &str) -> &str {
    entity.split_once(':').map_or("", |(_, key)| key)
}

#[async_trait::async_trait]
impl Source for Fake {
    fn descriptor(&self) -> SourceDescriptor {
        let mut d = Registry::for_process()
            .descriptors()
            .into_iter()
            .find(|d| d.adapter_kind == "mock")
            .expect("the mock is compiled in");
        d.id = self.instance.id.clone();
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
        Ok(vec![self.instance.secret.clone().unwrap_or_default()])
    }
    fn view_standing(&self, entity: &str, _section: Option<&str>) -> Standing {
        if key(entity).starts_with("commit/") {
            Standing::Fixed
        } else if key(entity).starts_with("moving/") {
            Standing::Moving
        } else {
            Standing::Live
        }
    }
    fn refusal_standing(&self, ask: &knobas_source::grants::WriteAsk) -> Standing {
        if key(&ask.entity).starts_with("moving/") {
            Standing::Moving
        } else {
            Standing::Live
        }
    }
    /// Refuses every `commit_file` ask, naming the version, so a changed
    /// answer shows.
    async fn refused_writes(
        &self,
        asks: &[knobas_source::grants::WriteAsk],
    ) -> Result<Vec<knobas_source::grants::RefusedWrite>, SourceError> {
        self.seen.grants.fetch_add(1, Ordering::SeqCst);
        if self.seen.failing.load(Ordering::SeqCst) {
            return Err(SourceError::protocol("down"));
        }
        let version = self.seen.version.load(Ordering::SeqCst);
        Ok(asks
            .iter()
            .filter(|ask| ask.op == "commit_file")
            .map(|ask| knobas_source::grants::RefusedWrite::of(ask, format!("v{version}")))
            .collect())
    }
    async fn view(
        &self,
        entity: &str,
        section: Option<&str>,
    ) -> Result<Option<SourceView>, SourceError> {
        self.seen
            .reads
            .lock()
            .unwrap()
            .push((entity.to_owned(), section.map(str::to_owned)));
        // Taken when the read is sent, as a server answers the state it
        // had then: a push during a slow read is not in its answer.
        let version = if key(entity).starts_with("moving/") {
            self.seen.version.load(Ordering::SeqCst)
        } else {
            0
        };
        if key(entity).starts_with("commit/slow") || key(entity).starts_with("moving/slow") {
            tokio::time::sleep(SLOW).await;
        }
        if self.seen.failing.load(Ordering::SeqCst) {
            return Err(SourceError::protocol("down"));
        }
        let padding = key(entity)
            .strip_prefix("commit/big:")
            .and_then(|rest| rest.split(':').next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        Ok(Some(SourceView::new(vec![ViewSection {
            id: section.unwrap_or("whole").to_owned(),
            title: "Drawn".to_owned(),
            count: None,
            deferred: false,
            blocks: vec![Block::Text {
                text: format!(
                    "{MARK} {} {} v{version} {}",
                    self.instance.base_url,
                    self.instance.secret.as_deref().unwrap_or("-"),
                    "x".repeat(padding)
                ),
                markdown: false,
            }],
        }])))
    }
}

/// A keychain that counts its reads.
#[derive(Default)]
struct CountingKeychain {
    inner: MemoryStore,
    gets: AtomicUsize,
}

impl SecretStore for CountingKeychain {
    fn get(&self, account: &KeychainAccount) -> Result<Option<Secret>, SecretError> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        self.inner.get(account)
    }
    fn put(&self, account: &KeychainAccount, secret: &Secret) -> Result<(), SecretError> {
        self.inner.put(account, secret)
    }
    fn delete(&self, account: &KeychainAccount) -> Result<(), SecretError> {
        self.inner.delete(account)
    }
}

struct Fixture {
    pool: PgPool,
    keychain: Arc<CountingKeychain>,
    secrets: Arc<dyn SecretStore>,
    registry: Arc<dyn AdapterRegistry>,
    seen: Arc<Seen>,
    live: LiveSources,
    id: String,
}

async fn fixture_bound(bound: usize) -> Fixture {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let keychain = Arc::new(CountingKeychain::default());
    let seen = Arc::new(Seen::default());
    let f = Fixture {
        pool,
        secrets: keychain.clone(),
        keychain,
        registry: Arc::new(Building(seen.clone())),
        seen,
        live: LiveSources::with_bound(bound),
        id: format!("live-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
    };
    add(&f).await;
    f
}

async fn fixture() -> Fixture {
    fixture_bound(knobas_app::sources::live::DEFAULT_BOUND).await
}

async fn add(f: &Fixture) {
    sources::crud::add(
        &f.pool,
        &f.secrets,
        f.registry.as_ref(),
        NewSource {
            id: f.id.clone(),
            adapter_kind: "mock".to_owned(),
            display_name: "Fake".to_owned(),
            base_url: "https://one.example.invalid".to_owned(),
            auth_kind: AuthMethod::Pat,
            config: serde_json::json!({}),
            secret: SecretInput::of("first-pat"),
            sync_interval_secs: 300,
            enabled: true,
        },
    )
    .await
    .unwrap();
}

impl Fixture {
    fn entity(&self, key: &str) -> String {
        format!("{}:{key}", self.id)
    }

    /// The view's text, read as the detail reads it.
    async fn read(&self, key: &str, section: Option<&str>) -> Result<String, SourcesError> {
        let view = sources::crud::view(
            &self.pool,
            &self.secrets,
            &self.registry,
            &self.live,
            &self.entity(key),
            section,
        )
        .await?
        .expect("the fake draws every entity");
        match &view.sections[0].blocks[0] {
            Block::Text { text, .. } => Ok(text.clone()),
            other => panic!("one text block, not {other:?}"),
        }
    }

    fn keychain_reads(&self) -> usize {
        self.keychain.gets.load(Ordering::SeqCst)
    }
}

// -- one source per source (#2240) --------------------------------------------

/// Ten live reads of one source, of three kinds, build it once and ask the
/// keychain once.
#[tokio::test]
async fn ten_reads_build_the_source_once_and_ask_the_keychain_once() {
    let f = fixture().await;
    let (builds, gets) = (f.seen.builds(), f.keychain_reads());
    for n in 0..8 {
        f.read(&format!("branch/{n}"), None).await.unwrap();
    }
    sources::crud::reachable_transitions(
        &f.pool,
        &f.secrets,
        &f.registry,
        &f.live,
        &f.entity("ticket/1"),
    )
    .await
    .unwrap();
    let refused = sources::crud::refused_writes(
        &f.pool,
        &f.secrets,
        &f.registry,
        &f.live,
        &[knobas_source::grants::WriteAsk {
            entity: f.entity("ticket/1"),
            op: "comment".to_owned(),
            field: None,
        }],
    )
    .await;
    assert!(refused.is_empty());
    assert_eq!(f.seen.reads(), 8, "every live view was read");
    assert_eq!(f.seen.builds() - builds, 1, "built once");
    assert_eq!(f.keychain_reads() - gets, 1, "the keychain asked once");
}

/// *Re-enter*: the next read is made with the new credential.
#[tokio::test]
async fn a_new_credential_is_the_next_reads() {
    let f = fixture().await;
    assert!(
        f.read("branch/a", None)
            .await
            .unwrap()
            .contains("first-pat")
    );
    sources::crud::set_secret(
        &f.pool,
        &f.secrets,
        &f.registry,
        &f.live,
        &f.id,
        SecretInput::of("second-pat"),
    )
    .await
    .unwrap();
    let text = f.read("branch/a", None).await.unwrap();
    assert!(text.contains("second-pat"), "{text}");
}

/// An edited address rebuilds the source, and the views read through the old
/// one are not answered for the new (it may be another server).
#[tokio::test]
async fn an_edited_address_rebuilds_the_source() {
    let f = fixture().await;
    assert!(
        f.read("branch/a", None)
            .await
            .unwrap()
            .contains("one.example")
    );
    assert!(
        f.read("commit/c1", Some("files"))
            .await
            .unwrap()
            .contains("one.example")
    );
    let builds = f.seen.builds();
    sources::crud::update(
        &f.pool,
        f.registry.as_ref(),
        &f.id,
        SourcePatch {
            base_url: Some("https://two.example.invalid".to_owned()),
            ..SourcePatch::default()
        },
    )
    .await
    .unwrap();
    assert!(
        f.read("branch/a", None)
            .await
            .unwrap()
            .contains("two.example")
    );
    assert!(
        f.read("commit/c1", Some("files"))
            .await
            .unwrap()
            .contains("two.example")
    );
    assert_eq!(f.seen.builds() - builds, 1, "rebuilt once, for both reads");
}

/// The source added back under a removed one's id, with the same address
/// and another credential: the same recipe, so only an instance dropped with
/// the removal reads the new credential.
async fn add_again(f: &Fixture) {
    sources::crud::add(
        &f.pool,
        &f.secrets,
        f.registry.as_ref(),
        NewSource {
            id: f.id.clone(),
            adapter_kind: "mock".to_owned(),
            display_name: "Fake".to_owned(),
            base_url: "https://one.example.invalid".to_owned(),
            auth_kind: AuthMethod::Pat,
            config: serde_json::json!({}),
            secret: SecretInput::of("again-pat"),
            sync_interval_secs: 300,
            enabled: true,
        },
    )
    .await
    .unwrap();
}

/// *Remove*: a read refuses as it always did, and the built instance goes
/// with the source, so one added again under the id reads its own
/// credential, even with no read between.
#[tokio::test]
async fn a_removed_source_is_dropped() {
    let f = fixture().await;
    f.read("branch/a", None).await.unwrap();
    sources::crud::delete(&f.pool, &f.secrets, &f.live, &f.id, true)
        .await
        .unwrap();
    add_again(&f).await;
    let text = f.read("branch/a", None).await.unwrap();
    assert!(text.contains("again-pat"), "{text}");

    sources::crud::delete(&f.pool, &f.secrets, &f.live, &f.id, true)
        .await
        .unwrap();
    let refused = f.read("branch/a", None).await.unwrap_err();
    assert!(matches!(refused, SourcesError::NotFound(_)), "{refused:?}");
}

/// A source whose row is gone however it went is refused, and its instance
/// dropped by that read.
#[tokio::test]
async fn a_source_gone_from_the_configuration_is_dropped_by_the_next_read() {
    let f = fixture().await;
    f.read("branch/a", None).await.unwrap();
    assert!(
        knobas_sync::config::delete(&f.pool, &f.id, true)
            .await
            .unwrap()
    );
    let refused = f.read("branch/a", None).await.unwrap_err();
    assert!(matches!(refused, SourcesError::NotFound(_)), "{refused:?}");
    add_again(&f).await;
    let text = f.read("branch/a", None).await.unwrap();
    assert!(text.contains("again-pat"), "{text}");
}

/// Two sources keep their own instances.
#[tokio::test]
async fn each_source_has_its_own_instance() {
    let f = fixture().await;
    let other = Fixture {
        pool: f.pool.clone(),
        keychain: f.keychain.clone(),
        secrets: f.secrets.clone(),
        registry: f.registry.clone(),
        seen: f.seen.clone(),
        live: f.live.clone(),
        id: format!("{}-b", f.id),
    };
    sources::crud::add(
        &other.pool,
        &other.secrets,
        other.registry.as_ref(),
        NewSource {
            id: other.id.clone(),
            adapter_kind: "mock".to_owned(),
            display_name: "Other".to_owned(),
            base_url: "https://other.example.invalid".to_owned(),
            auth_kind: AuthMethod::Pat,
            config: serde_json::json!({}),
            secret: SecretInput::of("other-pat"),
            sync_interval_secs: 300,
            enabled: true,
        },
    )
    .await
    .unwrap();
    assert!(
        f.read("branch/a", None)
            .await
            .unwrap()
            .contains("first-pat")
    );
    assert!(
        other
            .read("branch/a", None)
            .await
            .unwrap()
            .contains("other-pat")
    );
    assert!(
        f.read("branch/a", None)
            .await
            .unwrap()
            .contains("first-pat")
    );
}

// -- the read cache (#2239) ---------------------------------------------------

/// A fixed view is read once per entity and section; another sha, another
/// section, is read anew; a live view is read every time.
#[tokio::test]
async fn a_fixed_view_is_read_once_and_a_live_one_every_time() {
    let f = fixture().await;
    let first = f.read("commit/aaa", Some("file:src/lib.rs")).await.unwrap();
    let again = f.read("commit/aaa", Some("file:src/lib.rs")).await.unwrap();
    assert_eq!(first, again);
    assert_eq!(f.seen.reads(), 1, "the second read is the cache's");

    f.read("commit/bbb", Some("file:src/lib.rs")).await.unwrap();
    assert_eq!(f.seen.reads(), 2, "the same path at another sha is read");
    f.read("commit/aaa", Some("file:src/main.rs"))
        .await
        .unwrap();
    assert_eq!(f.seen.reads(), 3, "another path at the sha is read");

    f.read("branch/main", Some("file:src/lib.rs"))
        .await
        .unwrap();
    f.read("branch/main", Some("file:src/lib.rs"))
        .await
        .unwrap();
    assert_eq!(f.seen.reads(), 5, "a live view is never kept");
}

/// **The cached open's budget** (#2239): over a source that takes 400 ms, a
/// second open at the same sha answers in under 150 ms. Against the real
/// Gitea the same budget is `tests/read_cache_live.rs`'s.
#[tokio::test]
async fn a_second_open_at_a_sha_answers_within_the_budget() {
    let f = fixture().await;
    let started = std::time::Instant::now();
    f.read("commit/slow", Some("file:README.md")).await.unwrap();
    assert!(started.elapsed() >= SLOW, "the first open is the source's");
    let started = std::time::Instant::now();
    f.read("commit/slow", Some("file:README.md")).await.unwrap();
    let took = started.elapsed();
    assert!(took < CACHED_BUDGET, "a cached open took {took:?}");
}

/// Past its bound the cache drops the view used longest ago, and a dropped
/// view is read again when asked for.
#[tokio::test]
async fn the_least_recently_used_view_is_dropped_past_the_bound() {
    // Room for two of these views and not three.
    let f = fixture_bound(25_000).await;
    f.read("commit/big:10000:a", None).await.unwrap();
    f.read("commit/big:10000:b", None).await.unwrap();
    f.read("commit/big:10000:a", None).await.unwrap();
    assert_eq!(f.seen.reads(), 2, "both held");
    // `b` is now the least recently used, so `c` drops it and not `a`.
    f.read("commit/big:10000:c", None).await.unwrap();
    assert_eq!(f.seen.reads(), 3);
    f.read("commit/big:10000:a", None).await.unwrap();
    assert_eq!(f.seen.reads(), 3, "`a` was kept");
    f.read("commit/big:10000:b", None).await.unwrap();
    assert_eq!(f.seen.reads(), 4, "`b` was dropped and is read again");
    // A view larger than the whole bound is answered and not kept.
    f.read("commit/big:30000:d", None).await.unwrap();
    f.read("commit/big:30000:d", None).await.unwrap();
    assert_eq!(f.seen.reads(), 6);
}

/// Nothing a view held in memory says is written anywhere in the database.
#[tokio::test]
async fn nothing_the_cache_holds_reaches_the_database() {
    let f = fixture().await;
    let text = f.read("commit/aaa", Some("files")).await.unwrap();
    f.read("commit/aaa", Some("files")).await.unwrap();
    assert!(text.contains(MARK));
    let tables: Vec<(String, String)> = sqlx::query_as(
        "select table_schema::text, table_name::text from information_schema.tables
          where table_type = 'BASE TABLE'
            and table_schema not in ('pg_catalog', 'information_schema')",
    )
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert!(tables.len() > 10, "the schema is there: {tables:?}");
    for (schema, table) in tables {
        // The names are the catalogue's own, quoted.
        let found: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "select count(*) from \"{schema}\".\"{table}\" t where t::text like '%' || $1 || '%'"
        )))
        .bind(MARK)
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert_eq!(found, 0, "{schema}.{table} holds a cached view");
    }
}

// -- moving reads (#2242) and grants (#2241) ---------------------------------

impl Fixture {
    fn push(&self) {
        self.seen.version.fetch_add(1, Ordering::SeqCst);
    }

    async fn refused(&self, key: &str) -> Vec<String> {
        sources::crud::refused_writes(
            &self.pool,
            &self.secrets,
            &self.registry,
            &self.live,
            &[knobas_source::grants::WriteAsk {
                entity: self.entity(key),
                op: "commit_file".to_owned(),
                field: None,
            }],
        )
        .await
        .into_iter()
        .map(|r| r.reason)
        .collect()
    }
}

/// **Stale, then fresh** (#2242): a second read of a moving view answers
/// what the cache holds and starts exactly one read behind it; a push between
/// the two is the third read's answer.
#[tokio::test]
async fn a_moving_view_answers_from_the_cache_and_reads_again_behind() {
    let f = fixture().await;
    let first = f.read("moving/repo", Some("branches")).await.unwrap();
    assert!(first.contains("v0"), "{first}");
    assert_eq!(f.seen.reads(), 1);
    f.push();
    let second = f.read("moving/repo", Some("branches")).await.unwrap();
    assert_eq!(second, first, "the kept answer, at once");
    f.live.settled().await;
    assert_eq!(f.seen.reads(), 2, "one read behind it");
    let third = f.read("moving/repo", Some("branches")).await.unwrap();
    assert!(third.contains("v1"), "the push is the next read's: {third}");
}

/// The kept answer does not wait for the read behind it: over a source that
/// takes 400 ms, the second read answers within the cached budget.
#[tokio::test]
async fn a_moving_view_does_not_wait_for_its_read_behind() {
    let f = fixture().await;
    f.read("moving/slow", Some("branches")).await.unwrap();
    let started = std::time::Instant::now();
    f.read("moving/slow", Some("branches")).await.unwrap();
    let took = started.elapsed();
    assert!(took < CACHED_BUDGET, "a stale answer took {took:?}");
    f.live.settled().await;
}

/// Reads behind one key do not pile up: asks while one is under way start
/// no second beside it, and one more after it, however many they were.
#[tokio::test]
async fn one_read_behind_runs_at_a_time_per_view() {
    let f = fixture().await;
    f.read("moving/slow", Some("branches")).await.unwrap();
    f.read("moving/slow", Some("branches")).await.unwrap();
    f.read("moving/slow", Some("branches")).await.unwrap();
    f.read("moving/slow", Some("branches")).await.unwrap();
    assert_eq!(f.seen.reads(), 2, "the first, and one behind at a time");
    f.live.settled().await;
    assert_eq!(f.seen.reads(), 3, "and one more after it, for the asks meanwhile");
}

/// A push after a read behind was sent is not lost behind it: the ask that
/// came after the push, while that read was under way, is read once more
/// when it ends, and the next ask has the push (a write's echo, #2242).
#[tokio::test]
async fn an_ask_during_a_read_behind_is_read_again_after_it() {
    let f = fixture().await;
    f.read("moving/slow", Some("branches")).await.unwrap();
    f.read("moving/slow", Some("branches")).await.unwrap();
    // The read behind is sent, at v0, and under way for 400 ms.
    while f.seen.reads() < 2 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    f.push();
    let meanwhile = f.read("moving/slow", Some("branches")).await.unwrap();
    assert!(meanwhile.contains("v0"), "the kept answer, at once: {meanwhile}");
    f.live.settled().await;
    let after = f.read("moving/slow", Some("branches")).await.unwrap();
    assert!(after.contains("v1"), "the push, after both reads behind: {after}");
    f.live.settled().await;
}

/// A read behind that fails keeps the kept answer and tells nobody.
#[tokio::test]
async fn a_failed_read_behind_keeps_the_answer_and_says_nothing() {
    let f = fixture().await;
    let heard = Arc::new(Mutex::new(Vec::new()));
    let ear = heard.clone();
    f.live
        .on_fresh(move |fresh| ear.lock().unwrap().push(fresh));
    let first = f.read("moving/repo", Some("branches")).await.unwrap();
    f.seen.failing.store(true, Ordering::SeqCst);
    f.push();
    assert_eq!(
        f.read("moving/repo", Some("branches")).await.unwrap(),
        first
    );
    f.live.settled().await;
    assert_eq!(
        f.read("moving/repo", Some("branches")).await.unwrap(),
        first
    );
    f.live.settled().await;
    assert!(heard.lock().unwrap().is_empty(), "nothing said");
}

/// A read behind whose answer differs tells the shell which view moved;
/// one that answers the same says nothing.
#[tokio::test]
async fn a_read_behind_that_moved_is_told() {
    let f = fixture().await;
    let heard = Arc::new(Mutex::new(Vec::new()));
    let ear = heard.clone();
    f.live
        .on_fresh(move |fresh| ear.lock().unwrap().push(fresh));
    f.read("moving/repo", Some("branches")).await.unwrap();
    f.read("moving/repo", Some("branches")).await.unwrap();
    f.live.settled().await;
    assert!(heard.lock().unwrap().is_empty(), "the same answer");
    f.push();
    f.read("moving/repo", Some("branches")).await.unwrap();
    f.live.settled().await;
    let heard = heard.lock().unwrap().clone();
    assert_eq!(heard.len(), 1, "{heard:?}");
    assert_eq!(heard[0].entity_id, f.entity("moving/repo"));
    assert_eq!(heard[0].section.as_deref(), Some("branches"));
}

/// A fixed view is never read behind (#2239's stay put).
#[tokio::test]
async fn a_fixed_view_is_never_read_behind() {
    let f = fixture().await;
    f.read("commit/aaa", Some("file:a")).await.unwrap();
    f.read("commit/aaa", Some("file:a")).await.unwrap();
    f.live.settled().await;
    assert_eq!(f.seen.reads(), 1);
}

/// **Grants** (#2241): a moving answer is asked once while kept, read again
/// behind, and a changed answer is the next ask's; a live one is asked
/// every time.
#[tokio::test]
async fn grants_are_kept_and_read_again_behind() {
    let f = fixture().await;
    assert_eq!(f.refused("moving/repo").await, ["v0"]);
    assert_eq!(f.seen.grants.load(Ordering::SeqCst), 1);
    f.push();
    assert_eq!(f.refused("moving/repo").await, ["v0"], "kept");
    f.live.settled().await;
    assert_eq!(f.seen.grants.load(Ordering::SeqCst), 2, "once behind");
    assert_eq!(f.refused("moving/repo").await, ["v1"], "changed grants");
    f.live.settled().await;

    let before = f.seen.grants.load(Ordering::SeqCst);
    f.refused("ticket/1").await;
    f.refused("ticket/1").await;
    assert_eq!(f.seen.grants.load(Ordering::SeqCst) - before, 2, "live");
}

/// A new credential while a read behind is under way: the read, made with
/// the old one, lands nowhere and tells nobody, and the next read is made
/// with the new credential (ADR-0032: a new credential may read less).
#[tokio::test]
async fn a_read_behind_across_a_new_credential_lands_nowhere() {
    let f = fixture().await;
    let heard = Arc::new(Mutex::new(Vec::new()));
    let ear = heard.clone();
    f.live
        .on_fresh(move |fresh| ear.lock().unwrap().push(fresh));
    f.read("moving/slow", Some("branches")).await.unwrap();
    f.read("moving/slow", Some("branches")).await.unwrap();
    while f.seen.reads() < 2 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    f.push();
    sources::crud::set_secret(
        &f.pool,
        &f.secrets,
        &f.registry,
        &f.live,
        &f.id,
        SecretInput::of("second-pat"),
    )
    .await
    .unwrap();
    f.live.settled().await;
    assert!(heard.lock().unwrap().is_empty(), "nothing told");
    let next = f.read("moving/slow", Some("branches")).await.unwrap();
    assert!(next.contains("second-pat"), "the new credential's read: {next}");
    assert_eq!(f.seen.reads(), 3, "read again, not answered from the old one");
    f.live.settled().await;
}
