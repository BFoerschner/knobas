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
            server: MockServer::start().await,
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
                { "name": "fixture.wasm", "browser_download_url": self.download(&r.tag, "fixture.wasm") },
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
    install::install_from(pool, registry, store, &forge.endpoints(), input)
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
        ("/adapters/knobas-adapter.json", manifest_for(&artifact).to_string().into_bytes()),
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
    assert_eq!(installed.artifact_sha256, knobas_wasm_host::manifest::sha256(&artifact));
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
    let Err(error) =
        install::install_from(&pool, &Registry::default(), store.path(), &forge.endpoints(), &forge.repo_url()).await
    else {
        panic!("a release with no manifest must not install");
    };
    assert_eq!(error.to_ipc().message, "the release v1 has no knobas-adapter.json");
}

// -- the update check and update (#1209) ---------------------------------------

fn sha_of(release: &Published) -> String {
    release.manifest["artifact"]["sha256"].as_str().unwrap().to_owned()
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

    let list = installed::check_updates(&pool, &registry, &forge.endpoints()).await.unwrap();
    assert_eq!(list[0].update, Some(UpdateCheck::Current));

    let newer = forge.publish("v0.2.0", &newer_bytes(), |_| {}).await;
    let list = installed::check_updates(&pool, &registry, &forge.endpoints()).await.unwrap();
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

    let updated = installed::update(&pool, &registry, store.path(), &forge.endpoints(), "wasm-fixture")
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
    let loaded = install::load_installed(&pool, &restarted, store.path()).await.unwrap();
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
    let list = installed::check_updates(&pool, &registry, &forge.endpoints()).await.unwrap();
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
        .publish("v0.2.0", &newer_bytes(), |m| m["interface_version"] = "2.0.0".into())
        .await;
    installed::check_updates(&pool, &registry, &forge.endpoints()).await.unwrap();

    let Err(error) =
        installed::update(&pool, &registry, store.path(), &forge.endpoints(), "wasm-fixture").await
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
