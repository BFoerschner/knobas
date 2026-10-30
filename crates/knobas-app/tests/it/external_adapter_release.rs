//! **External adapters from a release** (ADR-0028): install from a GitHub
//! repository URL or a manifest URL, the update check and update (#1209),
//! uninstall, and the backup's restore (#1210), at the app's seam
//! (`sources::install` and `sources::installed`, the commands' bodies).
//!
//! The releases are served by [`Forge`], a local server that answers both
//! release APIs knobas reads: GitHub's `/repos/<o>/<r>/releases/…` and
//! Gitea's and Forgejo's `/api/v1/repos/<o>/<r>/releases/…`, with each
//! release's assets under `/<o>/<r>/releases/download/<tag>/<file>`, the
//! path every forge's downloads have. So the gate needs no network. The
//! real public repository is [`a_public_github_repository_installs_its_latest_release`],
//! `#[ignore]`d, which reads `KNOBAS_ADAPTER_LIVE_REPO`.
//!
//! The newer release is the fixture with one literal changed, the same
//! length, so it still loads and passes every check: another artifact, so
//! another hash.

use std::path::Path;
use std::sync::{Arc, Mutex};

use knobas_app::sources::Registry;
use knobas_app::sources::install::{self, InstallError};
use knobas_app::sources::installed::{self, Removal, UpdateCheck};
use knobas_app::sources::remote::Endpoints;
use knobas_sync::scheduler::AdapterRegistry as _;
use sqlx::PgPool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::external_adapter_install::{
    add_and_sync, fixture_bytes, kinds, manifest_for, patched, scratch,
};

const OWNER: &str = "harbour";
const REPO: &str = "surveys";

/// The fixture as a newer release: another artifact that passes every check.
fn newer_bytes() -> Vec<u8> {
    patched(&fixture_bytes(), "Reefer plug audit", "Reefer plug check")
}

/// One release: its tag, manifest and artifact.
#[derive(Clone)]
struct Published {
    tag: String,
    manifest: serde_json::Value,
    artifact: Vec<u8>,
}

/// A local forge with one repository, [`OWNER`]/[`REPO`].
struct Forge {
    server: MockServer,
    /// Oldest first; the last is the latest.
    releases: Mutex<Vec<Published>>,
}

impl Forge {
    async fn start() -> Self {
        Self {
            server: MockServer::builder().start().await,
            releases: Mutex::new(Vec::new()),
        }
    }

    /// GitHub, as far as knobas reads it, is this server.
    fn endpoints(&self) -> Endpoints {
        Endpoints {
            github_host: "github.test".to_owned(),
            github_api: self.server.uri(),
        }
    }

    fn repo_url(&self) -> String {
        format!("https://github.test/{OWNER}/{REPO}")
    }

    /// The manifest's download URL in release `tag`.
    fn download(&self, tag: &str, file: &str) -> String {
        format!(
            "{}/{OWNER}/{REPO}/releases/download/{tag}/{file}",
            self.server.uri()
        )
    }

    /// Publish `artifact` as release `tag` under its truthful manifest,
    /// edited by `edit`, as the latest. A tag published again is replaced.
    async fn publish(
        &self,
        tag: &str,
        artifact: &[u8],
        edit: impl FnOnce(&mut serde_json::Value),
    ) -> Published {
        let mut manifest = manifest_for(artifact);
        edit(&mut manifest);
        let release = Published {
            tag: tag.to_owned(),
            manifest,
            artifact: artifact.to_vec(),
        };
        let releases = {
            let mut releases = self.releases.lock().unwrap();
            releases.retain(|r| r.tag != tag);
            releases.push(release.clone());
            releases.clone()
        };
        self.server.reset().await;
        for (i, r) in releases.iter().enumerate() {
            self.mount(r, i + 1 == releases.len()).await;
        }
        release
    }

    async fn mount(&self, r: &Published, latest: bool) {
        let answer = serde_json::json!({
            "tag_name": r.tag,
            "assets": [
                { "name": "knobas-adapter.json", "browser_download_url": self.download(&r.tag, "knobas-adapter.json") },
                // Behind a redirect to another path, as GitHub serves an asset.
                { "name": "fixture.wasm", "browser_download_url": format!("{}/objects/{}/fixture.wasm", self.server.uri(), r.tag) },
            ],
        });
        let mut apis = vec![
            format!("/repos/{OWNER}/{REPO}/releases/tags/{}", r.tag),
            format!("/api/v1/repos/{OWNER}/{REPO}/releases/tags/{}", r.tag),
        ];
        if latest {
            apis.push(format!("/repos/{OWNER}/{REPO}/releases/latest"));
            apis.push(format!("/api/v1/repos/{OWNER}/{REPO}/releases/latest"));
        }
        for api in apis {
            Mock::given(method("GET"))
                .and(path(api))
                .respond_with(ResponseTemplate::new(200).set_body_json(&answer))
                .mount(&self.server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path(format!("/objects/{}/fixture.wasm", r.tag)))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", self.download(&r.tag, "fixture.wasm")),
            )
            .mount(&self.server)
            .await;
        let base = format!("/{OWNER}/{REPO}/releases/download/{}", r.tag);
        Mock::given(method("GET"))
            .and(path(format!("{base}/knobas-adapter.json")))
            .respond_with(ResponseTemplate::new(200).set_body_string(r.manifest.to_string()))
            .mount(&self.server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{base}/fixture.wasm")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(r.artifact.clone()))
            .mount(&self.server)
            .await;
    }
}

async fn install_from(
    pool: &PgPool,
    registry: &Registry,
    store: &Path,
    forge: &Forge,
    input: &str,
) -> install::InstalledAdapter {
    install::install_from(pool, registry, store, &forge.endpoints(), input, None)
        .await
        .unwrap_or_else(|error| panic!("{input} must install: {error}"))
}

async fn only(pool: &PgPool, registry: &Registry) -> installed::AdapterStatus {
    let mut list = installed::list(pool, registry).await.unwrap();
    assert_eq!(list.len(), 1, "one adapter is installed");
    list.remove(0)
}

#[tokio::test]
async fn a_repository_url_installs_its_latest_release_pinned_to_tag_and_hash() {
    let forge = Forge::start().await;
    forge.publish("v0.0.9", &newer_bytes(), |_| {}).await;
    let latest = forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (pool, _connector) = scratch("release-repo").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();

    let installed = install_from(&pool, &registry, store.path(), &forge, &forge.repo_url()).await;

    assert_eq!(installed.tag.as_deref(), Some("v0.1.0"));
    assert_eq!(installed.source_url, forge.repo_url());
    let sha = latest.manifest["artifact"]["sha256"].as_str().unwrap();
    assert_eq!(installed.artifact_sha256, sha);
    assert!(install::artifact_path(store.path(), sha).is_file());
    assert!(kinds(&registry).contains(&"wasm-fixture".to_owned()));
    let status = only(&pool, &registry).await;
    assert_eq!(
        (status.tag.as_deref(), status.artifact_sha256.as_str()),
        (Some("v0.1.0"), sha)
    );
}

#[tokio::test]
async fn a_release_download_url_installs_with_the_tag_its_path_names() {
    let forge = Forge::start().await;
    forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (pool, _connector) = scratch("release-url").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    let url = forge.download("v0.1.0", "knobas-adapter.json");

    let installed = install_from(&pool, &registry, store.path(), &forge, &url).await;

    assert_eq!(installed.tag.as_deref(), Some("v0.1.0"));
    assert_eq!(installed.source_url, url);
}

#[tokio::test]
async fn a_manifest_url_that_is_no_release_installs_pinned_by_its_hash() {
    let forge = Forge::start().await;
    let artifact = fixture_bytes();
    for (file, body) in [
        (
            "/adapters/knobas-adapter.json",
            manifest_for(&artifact).to_string().into_bytes(),
        ),
        ("/adapters/fixture.wasm", artifact.clone()),
    ] {
        Mock::given(method("GET"))
            .and(path(file))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&forge.server)
            .await;
    }
    let (pool, _connector) = scratch("release-plain").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    let url = format!("{}/adapters/knobas-adapter.json", forge.server.uri());

    let installed = install_from(&pool, &registry, store.path(), &forge, &url).await;

    assert_eq!(installed.tag, None);
    assert_eq!(
        installed.artifact_sha256,
        knobas_wasm_host::manifest::sha256(&artifact)
    );
}

#[tokio::test]
async fn a_release_with_no_manifest_is_refused_naming_it() {
    let forge = Forge::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{OWNER}/{REPO}/releases/latest")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tag_name": "v1", "assets": [],
        })))
        .mount(&forge.server)
        .await;
    let (pool, _connector) = scratch("release-none").await;
    let store = tempfile::tempdir().unwrap();
    let Err(error) = install::install_from(
        &pool,
        &Registry::default(),
        store.path(),
        &forge.endpoints(),
        &forge.repo_url(),
        None,
    )
    .await
    else {
        panic!("a release with no manifest must not install");
    };
    assert_eq!(
        error.to_ipc().message,
        "the release v1 has no knobas-adapter.json"
    );
}

// -- the update check and update (#1209) ---------------------------------------

fn sha_of(release: &Published) -> String {
    release.manifest["artifact"]["sha256"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// A source of the fixture builds: the adapter is loaded and running.
fn builds(registry: &Registry) -> bool {
    registry
        .build(knobas_source::instance::SourceInstance {
            id: "surveys".to_owned(),
            kind: "wasm-fixture".to_owned(),
            display_name: "Harbour surveys".to_owned(),
            base_url: String::new(),
            auth: None,
            secret: None,
            account: None,
            config: serde_json::json!({}),
        })
        .is_ok()
}

#[tokio::test]
async fn a_newer_tag_shows_update_available_and_nothing_changes_until_the_reader_updates() {
    let forge = Forge::start().await;
    let first = forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (pool, _connector) = scratch("update-available").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    install_from(&pool, &registry, store.path(), &forge, &forge.repo_url()).await;

    let list = installed::check_updates(&pool, &registry, &forge.endpoints())
        .await
        .unwrap();
    assert_eq!(list[0].update, Some(UpdateCheck::Current));

    let newer = forge.publish("v0.2.0", &newer_bytes(), |_| {}).await;
    let list = installed::check_updates(&pool, &registry, &forge.endpoints())
        .await
        .unwrap();
    assert_eq!(
        list[0].update,
        Some(UpdateCheck::Available {
            tag: Some("v0.2.0".to_owned()),
            adapter_version: "0.1.0".to_owned(),
        })
    );
    // Told, not changed.
    assert_eq!(list[0].tag.as_deref(), Some("v0.1.0"));
    assert_eq!(list[0].artifact_sha256, sha_of(&first));
    assert!(!install::artifact_path(store.path(), &sha_of(&newer)).exists());

    let updated = installed::update(
        &pool,
        &registry,
        store.path(),
        &forge.endpoints(),
        "wasm-fixture",
        None,
    )
    .await
    .expect("the newer release passes every check");
    assert_eq!(updated.tag.as_deref(), Some("v0.2.0"));
    assert_eq!(updated.artifact_sha256, sha_of(&newer));
    assert_eq!(updated.update, None, "the check is answered by the update");
    assert!(install::artifact_path(store.path(), &sha_of(&newer)).is_file());
    assert!(
        !install::artifact_path(store.path(), &sha_of(&first)).exists(),
        "the old artifact goes once nothing pins it"
    );
    assert!(builds(&registry));

    // And a restart loads the updated pin.
    let restarted = Registry::default();
    let loaded = install::load_installed(&pool, &restarted, store.path())
        .await
        .unwrap();
    assert_eq!(loaded, vec![("wasm-fixture".to_owned(), Ok(()))]);
}

#[tokio::test]
async fn a_forge_release_url_sees_its_forge_s_newer_release() {
    let forge = Forge::start().await;
    forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (pool, _connector) = scratch("update-forge").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    let url = forge.download("v0.1.0", "knobas-adapter.json");
    install_from(&pool, &registry, store.path(), &forge, &url).await;

    forge.publish("v0.2.0", &newer_bytes(), |_| {}).await;
    let list = installed::check_updates(&pool, &registry, &forge.endpoints())
        .await
        .unwrap();
    assert!(
        matches!(&list[0].update, Some(UpdateCheck::Available { tag: Some(t), .. }) if t == "v0.2.0"),
        "{:?}",
        list[0].update
    );
}

#[tokio::test]
async fn an_update_whose_static_checks_fail_stays_on_the_old_version_with_the_reason() {
    let forge = Forge::start().await;
    let first = forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (pool, _connector) = scratch("update-refused").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    install_from(&pool, &registry, store.path(), &forge, &forge.repo_url()).await;
    forge
        .publish("v0.2.0", &newer_bytes(), |m| {
            m["interface_version"] = "2.0.0".into()
        })
        .await;
    installed::check_updates(&pool, &registry, &forge.endpoints())
        .await
        .unwrap();

    let Err(error) = installed::update(
        &pool,
        &registry,
        store.path(),
        &forge.endpoints(),
        "wasm-fixture",
        None,
    )
    .await
    else {
        panic!("an update the checks refuse must not happen");
    };
    let reason = error.to_ipc().message;
    assert!(
        reason.contains("install refused: the manifest asks for knobas:adapter@2.0.0"),
        "{reason}"
    );

    let status = only(&pool, &registry).await;
    assert_eq!(status.tag.as_deref(), Some("v0.1.0"));
    assert_eq!(status.artifact_sha256, sha_of(&first));
    assert_eq!(
        status.update,
        Some(UpdateCheck::Refused {
            tag: Some("v0.2.0".to_owned()),
            reason,
        })
    );
    assert!(install::artifact_path(store.path(), &sha_of(&first)).is_file());
    assert!(builds(&registry), "the old version still runs");
}

// -- the update's preview and confirm (#2031) --------------------------------------

/// The preview answers what is installed beside the latest release, with
/// the hosts, roles and limits of each, and switches nothing.
#[tokio::test]
async fn an_update_preview_shows_the_release_beside_what_is_installed_and_switches_nothing() {
    let forge = Forge::start().await;
    let first = forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (pool, _connector) = scratch("update-preview").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    install_from(&pool, &registry, store.path(), &forge, &forge.repo_url()).await;
    let newer = forge
        .publish("v0.2.0", &newer_bytes(), |m| {
            m["hosts"] = serde_json::json!(["{base_url}", "api.example.com"]);
            m["limits"] = serde_json::json!({ "memory_mib": 512 });
        })
        .await;

    let preview = installed::preview_update(&pool, &registry, &forge.endpoints(), "wasm-fixture")
        .await
        .expect("the newer release passes every check");
    assert_eq!(preview.installed.tag.as_deref(), Some("v0.1.0"));
    assert_eq!(preview.installed.artifact_sha256, sha_of(&first));
    assert_eq!(preview.installed.hosts, ["{base_url}"]);
    assert_eq!(preview.installed.name, "wasm-fixture");
    assert_eq!(
        preview
            .installed
            .roles
            .iter()
            .map(|r| r.role.as_str())
            .collect::<Vec<_>>(),
        ["wasm-fixture:survey-desk", "ci"],
        "the fixture's custom role and CI"
    );
    assert_eq!(
        preview.installed.limits,
        install::AdapterLimits {
            deadline_secs: 30,
            deadline_asked: true,
            memory_mib: 256,
            memory_asked: false,
        }
    );
    assert_eq!(preview.release.tag.as_deref(), Some("v0.2.0"));
    assert_eq!(preview.release.artifact_sha256, sha_of(&newer));
    assert_eq!(preview.release.hosts, ["{base_url}", "api.example.com"]);
    assert_eq!(preview.release.roles, preview.installed.roles);
    assert_eq!(
        preview.release.limits,
        install::AdapterLimits {
            deadline_secs: 600,
            deadline_asked: false,
            memory_mib: 512,
            memory_asked: true,
        }
    );

    let status = only(&pool, &registry).await;
    assert_eq!(status.tag.as_deref(), Some("v0.1.0"), "nothing switched");
    assert_eq!(status.hosts, ["{base_url}"]);
    assert!(!install::artifact_path(store.path(), &sha_of(&newer)).exists());
}

/// A release the checks refuse is refused at the preview, with the reason
/// recorded beside the check as an update records it.
#[tokio::test]
async fn an_update_preview_the_checks_refuse_records_the_reason() {
    let forge = Forge::start().await;
    forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (pool, _connector) = scratch("update-preview-refused").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    install_from(&pool, &registry, store.path(), &forge, &forge.repo_url()).await;
    forge
        .publish("v0.2.0", &newer_bytes(), |m| {
            m["interface_version"] = "2.0.0".into()
        })
        .await;
    installed::check_updates(&pool, &registry, &forge.endpoints())
        .await
        .unwrap();

    let Err(error) =
        installed::preview_update(&pool, &registry, &forge.endpoints(), "wasm-fixture").await
    else {
        panic!("a release the checks refuse has no preview");
    };
    let reason = error.to_ipc().message;
    assert_eq!(
        only(&pool, &registry).await.update,
        Some(UpdateCheck::Refused {
            tag: Some("v0.2.0".to_owned()),
            reason,
        })
    );
}

/// The confirm switches to the release the reader was shown and no other:
/// one published after the preview is refused, and the old version stays.
#[tokio::test]
async fn an_update_to_another_artifact_than_the_one_confirmed_is_refused() {
    let forge = Forge::start().await;
    let first = forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (pool, _connector) = scratch("update-pinned").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    install_from(&pool, &registry, store.path(), &forge, &forge.repo_url()).await;
    forge.publish("v0.2.0", &newer_bytes(), |_| {}).await;
    let shown = installed::preview_update(&pool, &registry, &forge.endpoints(), "wasm-fixture")
        .await
        .unwrap()
        .release
        .confirm_sha256;
    let later = patched(&fixture_bytes(), "Reefer plug audit", "Reefer plug probe");
    forge.publish("v0.3.0", &later, |_| {}).await;

    let Err(error) = installed::update(
        &pool,
        &registry,
        store.path(),
        &forge.endpoints(),
        "wasm-fixture",
        Some(&shown),
    )
    .await
    else {
        panic!("a release other than the confirmed one must not switch");
    };
    let ipc = error.to_ipc();
    assert_eq!(ipc.code, knobas_app::IpcErrorCode::Invalid);
    assert!(
        ipc.message
            .contains("the release changed after it was checked"),
        "{}",
        ipc.message
    );
    let status = only(&pool, &registry).await;
    assert_eq!(status.tag.as_deref(), Some("v0.1.0"));
    assert_eq!(status.artifact_sha256, sha_of(&first));
    assert!(builds(&registry), "the old version still runs");
}

/// The release swapped under its tag after the preview, around the same
/// artifact, for a manifest with a host the reader was not shown, is refused at the confirm: the artifact's
/// hash alone does not cover the manifest's hosts.
#[tokio::test]
async fn an_update_whose_hosts_widened_after_the_preview_is_refused() {
    let forge = Forge::start().await;
    let first = forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (pool, _connector) = scratch("update-pinned-hosts").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    install_from(&pool, &registry, store.path(), &forge, &forge.repo_url()).await;
    forge.publish("v0.2.0", &newer_bytes(), |_| {}).await;
    let shown = installed::preview_update(&pool, &registry, &forge.endpoints(), "wasm-fixture")
        .await
        .unwrap()
        .release;
    forge
        .publish("v0.2.0", &newer_bytes(), |m| {
            m["hosts"] = serde_json::json!(["{base_url}", "collect.example"]);
        })
        .await;

    let Err(error) = installed::update(
        &pool,
        &registry,
        store.path(),
        &forge.endpoints(),
        "wasm-fixture",
        Some(&shown.confirm_sha256),
    )
    .await
    else {
        panic!("a host the reader was not shown must not switch in");
    };
    assert!(
        error
            .to_string()
            .contains("the release changed after it was checked"),
        "{error}"
    );
    let status = only(&pool, &registry).await;
    assert_eq!(status.artifact_sha256, sha_of(&first));
    assert_eq!(status.hosts, ["{base_url}"]);
}

// -- uninstall (#1210) -----------------------------------------------------------

async fn items_of(pool: &PgPool, source: &str) -> i64 {
    sqlx::query_scalar("select count(*) from sync.item where source_id = $1")
        .bind(source)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn source_ids(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar("select id from knobas.source_config order by id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn uninstall_with_two_sources_asks_once_per_source_and_does_what_each_says() {
    let forge = Forge::start().await;
    let release = forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (pool, connector) = scratch("uninstall-two").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    install_from(&pool, &registry, store.path(), &forge, &forge.repo_url()).await;
    assert_eq!(
        add_and_sync(&pool, &connector, &registry, "quay-a").await,
        5
    );
    assert_eq!(
        add_and_sync(&pool, &connector, &registry, "quay-b").await,
        5
    );
    let secrets: Arc<dyn knobas_secrets::SecretStore> =
        Arc::new(knobas_secrets::MemoryStore::new());

    let asked: Vec<String> = only(&pool, &registry)
        .await
        .sources
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(
        asked,
        ["quay-a", "quay-b"],
        "the question names each source"
    );

    // One answer of two: refused, and nothing removed.
    let purge_a = Removal {
        source_id: "quay-a".to_owned(),
        purge_items: true,
    };
    let keep_b = Removal {
        source_id: "quay-b".to_owned(),
        purge_items: false,
    };
    let Err(error) = installed::uninstall(
        &pool,
        &secrets,
        &knobas_app::sources::live::LiveSources::default(),
        &registry,
        store.path(),
        "wasm-fixture",
        std::slice::from_ref(&purge_a),
    )
    .await
    else {
        panic!("an uninstall missing an answer must not happen");
    };
    assert!(matches!(error, InstallError::Unanswered { .. }), "{error}");
    assert!(
        error.to_string().contains("no answer for quay-b"),
        "{error}"
    );
    assert_eq!(source_ids(&pool).await, ["quay-a", "quay-b"]);
    assert!(builds(&registry));

    let removed = installed::uninstall(
        &pool,
        &secrets,
        &knobas_app::sources::live::LiveSources::default(),
        &registry,
        store.path(),
        "wasm-fixture",
        &[purge_a.clone(), keep_b.clone()],
    )
    .await
    .expect("every source answered");
    assert_eq!(removed, [purge_a, keep_b]);

    assert!(
        source_ids(&pool).await.is_empty(),
        "both sources are removed"
    );
    assert_eq!(
        items_of(&pool, "quay-a").await,
        0,
        "purge took quay-a's items"
    );
    assert_eq!(
        items_of(&pool, "quay-b").await,
        5,
        "keep left quay-b's items"
    );
    assert!(installed::list(&pool, &registry).await.unwrap().is_empty());
    assert!(!kinds(&registry).contains(&"wasm-fixture".to_owned()));
    assert!(!builds(&registry));
    assert!(!install::artifact_path(store.path(), &sha_of(&release)).exists());
}

// -- backup and restore (#1210) --------------------------------------------------

/// A backup service over a scratch database, reinstalling restored adapters
/// into `registry` and a store of its own.
async fn backup_service(
    label: &str,
    registry: &Registry,
    forge: &Forge,
) -> (
    Arc<knobas_app::backup::BackupState>,
    tempfile::TempDir,
    tempfile::TempDir,
) {
    let connector = knobas_db::test_util::scratch_database(label).await;
    let pool = connector.pool(2).await.unwrap();
    let archives = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let state = knobas_app::backup::BackupState::new(
        pool,
        connector,
        archives.path().to_path_buf(),
        Arc::new(knobas_secrets::MemoryStore::new()),
    )
    .with_adapters(knobas_app::backup::RestoreAdapters {
        registry: registry.clone(),
        store: store.path().to_path_buf(),
        endpoints: forge.endpoints(),
    });
    (Arc::new(state), archives, store)
}

/// The old machine: the fixture installed from the forge and a source of
/// it; answers its backup's file name and the old machine's archives.
async fn old_machine(label: &str, forge: &Forge) -> (String, tempfile::TempDir) {
    let registry = Registry::default();
    let (old, archives, store) = backup_service(label, &registry, forge).await;
    install_from(&old.pool, &registry, store.path(), forge, &forge.repo_url()).await;
    let secrets: Arc<dyn knobas_secrets::SecretStore> =
        Arc::new(knobas_secrets::MemoryStore::new());
    let input: knobas_app::sources::NewSource = serde_json::from_value(serde_json::json!({
        "id": "quay", "adapter_kind": "wasm-fixture", "display_name": "Harbour surveys",
        "base_url": "https://surveys.example", "auth_kind": "Pat", "config": { "fault": "none" },
        "secret": { "value": "t" }, "sync_interval_secs": 900, "enabled": true,
    }))
    .unwrap();
    knobas_app::sources::crud::add(&old.pool, &secrets, &registry, input)
        .await
        .unwrap();
    let archive = knobas_app::backup::export_now(&old).await.unwrap();
    (archive.file, archives)
}

#[tokio::test]
async fn a_backup_restored_on_a_fresh_profile_reinstalls_the_adapter_at_its_tag_and_hash() {
    let forge = Forge::start().await;
    let release = forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (file, old_archives) = old_machine("restore-old", &forge).await;
    // A newer release since: the restore must bring back the pinned one.
    forge.publish("v0.2.0", &newer_bytes(), |_| {}).await;

    let registry = Registry::default();
    let (fresh, archives, store) = backup_service("restore-fresh", &registry, &forge).await;
    std::fs::copy(old_archives.path().join(&file), archives.path().join(&file)).unwrap();
    knobas_app::backup::restore(&fresh, &file)
        .await
        .expect("the restore");

    let status = only(&fresh.pool, &registry).await;
    assert_eq!(status.tag.as_deref(), Some("v0.1.0"));
    assert_eq!(status.artifact_sha256, sha_of(&release));
    assert_eq!(status.refused, None);
    assert!(install::artifact_path(store.path(), &sha_of(&release)).is_file());
    assert!(builds(&registry));
    assert_eq!(source_ids(&fresh.pool).await, ["quay"]);
}

#[tokio::test]
async fn a_restore_whose_release_artifact_changed_hash_is_refused_and_the_rest_proceeds() {
    let forge = Forge::start().await;
    let release = forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (file, old_archives) = old_machine("rehash-old", &forge).await;
    // The same tag, re-published with another artifact.
    let changed = forge.publish("v0.1.0", &newer_bytes(), |_| {}).await;

    let registry = Registry::default();
    let (fresh, archives, store) = backup_service("rehash-fresh", &registry, &forge).await;
    std::fs::copy(old_archives.path().join(&file), archives.path().join(&file)).unwrap();
    knobas_app::backup::restore(&fresh, &file)
        .await
        .expect("the restore itself stands");

    let status = only(&fresh.pool, &registry).await;
    assert_eq!(
        status.refused.as_deref(),
        Some(
            format!(
                "the release's artifact SHA-256 is now {}, and the backup pinned {}",
                sha_of(&changed),
                sha_of(&release)
            )
            .as_str()
        )
    );
    assert!(std::fs::read_dir(store.path()).map_or(true, |mut d| d.next().is_none()));
    assert!(!builds(&registry));
    assert_eq!(
        source_ids(&fresh.pool).await,
        ["quay"],
        "the rest of the restore proceeds"
    );
}

/// A manifest that still names the pinned hash over an artifact that is not
/// it: the bytes are what is held to the pin.
#[tokio::test]
async fn a_restore_whose_artifact_bytes_changed_under_the_same_manifest_is_refused() {
    let forge = Forge::start().await;
    let release = forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (file, old_archives) = old_machine("rebytes-old", &forge).await;
    let pinned = sha_of(&release);
    forge
        .publish("v0.1.0", &newer_bytes(), |m| {
            m["artifact"]["sha256"] = pinned.clone().into()
        })
        .await;

    let registry = Registry::default();
    let (fresh, archives, _store) = backup_service("rebytes-fresh", &registry, &forge).await;
    std::fs::copy(old_archives.path().join(&file), archives.path().join(&file)).unwrap();
    knobas_app::backup::restore(&fresh, &file).await.unwrap();

    let refused = only(&fresh.pool, &registry)
        .await
        .refused
        .unwrap_or_default();
    assert!(
        refused.contains(&format!("and the backup pinned {pinned}")),
        "{refused}"
    );
    assert!(!builds(&registry));
}

#[tokio::test]
async fn a_share_export_carries_no_external_adapter_record() {
    let forge = Forge::start().await;
    forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let registry = Registry::default();
    let (state, archives, store) = backup_service("share-adapter", &registry, &forge).await;
    install_from(
        &state.pool,
        &registry,
        store.path(),
        &forge,
        &forge.repo_url(),
    )
    .await;
    let every = knobas_app::backup::ShareParts {
        links: true,
        assets: true,
        contexts: true,
        notes: true,
        time: true,
        sources: true,
        smart_lists: true,
        ticket_views: true,
        link_sets: true,
    };
    let share = knobas_app::backup::share_export(&state, every)
        .await
        .unwrap();
    let backup = knobas_app::backup::export_now(&state).await.unwrap();

    let tables = |file: String| {
        let path = archives.path().join(file);
        async move {
            knobas_db::backup::archive_contents(&path)
                .await
                .unwrap()
                .into_iter()
                .filter(|e| e.kind == "TABLE DATA" && e.schema == "knobas")
                .map(|e| e.name)
                .collect::<Vec<_>>()
        }
    };
    assert!(
        !tables(share.file)
            .await
            .contains(&"external_adapter".to_owned())
    );
    assert!(
        tables(backup.file)
            .await
            .contains(&"external_adapter".to_owned()),
        "the backup is what carries the record, so the share export's absence is a choice"
    );
}

// -- the real public repository (live, `#[ignore]`) --------------------------------

/// Criterion 1's real public repository: `KNOBAS_ADAPTER_LIVE_REPO` names a
/// GitHub repository whose latest release carries a `knobas-adapter.json`
/// and the fixture component it names. Off the gate, because the gate takes
/// no network; run with
/// `KNOBAS_ADAPTER_LIVE_REPO=https://github.com/<o>/<r> cargo test -p knobas-app --test it -- --ignored a_public_github_repository`.
#[tokio::test]
#[ignore = "live: reaches github.com; set KNOBAS_ADAPTER_LIVE_REPO"]
async fn a_public_github_repository_installs_its_latest_release() {
    let repo = std::env::var("KNOBAS_ADAPTER_LIVE_REPO")
        .expect("KNOBAS_ADAPTER_LIVE_REPO names the public test repository");
    let (pool, _connector) = scratch("release-live").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    let endpoints = Endpoints::default();

    let installed = install::install_from(&pool, &registry, store.path(), &endpoints, &repo, None)
        .await
        .unwrap_or_else(|error| panic!("{repo} must install: {error}"));
    assert!(installed.tag.is_some(), "a release has a tag");
    assert!(install::artifact_path(store.path(), &installed.artifact_sha256).is_file());
    let list = installed::check_updates(&pool, &registry, &endpoints)
        .await
        .unwrap();
    assert_eq!(
        list[0].update,
        Some(UpdateCheck::Current),
        "the latest is installed"
    );
}

/// An update that cannot reach its release is a failed check, not a
/// refusal: nothing was held to the checks.
#[tokio::test]
async fn an_update_that_cannot_reach_its_release_is_a_failed_check() {
    let forge = Forge::start().await;
    forge.publish("v0.1.0", &fixture_bytes(), |_| {}).await;
    let (pool, _connector) = scratch("update-unreachable").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    install_from(&pool, &registry, store.path(), &forge, &forge.repo_url()).await;
    forge.server.reset().await;

    let Err(error) = installed::update(
        &pool,
        &registry,
        store.path(),
        &forge.endpoints(),
        "wasm-fixture",
        None,
    )
    .await
    else {
        panic!("an unreachable release must not update");
    };
    assert!(
        matches!(
            only(&pool, &registry).await.update,
            Some(UpdateCheck::Failed { .. })
        ),
        "{error}"
    );
    assert!(builds(&registry));
}
