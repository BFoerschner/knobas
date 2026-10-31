//! **Installing an external adapter from a local manifest** (ADR-0028,
//! #1202), at the app's install seam: `sources::install::install` (the
//! `install_adapter` command's body) with a `knobas-adapter.json` and an
//! artifact written into a temporary release directory.
//!
//! The artifact is the fixture component `just wasm-fixture` builds. The
//! refusals a component has to cause (a descriptor that does not decode, a
//! role not namespaced to its adapter) are the fixture with one literal
//! changed in its bytes, the same length, so the component still loads and
//! the check that refuses it is the one named. Each refusal is asserted on
//! the message the reader is shown.
//!
//! A successful install writes a row, so each one runs on a scratch database
//! of its own: the fixture's kind can be installed once per database.

use std::path::{Path, PathBuf};

use knobas_app::sources::Registry;
use knobas_app::sources::install::{self, InstallError};
use knobas_wasm_host::manifest::{self, Manifest};
use sqlx::PgPool;

pub(crate) fn fixture_bytes() -> Vec<u8> {
    let path = knobas_wasm_host::fixture_component();
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "the fixture component at {} cannot be read ({e}); `just wasm-fixture` builds it",
            path.display()
        )
    })
}

/// `bytes` with the one occurrence of `from` replaced by `to`, which must
/// be as long, so offsets inside the component stay where they were.
pub(crate) fn patched(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    assert_eq!(from.len(), to.len(), "a patch keeps the length");
    let at: Vec<usize> = bytes
        .windows(from.len())
        .enumerate()
        .filter(|(_, w)| *w == from.as_bytes())
        .map(|(i, _)| i)
        .collect();
    assert_eq!(at.len(), 1, "{from:?} must occur once in the component");
    let mut out = bytes.to_vec();
    out[at[0]..at[0] + to.len()].copy_from_slice(to.as_bytes());
    out
}

/// The manifest that names `artifact` truthfully: the fixture's kind and
/// version, interface 1.4.0, its hash.
pub(crate) fn manifest_for(artifact: &[u8]) -> serde_json::Value {
    serde_json::json!({
        "adapter_kind": "wasm-fixture",
        "adapter_version": "0.1.0",
        "interface_version": "1.4.0",
        "artifact": { "file": "fixture.wasm", "sha256": manifest::sha256(artifact) },
        "hosts": ["{base_url}"],
        "limits": { "deadline_secs": 30 },
    })
}

/// A release directory holding `artifact` as `fixture.wasm` and `manifest`
/// as `knobas-adapter.json`; answers the directory and the manifest's path.
fn release(artifact: &[u8], manifest: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("fixture.wasm"), artifact).unwrap();
    let path = dir.path().join(manifest::FILE_NAME);
    std::fs::write(&path, manifest).unwrap();
    (dir, path)
}

async fn shared_pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// The message the reader is shown for an install of `artifact` under
/// `manifest`, which must be refused.
async fn refusal(artifact: &[u8], manifest: &str) -> String {
    let (_dir, path) = release(artifact, manifest);
    let store = tempfile::tempdir().unwrap();
    let pool = shared_pool().await;
    // The preview before the confirm refuses it with the same reason (#2031).
    let previewed = match preview(&Registry::default(), &path).await {
        Ok(preview) => panic!("previewed {preview:?}"),
        Err(error) => error.to_ipc().message,
    };
    match install::install(&pool, &Registry::default(), store.path(), &path).await {
        Ok(installed) => panic!("installed {installed:?}"),
        Err(error) => {
            assert!(
                std::fs::read_dir(store.path()).unwrap().next().is_none(),
                "a refused install stores nothing"
            );
            let message = error.to_ipc().message;
            assert_eq!(previewed, message, "the preview refuses as install does");
            message
        }
    }
}

/// The preview of the manifest at `path`, as `preview_install_adapter` answers it.
async fn preview(
    registry: &Registry,
    path: &Path,
) -> Result<install::AdapterPreview, InstallError> {
    install::preview_install(
        registry,
        &knobas_app::sources::remote::Endpoints::default(),
        path.to_str().unwrap(),
    )
    .await
}

async fn refusal_of(edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let artifact = fixture_bytes();
    let mut manifest = manifest_for(&artifact);
    edit(&mut manifest);
    refusal(&artifact, &manifest.to_string()).await
}

fn says(message: &str, needle: &str) {
    assert!(
        message.contains(needle),
        "{message:?} does not say {needle:?}"
    );
}

#[tokio::test]
async fn a_manifest_that_is_not_json_is_refused() {
    let message = refusal(&fixture_bytes(), "{ not json").await;
    says(&message, "install refused: the manifest is not well formed");
}

#[tokio::test]
async fn a_manifest_missing_a_field_is_refused_naming_it() {
    let message = refusal_of(|m| drop(m.as_object_mut().unwrap().remove("adapter_version"))).await;
    says(
        &message,
        "the manifest is not well formed: missing field `adapter_version`",
    );
}

#[tokio::test]
async fn a_manifest_asking_above_the_host_maximum_is_refused() {
    let message = refusal_of(|m| m["limits"] = serde_json::json!({ "memory_mib": 4_096 })).await;
    says(
        &message,
        "install refused: the manifest's limits are refused: limits.memory_mib 4096 is outside \
         what this knobas allows (1 to 1024)",
    );
}

#[tokio::test]
async fn a_manifest_field_it_does_not_know_is_refused_naming_it() {
    let message = refusal_of(|m| m["host"] = serde_json::json!(["api.example.com"])).await;
    says(&message, "unknown field `host`");
}

#[tokio::test]
async fn a_host_template_over_a_key_the_descriptor_does_not_declare_is_refused() {
    let message = refusal_of(|m| m["hosts"] = serde_json::json!(["{base_url}", "{tenant}"])).await;
    says(
        &message,
        "install refused: the manifest's host template {tenant} names no key the descriptor's \
         config_schema declares",
    );
}

#[tokio::test]
async fn a_host_entry_that_is_neither_a_host_nor_a_whole_template_is_refused() {
    for entry in [
        "https://api.example.com",
        "{tenant}.example.com",
        "*.example.com",
    ] {
        let message = refusal_of(|m| m["hosts"] = serde_json::json!([entry])).await;
        says(
            &message,
            "install refused: the manifest is not well formed: host",
        );
    }
}

#[tokio::test]
async fn an_artifact_outside_the_release_directory_is_refused() {
    let message = refusal_of(|m| m["artifact"]["file"] = "../fixture.wasm".into()).await;
    says(
        &message,
        "artifact.file \"../fixture.wasm\" must be a file name beside the manifest",
    );
}

/// Every other way out of the release directory, or into a stream of a
/// file: an absolute path, a subdirectory, a Windows drive-relative path,
/// a backslash, an NTFS stream, and the directory names.
#[tokio::test]
async fn an_artifact_file_that_is_not_a_bare_name_is_refused() {
    for file in [
        "/etc/fixture.wasm",
        "sub/fixture.wasm",
        "C:fixture.wasm",
        "..\\fixture.wasm",
        "fixture.wasm:stream",
        ".",
        "..",
        "",
    ] {
        let message = refusal_of(|m| m["artifact"]["file"] = file.into()).await;
        says(&message, "must be a file name beside the manifest");
    }
}

#[tokio::test]
async fn an_interface_version_this_knobas_cannot_host_is_refused() {
    let message = refusal_of(|m| m["interface_version"] = "2.0.0".into()).await;
    says(
        &message,
        "the manifest asks for knobas:adapter@2.0.0, and this knobas hosts knobas:adapter@1.x \
         up to 1.4.0",
    );
}

#[tokio::test]
async fn an_artifact_whose_hash_is_not_the_manifest_s_is_refused() {
    let message = refusal_of(|m| m["artifact"]["sha256"] = "0".repeat(64).into()).await;
    says(
        &message,
        &format!("and the manifest names {}", "0".repeat(64)),
    );
    says(&message, &manifest::sha256(&fixture_bytes()));
}

#[tokio::test]
async fn an_artifact_that_is_not_there_is_refused() {
    let message = refusal_of(|m| m["artifact"]["file"] = "missing.wasm".into()).await;
    says(&message, "cannot read the artifact missing.wasm");
}

#[tokio::test]
async fn an_artifact_that_is_no_knobas_adapter_is_refused() {
    let component = wat::parse_str("(component)").unwrap();
    let message = refusal(&component, &manifest_for(&component).to_string()).await;
    says(&message, "the artifact is not a knobas adapter");
}

#[tokio::test]
async fn an_artifact_built_against_another_interface_than_the_manifest_says_is_refused() {
    let component = wat::parse_str(
        r#"(component
             (instance $source)
             (export "knobas:adapter/source@2.0.0" (instance $source)))"#,
    )
    .unwrap();
    let message = refusal(&component, &manifest_for(&component).to_string()).await;
    says(
        &message,
        "the artifact is built against knobas:adapter@2.0.0, and the manifest says 1.4.0",
    );
}

#[tokio::test]
async fn a_descriptor_that_does_not_decode_is_refused() {
    let artifact = patched(&fixture_bytes(), "entity_kinds", "entity_kindz");
    let message = refusal(&artifact, &manifest_for(&artifact).to_string()).await;
    says(&message, "the adapter's descriptor is not well formed");
    says(&message, "entity_kinds");
}

#[tokio::test]
async fn a_manifest_that_names_another_kind_than_the_descriptor_is_refused() {
    let message = refusal_of(|m| m["adapter_kind"] = "harbour".into()).await;
    says(
        &message,
        "the descriptor names adapter_kind \"wasm-fixture\", and the manifest \"harbour\"",
    );
}

#[tokio::test]
async fn a_custom_role_not_namespaced_to_the_adapter_is_refused() {
    let artifact = patched(
        &fixture_bytes(),
        "wasm-fixture:survey-desk",
        "wasm-fixturz:survey-desk",
    );
    let message = refusal(&artifact, &manifest_for(&artifact).to_string()).await;
    says(
        &message,
        "the adapter's role claims are refused: role \"wasm-fixturz:survey-desk\"",
    );
}

// -- the role claims' cover: kinds, declared paths, reads ---------------------
//
// The descriptor half of the checks, over descriptors the fixture cannot be
// patched into: a claim of a built-in role. `role::check`'s own tests hold
// every variant; these hold that install runs it and says why.

fn descriptor_refusal(manifest: &Manifest, descriptor: &knobas_source::SourceDescriptor) -> String {
    match manifest::check_descriptor(manifest, descriptor) {
        Ok(()) => panic!("{descriptor:?} passed"),
        Err(refusal) => InstallError::Refused(refusal).to_ipc().message,
    }
}

fn fixture_manifest() -> Manifest {
    Manifest::parse(&manifest_for(&fixture_bytes()).to_string()).unwrap()
}

#[test]
fn a_role_whose_kind_the_descriptor_does_not_declare_is_refused() {
    let mut descriptor = knobas_wasm_host::load_fixture().template();
    descriptor.roles = vec![knobas_source::role::RoleClaim::new(
        "ticket-system",
        ["reachable_transitions"],
    )];
    says(
        &descriptor_refusal(&fixture_manifest(), &descriptor),
        "role \"ticket-system\" needs kind \"ticket\", which descriptor.entity_kinds does not \
         declare",
    );
}

#[test]
fn a_role_whose_kind_declares_no_path_it_needs_is_refused() {
    let mut descriptor = knobas_wasm_host::load_fixture().template();
    let mut ticket = descriptor.entity_kinds[0].clone();
    ticket.id = "ticket".to_owned();
    descriptor.entity_kinds.push(ticket);
    descriptor.roles = vec![knobas_source::role::RoleClaim::new(
        "ticket-system",
        ["reachable_transitions"],
    )];
    says(
        &descriptor_refusal(&fixture_manifest(), &descriptor),
        "role \"ticket-system\" needs kind \"ticket\" to declare the path",
    );
}

#[test]
fn a_role_whose_required_read_the_claim_does_not_declare_is_refused() {
    // Jira's descriptor fills the ticket-system role; without its read it
    // does not.
    let mut descriptor = knobas_source_jira::descriptor_template();
    for claim in &mut descriptor.roles {
        claim.reads.clear();
    }
    let manifest = Manifest::parse(
        &serde_json::json!({
            "adapter_kind": descriptor.adapter_kind,
            "adapter_version": descriptor.adapter_version,
            "interface_version": "1.0.0",
            "artifact": { "file": "jira.wasm", "sha256": "0".repeat(64) },
        })
        .to_string(),
    )
    .unwrap();
    says(
        &descriptor_refusal(&manifest, &descriptor),
        "role \"ticket-system\" needs the read \"reachable_transitions\", which its claim does \
         not declare",
    );
}

// -- an accepted install -------------------------------------------------------

/// A scratch database's pool and connector, for a test that installs.
pub(crate) async fn scratch(label: &str) -> (PgPool, knobas_db::embedded::Connector) {
    let connector = knobas_db::test_util::scratch_database(label).await;
    let pool = connector
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    (pool, connector)
}

pub(crate) async fn install_fixture(
    pool: &PgPool,
    registry: &Registry,
    store: &Path,
) -> install::InstalledAdapter {
    let artifact = fixture_bytes();
    let (_dir, path) = release(&artifact, &manifest_for(&artifact).to_string());
    match install::install(pool, registry, store, &path).await {
        Ok(installed) => installed,
        Err(error) => panic!("the fixture must install: {error}"),
    }
}

pub(crate) fn kinds(registry: &Registry) -> Vec<String> {
    registry
        .templates()
        .into_iter()
        .map(|t| t.adapter_kind)
        .collect()
}

/// Add a source of the fixture as the Add-source dialog does, sync it, and
/// answer how many items it stored.
pub(crate) async fn add_and_sync(
    pool: &PgPool,
    connector: &knobas_db::embedded::Connector,
    registry: &Registry,
    id: &str,
) -> u64 {
    let source = add_source(pool, registry, id, serde_json::json!({ "fault": "none" })).await;
    let mut conn = connector.connect().await.unwrap();
    knobas_sync::run_from_stored_cursor(&mut conn, pool, source.as_ref())
        .await
        .unwrap()
        .upserted
}

/// Add a source of the fixture with `config` as the Add-source dialog does,
/// and build it as the scheduler does.
async fn add_source(
    pool: &PgPool,
    registry: &Registry,
    id: &str,
    config: serde_json::Value,
) -> Box<dyn knobas_source::Source> {
    use knobas_sync::scheduler::AdapterRegistry as _;
    let secrets: std::sync::Arc<dyn knobas_secrets::SecretStore> =
        std::sync::Arc::new(knobas_secrets::MemoryStore::new());
    let input: knobas_app::sources::NewSource = serde_json::from_value(serde_json::json!({
        "id": id,
        "adapter_kind": "wasm-fixture",
        "display_name": "Harbour surveys",
        "base_url": "https://surveys.example",
        "auth_kind": "Pat",
        "config": config,
        "secret": { "value": "a token the fixture never sees" },
        "sync_interval_secs": 900,
        "enabled": true,
    }))
    .unwrap();
    let summary = knobas_app::sources::crud::add(pool, &secrets, registry, input)
        .await
        .unwrap();
    assert!(
        summary
            .roles
            .iter()
            .any(|r| r.role == "wasm-fixture:survey-desk"),
        "the source claims the fixture's custom role: {:?}",
        summary.roles
    );
    registry
        .build(knobas_source::instance::SourceInstance {
            id: id.to_owned(),
            kind: "wasm-fixture".to_owned(),
            display_name: "Harbour surveys".to_owned(),
            base_url: "https://surveys.example".to_owned(),
            auth: Some(knobas_source::AuthMethod::Pat),
            secret: None,
            account: None,
            config,
        })
        .unwrap()
}

#[tokio::test]
async fn an_installed_fixture_is_offered_syncs_and_survives_a_restart() {
    let (pool, connector) = scratch("install-restart").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();

    let installed = install_fixture(&pool, &registry, store.path()).await;
    assert_eq!(installed.adapter_kind, "wasm-fixture");
    assert_eq!(installed.interface_version, "1.4.0");
    assert_eq!(installed.tag, None, "a local manifest is no release");
    assert_eq!(installed.hosts, ["{base_url}"]);
    assert!(
        install::artifact_path(store.path(), &installed.artifact_sha256).is_file(),
        "the artifact is stored under its hash"
    );
    assert!(
        kinds(&registry).contains(&"wasm-fixture".to_owned()),
        "Add source offers it"
    );

    assert_eq!(
        add_and_sync(&pool, &connector, &registry, "surveys").await,
        5
    );

    // A restart: a registry with nothing in it, loaded from the table.
    let restarted = Registry::default();
    let loaded = install::load_installed(&pool, &restarted, store.path())
        .await
        .unwrap();
    assert_eq!(loaded, [("wasm-fixture".to_owned(), Ok(()))]);
    assert!(kinds(&restarted).contains(&"wasm-fixture".to_owned()));
    assert_eq!(
        add_and_sync(&pool, &connector, &restarted, "surveys-two").await,
        5
    );
    // The row's hosts came back with it: the restarted adapter reaches its
    // source's address, which `{base_url}` allows, and nothing else.
    let (address, served) = one_response().await;
    let reach = |fetch: String| {
        use knobas_sync::scheduler::AdapterRegistry as _;
        restarted
            .build(knobas_source::instance::SourceInstance {
                id: "surveys-three".to_owned(),
                kind: "wasm-fixture".to_owned(),
                display_name: "Harbour surveys".to_owned(),
                base_url: address.clone(),
                auth: None,
                secret: None,
                account: None,
                config: serde_json::json!({ "fetch": fetch }),
            })
            .unwrap()
    };
    let refused = reach("http://localhost:9/elsewhere".to_owned())
        .test_connection()
        .await;
    assert!(refused.is_err(), "{refused:?}");
    let info = reach(format!("{address}/ping"))
        .test_connection()
        .await
        .unwrap();
    assert!(info.detail.unwrap().contains("pong"));
    served.await.unwrap();
}

/// A local server that answers one request `pong`; answers its address and
/// the task that serves it.
async fn one_response() -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let served = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0, "the request ended early");
            buf.extend_from_slice(&chunk[..n]);
        }
        let reply = "HTTP/1.1 200 OK\r\ncontent-length: 4\r\nconnection: close\r\n\r\npong";
        socket.write_all(reply.as_bytes()).await.unwrap();
        let _ = socket.shutdown().await;
    });
    (address, served)
}

/// The restart through the app's own bring-up (`sources::bring_up`, the body
/// of `sources::start` below the database handle), not a hand-made call to
/// `load_installed`: a bring-up that skipped the load would leave an
/// installed adapter missing after every restart.
#[tokio::test(flavor = "multi_thread")]
async fn bring_up_loads_the_installed_adapters_before_the_scheduler_starts() {
    let (pool, connector) = scratch("install-bring-up").await;
    let store = tempfile::tempdir().unwrap();
    install_fixture(&pool, &Registry::default(), store.path()).await;

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    let handle = app.handle().clone();
    let sync_pool = connector.pool(2).await.unwrap();
    let deps = knobas_app::sources::BringUp {
        suggestions: knobas_app::suggestion_pass::SuggestionPass::beside(&pool),
        pool,
        sync_pool,
        connections: connector,
        secrets: std::sync::Arc::new(knobas_secrets::MemoryStore::new()),
        registry: Registry::default(),
        store: store.path().to_path_buf(),
    };
    // `block_in_place` first: `block_on` from inside the test's runtime panics.
    tokio::task::block_in_place(|| {
        tauri::async_runtime::block_on(async move {
            let state = knobas_app::sources::bring_up(&handle, deps)
                .await
                .expect("bring-up over the scratch database");
            let built = state
                .registry
                .build(knobas_source::instance::SourceInstance {
                    id: "surveys".to_owned(),
                    kind: "wasm-fixture".to_owned(),
                    display_name: "Harbour surveys".to_owned(),
                    base_url: "https://surveys.example".to_owned(),
                    auth: Some(knobas_source::AuthMethod::Pat),
                    secret: None,
                    account: None,
                    config: serde_json::json!({ "fault": "none" }),
                });
            state.scheduler.shutdown().await;
            if let Err(error) = built {
                panic!("the scheduler's registry must hold the installed fixture: {error}");
            }
        });
    });
}

#[tokio::test]
async fn the_fixture_s_custom_role_opens_its_block_view() {
    use knobas_source::view::Block;
    let (pool, connector) = scratch("install-view").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    install_fixture(&pool, &registry, store.path()).await;
    add_and_sync(&pool, &connector, &registry, "surveys").await;

    let secrets: std::sync::Arc<dyn knobas_secrets::SecretStore> =
        std::sync::Arc::new(knobas_secrets::MemoryStore::new());
    knobas_secrets::spawn::put(
        &secrets,
        &knobas_secrets::KeychainAccount::source("surveys"),
        knobas_secrets::Secret {
            kind: knobas_source::AuthMethod::Pat,
            value: "a token".to_owned(),
            account: None,
        },
    )
    .await
    .unwrap();
    let view = knobas_app::sources::crud::view(
        &pool,
        &secrets,
        &crate::support::shared(registry.clone()),
        &knobas_app::sources::live::LiveSources::default(),
        "surveys:survey-2",
        None,
    )
    .await
    .unwrap()
    .expect("a survey has a view");
    let blocks = &view.sections[0].blocks;
    assert_eq!(view.sections[0].title, "Berth 7 fender survey");
    assert!(
        matches!(&blocks[1], Block::Text { text, .. } if text.contains("berth 7 fenders")),
        "{blocks:?}"
    );
}

#[tokio::test]
async fn a_second_install_of_an_installed_kind_is_refused() {
    let (pool, _connector) = scratch("install-twice").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    install_fixture(&pool, &registry, store.path()).await;
    let artifact = fixture_bytes();
    let (_dir, path) = release(&artifact, &manifest_for(&artifact).to_string());
    let Err(error) = install::install(&pool, &registry, store.path(), &path).await else {
        panic!("a second install must be refused");
    };
    let ipc = error.to_ipc();
    assert_eq!(ipc.code, knobas_app::IpcErrorCode::Conflict);
    says(
        &ipc.message,
        "an adapter of kind \"wasm-fixture\" is already installed",
    );
}

#[tokio::test]
async fn a_stored_artifact_whose_hash_no_longer_matches_is_refused_at_load() {
    use knobas_sync::scheduler::AdapterRegistry as _;
    let (pool, _connector) = scratch("install-tampered").await;
    let store = tempfile::tempdir().unwrap();
    let installed = install_fixture(&pool, &Registry::default(), store.path()).await;
    let stored = install::artifact_path(store.path(), &installed.artifact_sha256);
    let mut tampered = std::fs::read(&stored).unwrap();
    tampered.push(0);
    std::fs::write(&stored, &tampered).unwrap();

    let restarted = Registry::default();
    let loaded = install::load_installed(&pool, &restarted, store.path())
        .await
        .unwrap();
    let reason = match &loaded[..] {
        [(kind, Err(reason))] if kind == "wasm-fixture" => reason.clone(),
        other => panic!("the tampered artifact must be refused: {other:?}"),
    };
    says(
        &reason,
        &format!("the install pinned {}", installed.artifact_sha256),
    );
    says(&reason, &manifest::sha256(&tampered));
    assert!(!kinds(&restarted).contains(&"wasm-fixture".to_owned()));
    let Err(error) = restarted.build(knobas_source::instance::SourceInstance {
        id: "surveys".to_owned(),
        kind: "wasm-fixture".to_owned(),
        display_name: "Harbour surveys".to_owned(),
        base_url: String::new(),
        auth: None,
        secret: None,
        account: None,
        config: serde_json::json!({}),
    }) else {
        panic!("a source of a refused adapter must not build");
    };
    says(&error.to_string(), "was refused at load");
}

/// The limits a manifest asks for are what every call is held to after a
/// restart (#1204): the row's `limits` come back with the adapter. A run
/// past its deadline ends failed naming it, and a source synced beside it
/// is not held up.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_past_its_deadline_ends_failed_naming_it_and_its_neighbour_syncs() {
    let (pool, connector) = scratch("install-deadline").await;
    let store = tempfile::tempdir().unwrap();
    let artifact = fixture_bytes();
    let mut asked = manifest_for(&artifact);
    asked["limits"] = serde_json::json!({ "deadline_secs": 1 });
    let (_dir, path) = release(&artifact, &asked.to_string());
    install::install(&pool, &Registry::default(), store.path(), &path)
        .await
        .unwrap();

    let restarted = Registry::default();
    install::load_installed(&pool, &restarted, store.path())
        .await
        .unwrap();
    let spinner = add_source(
        &pool,
        &restarted,
        "spinner",
        serde_json::json!({ "fault": "spin" }),
    )
    .await;
    let healthy = add_source(
        &pool,
        &restarted,
        "surveys",
        serde_json::json!({ "fault": "none" }),
    )
    .await;
    let mut spin_conn = connector.connect().await.unwrap();
    let mut sync_conn = connector.connect().await.unwrap();
    let (spun, synced) = tokio::join!(
        knobas_sync::run_from_stored_cursor(&mut spin_conn, &pool, spinner.as_ref()),
        knobas_sync::run_from_stored_cursor(&mut sync_conn, &pool, healthy.as_ref()),
    );
    let Err(error) = spun else {
        panic!("a run past its deadline must fail");
    };
    assert_eq!(
        knobas_sync::run_log::SyncOutcome::of(&error),
        knobas_sync::run_log::SyncOutcome::Error,
        "the run ends failed"
    );
    says(
        &error.to_string(),
        "the external adapter \"wasm-fixture\" was stopped at its deadline of 1 s",
    );
    assert!(synced.unwrap().upserted > 0, "the neighbour synced");
}

// -- the preview and the confirm (#2031) ---------------------------------------

async fn installed_rows(pool: &PgPool) -> i64 {
    sqlx::query_scalar("select count(*) from knobas.external_adapter")
        .fetch_one(pool)
        .await
        .unwrap()
}

fn stored(store: &Path) -> usize {
    std::fs::read_dir(store).map_or(0, Iterator::count)
}

/// The preview names what the reader decides on, the adapter's name,
/// version, roles, hosts and limits, and writes nothing: a *Cancel* after it
/// leaves no row, no artifact and no adapter on offer.
#[tokio::test]
async fn a_preview_shows_name_version_roles_hosts_and_limits_and_writes_nothing() {
    let (pool, _connector) = scratch("install-preview").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    let artifact = fixture_bytes();
    let (_dir, path) = release(&artifact, &manifest_for(&artifact).to_string());

    let preview = preview(&registry, &path).await.unwrap();
    assert_eq!(preview.adapter_kind, "wasm-fixture");
    // The fixture names itself by its kind.
    assert_eq!(preview.name, "wasm-fixture");
    assert_eq!(preview.adapter_version, "0.1.0");
    assert_eq!(preview.interface_version, "1.4.0");
    assert_eq!(preview.tag, None);
    assert_eq!(preview.artifact_sha256, manifest::sha256(&artifact));
    assert_eq!(preview.hosts, ["{base_url}"]);
    assert_eq!(
        preview
            .roles
            .iter()
            .map(|r| r.role.as_str())
            .collect::<Vec<_>>(),
        ["wasm-fixture:survey-desk", "ci"],
        "the fixture's custom role and CI"
    );
    assert_eq!(
        preview.limits,
        install::AdapterLimits {
            deadline_secs: 30,
            deadline_asked: true,
            memory_mib: 256,
            memory_asked: false,
        }
    );

    assert_eq!(installed_rows(&pool).await, 0, "no row");
    assert_eq!(stored(store.path()), 0, "no artifact");
    assert!(
        !kinds(&registry).contains(&"wasm-fixture".to_owned()),
        "not on offer"
    );
}

/// The confirm installs the release the reader was shown and no other: an
/// artifact whose hash is not the one confirmed is refused, and nothing is
/// written.
#[tokio::test]
async fn an_install_of_another_artifact_than_the_one_confirmed_is_refused() {
    let (pool, _connector) = scratch("install-pinned").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    let artifact = fixture_bytes();
    let (_dir, path) = release(&artifact, &manifest_for(&artifact).to_string());
    let shown = "0".repeat(64);

    let Err(error) = install::install_from(
        &pool,
        &registry,
        store.path(),
        &knobas_app::sources::remote::Endpoints::default(),
        path.to_str().unwrap(),
        Some(&shown),
    )
    .await
    else {
        panic!("an artifact other than the confirmed one must not install");
    };
    let ipc = error.to_ipc();
    assert_eq!(ipc.code, knobas_app::IpcErrorCode::Invalid);
    says(
        &ipc.message,
        "the release changed after it was checked: its artifact, hosts or limits are not the ones \
         shown",
    );
    assert_eq!(installed_rows(&pool).await, 0, "no row");
    assert_eq!(stored(store.path()), 0, "no artifact");

    let confirmed = preview(&registry, &path).await.unwrap().confirm_sha256;
    install::install_from(
        &pool,
        &registry,
        store.path(),
        &knobas_app::sources::remote::Endpoints::default(),
        path.to_str().unwrap(),
        Some(&confirmed),
    )
    .await
    .expect("the confirmed artifact installs");
    assert_eq!(installed_rows(&pool).await, 1);
}

/// The hosts and limits are the manifest's, not the artifact's: a manifest
/// rewritten after the preview to reach another host, or to ask for more
/// memory, around the same artifact, is refused at the confirm.
#[tokio::test]
async fn an_install_whose_manifest_widened_after_the_preview_is_refused() {
    let (pool, _connector) = scratch("install-pinned-manifest").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    let artifact = fixture_bytes();
    let (_dir, path) = release(&artifact, &manifest_for(&artifact).to_string());
    let shown = preview(&registry, &path).await.unwrap();

    let widenings: [fn(&mut serde_json::Value); 2] = [
        |m| m["hosts"] = serde_json::json!(["{base_url}", "collect.example"]),
        |m| m["limits"] = serde_json::json!({ "deadline_secs": 30, "memory_mib": 1024 }),
    ];
    for widen in widenings {
        let mut widened = manifest_for(&artifact);
        widen(&mut widened);
        std::fs::write(&path, widened.to_string()).unwrap();
        let Err(error) = install::install_from(
            &pool,
            &registry,
            store.path(),
            &knobas_app::sources::remote::Endpoints::default(),
            path.to_str().unwrap(),
            Some(&shown.confirm_sha256),
        )
        .await
        else {
            panic!("{widened} is not the manifest the reader confirmed");
        };
        says(
            &error.to_string(),
            "the release changed after it was checked",
        );
        assert_eq!(installed_rows(&pool).await, 0, "no row");
        assert_eq!(stored(store.path()), 0, "no artifact");
    }
}

#[tokio::test]
async fn a_preview_of_a_kind_already_installed_is_refused() {
    let (pool, _connector) = scratch("install-preview-taken").await;
    let store = tempfile::tempdir().unwrap();
    let registry = Registry::default();
    install_fixture(&pool, &registry, store.path()).await;
    let artifact = fixture_bytes();
    let (_dir, path) = release(&artifact, &manifest_for(&artifact).to_string());
    let Err(error) = preview(&registry, &path).await else {
        panic!("a taken kind must be refused at the preview");
    };
    let ipc = error.to_ipc();
    assert_eq!(ipc.code, knobas_app::IpcErrorCode::Conflict);
    says(
        &ipc.message,
        "an adapter of kind \"wasm-fixture\" is already installed",
    );
}
