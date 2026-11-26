//! External URL handoff, `knobas://` deep links, one-shot *Open in browser*
//! bypass, and Firefox Native Messaging stdio framing + bridge (ADR-0061, #2530).

use std::io::Cursor;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use knobas_app::IpcErrorCode;
use knobas_app::link_handoff::bridge::{BRIDGE_ENV, BridgeServer};
use knobas_app::link_handoff::bypass::BypassStore;
use knobas_app::link_handoff::deep_link::{find_deep_link_in_args, parse_deep_link};
use knobas_app::link_handoff::manifest::{
    HostManifest, ManifestTarget, install_in, status_in, uninstall_in,
};
use knobas_app::link_handoff::native_host::{ARG as NATIVE_HOST_ARG, dispatch_to_bridge};
use knobas_app::link_handoff::protocol::{
    EXTENSION_ID, HOST_NAME, HandoffAction, HandoffOpenedTarget, HostRequest, HostResponse,
    IgnoreReason, PROTOCOL_VERSION, read_frame, serve_stdio, write_frame,
};
use knobas_app::link_handoff::{
    LinkHandoffScope, LinkHandoffTarget, evaluate_native_handoff, export_rules, forget_settings,
    read_setting, resolve_deep_link, write_setting,
};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn pool(label: &str) -> PgPool {
    knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database")
}

async fn configure(
    pool: &PgPool,
    id: &str,
    adapter_kind: &str,
    display_name: &str,
    base_url: &str,
) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, $2, $3, $4, 'pat')",
    )
    .bind(id)
    .bind(adapter_kind)
    .bind(display_name)
    .bind(base_url)
    .execute(pool)
    .await
    .expect("the source row");
}

async fn mirror(pool: &PgPool, entity_id: &str, kind: &str, source_id: &str, web_url: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
        .bind(entity_id)
        .bind(kind)
        .bind("Sample item")
        .execute(pool)
        .await
        .expect("the entity row");
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload, web_url)
         values ($1, $2, $3, $4, '{}'::jsonb, $5)",
    )
    .bind(entity_id)
    .bind(source_id)
    .bind(kind)
    .bind("Sample item")
    .bind(web_url)
    .execute(pool)
    .await
    .expect("the mirror row");
}

#[tokio::test]
async fn per_source_settings_persist_and_rule_export_includes_enabled_sources_and_url_forms() {
    let pool = pool("lh_settings_and_rules").await;
    let registry = knobas_app::sources::Registry::for_process();

    configure(
        &pool,
        "jira",
        "jira",
        "Tidewater Jira",
        "https://JIRA.example/",
    )
    .await;
    configure(
        &pool,
        "confluence",
        "confluence",
        "Tidewater Wiki",
        "https://wiki.example/confluence/",
    )
    .await;

    // Defaults: disabled, known_items.
    let initial = read_setting(&pool, "jira").await.unwrap();
    assert!(!initial.enabled);
    assert_eq!(initial.scope, LinkHandoffScope::KnownItems);
    assert!(export_rules(&pool, &registry).await.unwrap().is_empty());

    // Unknown source refuses writes with NotFound.
    let err = write_setting(&pool, "missing", true, LinkHandoffScope::KnownItems)
        .await
        .unwrap_err();
    assert_eq!(err.code, IpcErrorCode::NotFound);

    // Enable Jira (KnownItems) and Confluence (WholeSource + open_inside).
    let saved_jira = write_setting(&pool, "jira", true, LinkHandoffScope::KnownItems)
        .await
        .unwrap();
    assert!(saved_jira.enabled);
    assert_eq!(saved_jira.scope, LinkHandoffScope::KnownItems);

    write_setting(&pool, "confluence", true, LinkHandoffScope::WholeSource)
        .await
        .unwrap();
    knobas_app::site::set_open_inside(&pool, "confluence", true)
        .await
        .unwrap();

    let rules = export_rules(&pool, &registry).await.unwrap();
    assert_eq!(rules.len(), 2);

    let jira_rule = rules.iter().find(|r| r.source_id == "jira").unwrap();
    assert_eq!(jira_rule.base_url, "https://jira.example");
    assert_eq!(jira_rule.scope, LinkHandoffScope::KnownItems);
    assert!(!jira_rule.open_inside);

    let wiki_rule = rules.iter().find(|r| r.source_id == "confluence").unwrap();
    assert_eq!(wiki_rule.base_url, "https://wiki.example/confluence");
    assert_eq!(wiki_rule.scope, LinkHandoffScope::WholeSource);
    assert_eq!(wiki_rule.open_inside, knobas_site::OFFERED);
    assert!(
        wiki_rule
            .url_forms
            .iter()
            .any(|t| t.contains("viewpage.action?pageId={}")),
        "Confluence url_forms exported: {:?}",
        wiki_rule.url_forms
    );

    // Deleting a source cleans up its link handoff setting.
    forget_settings(&pool, "jira").await.unwrap();
    assert!(!read_setting(&pool, "jira").await.unwrap().enabled);
}

#[tokio::test]
async fn deep_link_dispatch_resolves_entities_and_whole_source_sites() {
    let pool = pool("lh_deep_link").await;
    let registry = knobas_app::sources::Registry::for_process();

    configure(&pool, "jira", "jira", "Jira", "https://jira.example").await;
    configure(&pool, "wiki", "confluence", "Wiki", "https://wiki.example").await;
    mirror(
        &pool,
        "jira:PAY-231",
        "ticket",
        "jira",
        "https://jira.example/browse/PAY-231",
    )
    .await;

    // 1. Parse argv carrying `knobas://open?url=...` and resolve to the native ticket view.
    let argv = [
        "knobas".to_owned(),
        "knobas://open?url=https%3A%2F%2FJIRA.example%2Fbrowse%2FPAY-231%2F%23comment-1".to_owned(),
    ];
    let target_url = find_deep_link_in_args(&argv).expect("deep link in argv");
    let target = resolve_deep_link(&pool, &registry, &target_url)
        .await
        .unwrap();
    assert_eq!(
        target,
        LinkHandoffTarget::Entity {
            entity_id: "jira:PAY-231".to_owned(),
            entity_kind: "ticket".to_owned(),
            url: target_url,
        }
    );

    // 2. Non-entity URL under a WholeSource source with site open_inside enabled routes to Site.
    write_setting(&pool, "wiki", true, LinkHandoffScope::WholeSource)
        .await
        .unwrap();
    knobas_app::site::set_open_inside(&pool, "wiki", true)
        .await
        .unwrap();

    let board_url =
        parse_deep_link("knobas://open?url=https%3A%2F%2Fwiki.example%2Fspaces%2FENG%2Foverview")
            .unwrap();
    let site_target = resolve_deep_link(&pool, &registry, &board_url)
        .await
        .unwrap();
    if knobas_site::OFFERED {
        assert_eq!(
            site_target,
            LinkHandoffTarget::Site {
                source: "wiki".to_owned(),
                url: board_url,
            }
        );
    } else {
        assert_eq!(site_target, LinkHandoffTarget::Unmatched { url: board_url });
    }

    // 3. Unknown URL outside WholeSource resolves to Unmatched.
    let unknown = resolve_deep_link(
        &pool,
        &registry,
        "https://jira.example/secure/Dashboard.jspa",
    )
    .await
    .unwrap();
    assert_eq!(
        unknown,
        LinkHandoffTarget::Unmatched {
            url: "https://jira.example/secure/Dashboard.jspa".to_owned()
        }
    );
}

#[tokio::test]
async fn native_handoff_respects_one_shot_bypass_and_scopes() {
    let pool = pool("lh_bypass_and_scope").await;
    let registry = knobas_app::sources::Registry::for_process();
    let bypass = BypassStore::new();

    configure(&pool, "jira", "jira", "Jira", "https://jira.example").await;
    mirror(
        &pool,
        "jira:PAY-231",
        "ticket",
        "jira",
        "https://jira.example/browse/PAY-231",
    )
    .await;

    // Source not opted in yet -> ignored.
    let (action, nav) = evaluate_native_handoff(
        &pool,
        &registry,
        &bypass,
        "https://jira.example/browse/PAY-231",
    )
    .await
    .unwrap();
    assert_eq!(
        action,
        HandoffAction::Ignore {
            reason: IgnoreReason::NoMatchingSource,
            source_id: None
        }
    );
    assert!(nav.is_none());

    // Opt in as KnownItems.
    write_setting(&pool, "jira", true, LinkHandoffScope::KnownItems)
        .await
        .unwrap();

    // Non-entity URL under KnownItems -> ignored as NotKnownItem.
    let (action, nav) = evaluate_native_handoff(
        &pool,
        &registry,
        &bypass,
        "https://jira.example/secure/Dashboard.jspa",
    )
    .await
    .unwrap();
    assert_eq!(
        action,
        HandoffAction::Ignore {
            reason: IgnoreReason::NotKnownItem,
            source_id: Some("jira".to_owned())
        }
    );
    assert!(nav.is_none());

    // Record one-shot *Open in browser* bypass -> first handoff returns Bypass (no navigation).
    assert!(bypass.record("https://jira.example/browse/PAY-231/#comment-9"));
    let (action, nav) = evaluate_native_handoff(
        &pool,
        &registry,
        &bypass,
        "https://JIRA.example/browse/PAY-231",
    )
    .await
    .unwrap();
    assert_eq!(action, HandoffAction::Bypass);
    assert!(nav.is_none());

    // Second handoff for the same URL opens the entity (bypass was one-shot).
    let (action, nav) = evaluate_native_handoff(
        &pool,
        &registry,
        &bypass,
        "https://jira.example/browse/PAY-231",
    )
    .await
    .unwrap();
    assert_eq!(
        action,
        HandoffAction::Opened {
            target: HandoffOpenedTarget::Entity,
            source_id: "jira".to_owned(),
            entity_id: Some("jira:PAY-231".to_owned()),
            entity_kind: Some("ticket".to_owned()),
            url: None,
        }
    );
    assert_eq!(
        nav,
        Some(LinkHandoffTarget::Entity {
            entity_id: "jira:PAY-231".to_owned(),
            entity_kind: "ticket".to_owned(),
            url: "https://jira.example/browse/PAY-231".to_owned(),
        })
    );
}

fn frame_json(value: &Value) -> Vec<u8> {
    let mut buf = Vec::new();
    let payload = serde_json::to_vec(value).unwrap();
    write_frame(&mut buf, &payload).unwrap();
    buf
}

/// Multi-thread on purpose: the bridge's worker thread `block_on`s database
/// futures on this runtime while the test waits on the child process. On a
/// current-thread runtime that wait parks the only thread that drives the
/// pool's sockets, the bridge worker never finishes, and `server.stop()` joins
/// it for ever (the hang #2530's first run met twice).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_messaging_host_binary_speaks_stdio_framing_over_authenticated_bridge() {
    let pool = pool("lh_native_stdio").await;
    let registry = Arc::new(knobas_app::sources::Registry::for_process());
    let bypass = Arc::new(BypassStore::new());
    let opened: Arc<Mutex<Vec<LinkHandoffTarget>>> = Arc::new(Mutex::new(Vec::new()));

    configure(
        &pool,
        "jira",
        "jira",
        "Tidewater Jira",
        "https://jira.example",
    )
    .await;
    mirror(
        &pool,
        "jira:PAY-231",
        "ticket",
        "jira",
        "https://jira.example/browse/PAY-231",
    )
    .await;
    write_setting(&pool, "jira", true, LinkHandoffScope::KnownItems)
        .await
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let endpoint_file = dir.path().join("link-handoff.json");
    let handle = tokio::runtime::Handle::current();

    let server_pool = pool.clone();
    let server_reg = Arc::clone(&registry);
    let server_bypass = Arc::clone(&bypass);
    let server_opened = Arc::clone(&opened);
    let server = BridgeServer::start(endpoint_file.clone(), move |req, _left| match req {
        HostRequest::Ping => HostResponse::pong(true),
        HostRequest::GetRules => {
            match handle.block_on(export_rules(&server_pool, server_reg.as_ref())) {
                Ok(rules) => HostResponse::rules(rules),
                Err(error) => HostResponse::error("internal", error.message),
            }
        }
        HostRequest::CheckBypass { url, consume } => {
            HostResponse::bypass(server_bypass.check(&url, consume))
        }
        HostRequest::Handoff { url } => {
            match handle.block_on(evaluate_native_handoff(
                &server_pool,
                server_reg.as_ref(),
                &server_bypass,
                &url,
            )) {
                Ok((action, maybe_target)) => {
                    if let Some(target) = maybe_target {
                        server_opened.lock().unwrap().push(target);
                    }
                    HostResponse::handoff(action)
                }
                Err(error) => HostResponse::error("invalid_url", error.message),
            }
        }
    })
    .unwrap();

    // Seed a one-shot bypass for a URL before spawning the native-messaging child.
    bypass.record("https://jira.example/browse/PAY-231");

    // Drive the actual compiled `knobas-app native-messaging-host` binary over
    // stdin/stdout pipes, off the runtime's threads, with a deadline.
    let child_endpoint = endpoint_file.clone();
    let output = tokio::task::spawn_blocking(move || run_host_child(&child_endpoint))
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut cursor = Cursor::new(output.stdout);
    let mut replies = Vec::new();
    while let Some(frame) = read_frame(&mut cursor).unwrap() {
        replies.push(serde_json::from_slice::<Value>(&frame).unwrap());
    }
    assert_eq!(replies.len(), 5, "all 5 requests answered: {replies:?}");

    assert_eq!(replies[0]["id"], 1);
    assert_eq!(replies[0]["type"], "pong");
    assert_eq!(replies[0]["protocol_version"], PROTOCOL_VERSION);
    assert_eq!(replies[0]["running"], true);

    assert_eq!(replies[1]["id"], 2);
    assert_eq!(replies[1]["type"], "rules");
    assert_eq!(replies[1]["rules"].as_array().unwrap().len(), 1);
    assert_eq!(replies[1]["rules"][0]["source_id"], "jira");

    assert_eq!(replies[2]["id"], 3);
    assert_eq!(replies[2]["bypassed"], true);

    // Request 4 consumes the one-shot bypass.
    assert_eq!(replies[3]["id"], 4);
    assert_eq!(replies[3]["action"], "bypass");

    // Request 5 opens the ticket inside knobas.
    assert_eq!(replies[4]["id"], 5);
    assert_eq!(replies[4]["action"], "opened");
    assert_eq!(replies[4]["target"], "entity");
    assert_eq!(replies[4]["entity_id"], "jira:PAY-231");
    assert_eq!(opened.lock().unwrap().len(), 1);

    // Stop the bridge and verify fallback when knobas is closed.
    tokio::task::spawn_blocking(move || server.stop())
        .await
        .unwrap();
    let mut offline_in = Vec::new();
    offline_in.extend_from_slice(&frame_json(&json!({ "id": 10, "type": "ping" })));
    offline_in.extend_from_slice(&frame_json(&json!({ "id": 11, "type": "get_rules" })));
    let mut offline_out = Vec::new();
    serve_stdio(&mut Cursor::new(offline_in), &mut offline_out, |req| {
        dispatch_to_bridge(Some(&endpoint_file), req)
    })
    .unwrap();
    let mut off_cursor = Cursor::new(offline_out);
    let p: Value = serde_json::from_slice(&read_frame(&mut off_cursor).unwrap().unwrap()).unwrap();
    let r: Value = serde_json::from_slice(&read_frame(&mut off_cursor).unwrap().unwrap()).unwrap();
    assert_eq!(p["ok"], true);
    assert_eq!(p["running"], false);
    assert_eq!(r["ok"], false);
    assert_eq!(r["error"], "knobas_not_running");
}

/// Spawn `knobas-app native-messaging-host`, write five framed requests, close
/// stdin, and collect its output. Kills the child and panics after 60 s rather
/// than hanging the test binary.
fn run_host_child(endpoint_file: &std::path::Path) -> std::process::Output {
    use std::io::{Read as _, Write as _};
    use std::time::{Duration, Instant};

    let mut child = Command::new(env!("CARGO_BIN_EXE_knobas-app"))
        .arg(NATIVE_HOST_ARG)
        .env(BRIDGE_ENV, endpoint_file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn knobas-app native-messaging-host");
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    let mut stdin = child.stdin.take().unwrap();
    for request in [
        json!({ "id": 1, "type": "ping" }),
        json!({ "id": 2, "type": "get_rules" }),
        json!({
            "id": 3,
            "type": "check_bypass",
            "url": "https://jira.example/browse/PAY-231",
            "consume": false
        }),
        json!({ "id": 4, "type": "handoff", "url": "https://jira.example/browse/PAY-231" }),
        json!({ "id": 5, "type": "handoff", "url": "https://jira.example/browse/PAY-231" }),
    ] {
        stdin.write_all(&frame_json(&request)).unwrap();
    }
    drop(stdin);

    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native-messaging-host did not exit within 60 s of stdin EOF");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    std::process::Output {
        status,
        stdout: out_reader.join().unwrap(),
        stderr: err_reader.join().unwrap(),
    }
}

#[test]
fn manifest_installer_writes_exact_firefox_native_messaging_schema() {
    let dir = tempfile::tempdir().unwrap();
    let manifest_path = dir
        .path()
        .join("Mozilla")
        .join("NativeMessagingHosts")
        .join(format!("{HOST_NAME}.json"));
    let target = ManifestTarget {
        manifest_path: manifest_path.clone(),
        write_registry: false,
    };
    let exe = PathBuf::from("/Applications/knobas.app/Contents/MacOS/knobas");

    let installed = install_in(&target, &exe).unwrap();
    assert!(installed.installed);
    assert!(installed.matches_current);
    assert_eq!(installed.host_name, HOST_NAME);
    assert_eq!(installed.extension_id, EXTENSION_ID);

    let on_disk: HostManifest =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    assert_eq!(on_disk.name, HOST_NAME);
    assert_eq!(on_disk.kind, "stdio");
    assert_eq!(on_disk.path, exe.to_string_lossy());
    assert_eq!(on_disk.allowed_extensions, vec![EXTENSION_ID.to_owned()]);

    let removed = uninstall_in(&target, &exe).unwrap();
    assert!(!removed.installed);
    assert!(!status_in(Some(&target), &exe).installed);
}
