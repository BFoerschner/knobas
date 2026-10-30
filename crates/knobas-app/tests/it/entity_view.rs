//! The `entity_view` read's app half (#661, ADR-0020): it routes to the
//! entity's own source the way `reachable_transitions` does, and it keeps an
//! entity-list row's address only where the mirror holds that entity (rule 7).
//!
//! The adapter here is a fake that draws a fixed view, because what is under
//! test is the app's handling of a view and not any adapter's drawing of one:
//! Gitea's commit view is witnessed in `knobas-source-gitea`'s `view.rs` and,
//! against the real server, in `live_gitea.rs`.

use std::sync::{Arc, Mutex};

use knobas_app::sources::{self, NewSource, Registry, SecretInput};
use knobas_secrets::{MemoryStore, SecretStore};
use knobas_source::instance::SourceInstance;
use knobas_source::view::{Block, CommitHint, SourceView, ViewRow, ViewSection};
use knobas_source::{AuthMethod, Source, SourceDescriptor, SourceError};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

/// Every `(entity, section)` the fake was asked for, in order.
type Asked = Arc<Mutex<Vec<(String, Option<String>)>>>;

struct DrawingRegistry(Asked);

impl AdapterRegistry for DrawingRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Registry::for_process().descriptors()
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Drawing {
            id: instance.id,
            asked: self.0.clone(),
        }))
    }
}

/// Draws one section of parents for `<id>:drawn`, nothing for `<id>:plain`,
/// fails for `<id>:down`, and draws `<id>:changes:<sha>` as a build's changes
/// -- rows naming a commit by repository URL and sha, as TeamCity does (#665).
struct Drawing {
    id: String,
    asked: Asked,
}

fn row(entity_id: Option<String>, title: &str) -> ViewRow {
    ViewRow {
        entity_id,
        title: title.to_owned(),
        meta: None,
        tone: None,
        chip: None,
        url: None,
        commit: None,
    }
}

#[async_trait::async_trait]
impl Source for Drawing {
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
        Err(SourceError::protocol("the drawing fake has no workflow"))
    }
    async fn view(
        &self,
        entity: &str,
        section: Option<&str>,
    ) -> Result<Option<SourceView>, SourceError> {
        self.asked
            .lock()
            .unwrap()
            .push((entity.to_owned(), section.map(str::to_owned)));
        let key = entity.split_once(':').map_or("", |(_, key)| key);
        // #683: hints named by another address of a mirrored host.
        if let Some(rest) = key.strip_prefix("aliased:") {
            let (sha, host) = rest.split_once(':').expect("aliased:<sha>:<host>");
            let hinted = |title: &str, repo_url: String| ViewRow {
                commit: Some(CommitHint {
                    repo_url,
                    sha: sha.to_owned(),
                }),
                ..row(None, title)
            };
            return Ok(Some(SourceView {
                actions: vec![],
                sections: vec![ViewSection {
                    id: "changes".into(),
                    title: "Changes".into(),
                    count: Some(4),
                    deferred: false,
                    blocks: vec![Block::Entities {
                        title: None,
                        rows: vec![
                            hinted(
                                "by its CI address",
                                format!("http://{host}:3000/tidewater/payout-service.git"),
                            ),
                            hinted("by ssh", format!("{host}:tidewater/payout-service.git")),
                            hinted(
                                "by another host",
                                "http://other.example/tidewater/payout-service.git".to_owned(),
                            ),
                            hinted(
                                "by an undeclared adapter's address",
                                format!("http://{host}-u:3000/tidewater/payout-service.git"),
                            ),
                        ],
                    }],
                }],
            }));
        }
        if let Some(sha) = key.strip_prefix("changes:") {
            let hinted = |title: &str, repo_url: &str| ViewRow {
                commit: Some(CommitHint {
                    repo_url: repo_url.to_owned(),
                    sha: sha.to_owned(),
                }),
                ..row(None, title)
            };
            return Ok(Some(SourceView {
                actions: vec![],
                sections: vec![ViewSection {
                    id: "changes".into(),
                    title: "Changes".into(),
                    count: Some(4),
                    deferred: false,
                    blocks: vec![Block::Entities {
                        title: None,
                        rows: vec![
                            // A clone URL, host in capitals: the commit page
                            // the mirror holds, by the paste resolver's rule.
                            hinted(
                                "mirrored",
                                "HTTP://Gitea.Example/tidewater/payout-service.git",
                            ),
                            // The same sha in a repository nothing mirrors.
                            hinted("unmirrored", "http://gitea.example/tidewater/elsewhere.git"),
                            // An scp-like git URL is no web page; a miss, not an error.
                            hinted("ssh", "git@gitea.example:tidewater/payout-service.git"),
                            row(None, "no hint"),
                        ],
                    }],
                }],
            }));
        }
        match key {
            "plain" => Ok(None),
            "down" => Err(SourceError::Unreachable("connection refused".to_owned())),
            _ => Ok(Some(SourceView {
                actions: vec![],
                sections: vec![ViewSection {
                    id: "overview".into(),
                    title: "Overview".into(),
                    count: None,
                    deferred: false,
                    blocks: vec![
                        Block::Entities {
                            title: Some("Parents".into()),
                            rows: vec![
                                row(Some(format!("{}:held", self.id)), "held"),
                                row(Some(format!("{}:never-synced", self.id)), "elsewhere"),
                                row(None, "no address"),
                            ],
                        },
                        Block::Text {
                            text: "untouched".into(),
                            markdown: false,
                        },
                    ],
                }],
            })),
        }
    }
}

struct Fixture {
    pool: PgPool,
    secrets: Arc<dyn SecretStore>,
    registry: DrawingRegistry,
    asked: Asked,
    id: String,
}

async fn fixture() -> Fixture {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let asked: Asked = Arc::default();
    let f = Fixture {
        pool,
        secrets: Arc::new(MemoryStore::new()),
        registry: DrawingRegistry(asked.clone()),
        asked,
        id: format!("view-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
    };
    sources::crud::add(
        &f.pool,
        &f.secrets,
        &f.registry,
        NewSource {
            id: f.id.clone(),
            adapter_kind: "mock".to_owned(),
            display_name: "Drawing".to_owned(),
            base_url: "https://drawing.example.invalid".to_owned(),
            auth_kind: AuthMethod::Pat,
            config: serde_json::json!({}),
            secret: SecretInput::of("pat"),
            sync_interval_secs: 300,
            enabled: true,
        },
    )
    .await
    .unwrap();
    // The one parent the mirror holds.
    let held = format!("{}:held", f.id);
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'commit', 'held')")
        .bind(&held)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload)
         values ($1, $2, 'commit', 'held', '{}'::jsonb)",
    )
    .bind(&held)
    .bind(&f.id)
    .execute(&f.pool)
    .await
    .unwrap();
    f
}

async fn view(f: &Fixture, key: &str, section: Option<&str>) -> Result<Option<SourceView>, String> {
    sources::crud::view(
        &f.pool,
        &f.secrets,
        &crate::support::shared(DrawingRegistry(f.asked.clone())),
        &knobas_app::sources::live::LiveSources::default(),
        &format!("{}:{key}", f.id),
        section,
    )
    .await
    .map_err(|error| error.to_string())
}

/// Rule 7: an address the mirror holds opens a detail; one it does not is a
/// row of plain text, whatever the adapter named it.
#[tokio::test]
async fn a_row_keeps_its_address_only_where_the_mirror_holds_the_entity() {
    let f = fixture().await;
    let drawn = view(&f, "drawn", None).await.unwrap().expect("drawn");
    let blocks = &drawn.sections[0].blocks;
    let Block::Entities { rows, .. } = &blocks[0] else {
        panic!("{blocks:#?}");
    };
    let addresses: Vec<_> = rows
        .iter()
        .map(|r| (r.title.as_str(), r.entity_id.clone()))
        .collect();
    assert_eq!(
        addresses,
        [
            ("held", Some(format!("{}:held", f.id))),
            ("elsewhere", None),
            ("no address", None),
        ]
    );
    assert_eq!(
        blocks[1],
        Block::Text {
            text: "untouched".into(),
            markdown: false
        }
    );
}

/// The read goes to the entity's own source with the section it was asked
/// for; `None` and a failure come back as what they are.
#[tokio::test]
async fn the_read_routes_to_the_entitys_source_and_keeps_none_and_failure_apart() {
    let f = fixture().await;
    view(&f, "drawn", Some("log")).await.unwrap();
    assert_eq!(view(&f, "plain", None).await.unwrap(), None);
    let failed = view(&f, "down", None).await.unwrap_err();
    assert!(failed.contains("unreachable"), "{failed}");
    assert_eq!(
        f.asked.lock().unwrap().as_slice(),
        [
            (format!("{}:drawn", f.id), Some("log".to_owned())),
            (format!("{}:plain", f.id), None),
            (format!("{}:down", f.id), None),
        ]
    );

    let unknown = sources::crud::view(
        &f.pool,
        &f.secrets,
        &crate::support::shared(DrawingRegistry(f.asked.clone())),
        &knobas_app::sources::live::LiveSources::default(),
        "nowhere-configured:x",
        None,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(unknown, knobas_app::sources::SourcesError::NotFound(_)),
        "{unknown:?}"
    );
}

/// #665: a row that names a commit by repository URL and sha opens the
/// mirrored commit when the mirror holds that commit's page -- resolved in
/// the app with the paste resolver, since the adapter that drew it cannot see
/// another source's mirror -- and is plain text when it does not. The hint
/// never reaches the shell.
#[tokio::test]
async fn a_commit_named_by_repository_and_sha_opens_the_mirrored_commit() {
    let f = fixture().await;
    let sha = format!("{}0000", uuid::Uuid::new_v4().simple());
    let mirrored = format!("{}:tidewater/payout-service@{sha}", f.id);
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'commit', 'c')")
        .bind(&mirrored)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload, web_url)
         values ($1, $2, 'commit', 'c', '{}'::jsonb, $3)",
    )
    .bind(&mirrored)
    .bind(&f.id)
    .bind(format!(
        "http://gitea.example/tidewater/payout-service/commit/{sha}"
    ))
    .execute(&f.pool)
    .await
    .unwrap();

    let drawn = view(&f, &format!("changes:{sha}"), None)
        .await
        .unwrap()
        .expect("drawn");
    let Block::Entities { rows, .. } = &drawn.sections[0].blocks[0] else {
        panic!("{drawn:#?}");
    };
    let resolved: Vec<_> = rows
        .iter()
        .map(|r| (r.title.as_str(), r.entity_id.clone(), r.commit.clone()))
        .collect();
    assert_eq!(
        resolved,
        [
            ("mirrored", Some(mirrored.clone()), None),
            ("unmirrored", None, None),
            ("ssh", None, None),
            ("no hint", None, None),
        ]
    );
    let wire = serde_json::to_string(&drawn).unwrap();
    assert!(
        !wire.contains("repo_url"),
        "the hint crossed to the shell: {wire}"
    );
}

/// #683: a source whose adapter declares `clone_urls` -- the other addresses
/// CI clones it from -- has a commit hint named by one of them resolve to its
/// mirrored commit, by URL or scp-like remote; a hint on any other host with
/// the same owner/repo stays plain text, and so does one named by an address
/// a source lists whose adapter declares no such key.
#[tokio::test]
async fn a_commit_named_by_a_configured_clone_url_opens_the_mirrored_commit() {
    let f = fixture().await;
    let sha = format!("{}0000", uuid::Uuid::new_v4().simple());
    // A host of this test's own: the pool is shared, and addresses are read
    // from every configured source.
    let host = format!("ci-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    // The Gitea adapter declares the key; the mock does not, so the same list
    // in a mock's config is nobody's declaration.
    let mut mirrored = Vec::new();
    for (suffix, adapter_kind, ci_host) in [
        ("git", "gitea", host.clone()),
        ("undeclared", "mock", format!("{host}-u")),
    ] {
        let id = format!("{}-{suffix}", f.id);
        sources::crud::add(
            &f.pool,
            &f.secrets,
            &f.registry,
            NewSource {
                id: id.clone(),
                adapter_kind: adapter_kind.to_owned(),
                display_name: "Git".to_owned(),
                base_url: format!("http://{ci_host}.pages.example/"),
                auth_kind: AuthMethod::Pat,
                config: serde_json::json!({
                    knobas_source::clone_urls::CONFIG_KEY:
                        [format!("http://{ci_host}:3000"), format!("git@{ci_host}:")]
                }),
                secret: SecretInput::of("pat"),
                sync_interval_secs: 300,
                enabled: true,
            },
        )
        .await
        .unwrap();
        let entity = format!("{id}:tidewater/payout-service@{sha}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'commit', 'c')")
            .bind(&entity)
            .execute(&f.pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, payload, web_url)
             values ($1, $2, 'commit', 'c', '{}'::jsonb, $3)",
        )
        .bind(&entity)
        .bind(&id)
        .bind(format!(
            "http://{ci_host}.pages.example/tidewater/payout-service/commit/{sha}"
        ))
        .execute(&f.pool)
        .await
        .unwrap();
        mirrored.push(entity);
    }
    let mirrored = mirrored[0].clone();

    let drawn = view(&f, &format!("aliased:{sha}:{host}"), None)
        .await
        .unwrap()
        .expect("drawn");
    let Block::Entities { rows, .. } = &drawn.sections[0].blocks[0] else {
        panic!("{drawn:#?}");
    };
    let resolved: Vec<_> = rows
        .iter()
        .map(|r| (r.title.as_str(), r.entity_id.clone()))
        .collect();
    assert_eq!(
        resolved,
        [
            ("by its CI address", Some(mirrored.clone())),
            ("by ssh", Some(mirrored.clone())),
            ("by another host", None),
            ("by an undeclared adapter's address", None),
        ]
    );
}
