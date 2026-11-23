//! The site commands through Tauri's own IPC pipeline (ADR-0050, decisions
//! §17): each is registered, its arguments decode, an address the site may
//! not show and an origin that is not `http(s)://host[:port]` are refused,
//! and every one answers `not_ready` before the sites are up.

use std::collections::HashMap;
use std::sync::Arc;

use knobas_app::site::SiteState;
use knobas_site::fake::FakeShell;
use knobas_site::keys::Platform;
use knobas_site::policy::{Base, SiteRules};
use knobas_site::{Config, Sites};
use serde_json::json;
use tauri::Manager;
use tauri::ipc::CallbackFn;
use tauri::test::MockRuntime;

#[cfg(any(windows, target_os = "android"))]
const LOCAL_ORIGIN: &str = "http://tauri.localhost";
#[cfg(not(any(windows, target_os = "android")))]
const LOCAL_ORIGIN: &str = "tauri://localhost";

fn sites() -> Sites {
    let sites = Sites::new(
        Arc::new(FakeShell::new()),
        Config {
            profile_dir: std::env::temp_dir()
                .join(format!("knobas-site-ipc-{}", std::process::id())),
            keychain_service: "dev.knobas.site-ipc".into(),
            downloads_dir: None,
            devtools: false,
            platform: Platform::Mac,
        },
    );
    sites.set_rules(HashMap::from([(
        "jira".to_owned(),
        SiteRules {
            base: Base::parse("https://jira.example").unwrap(),
            allowed: vec![],
            others: vec![],
            confined: false,
        },
    )]));
    sites
}

fn invoke(
    cmd: &str,
    body: serde_json::Value,
    up: bool,
) -> Result<tauri::ipc::InvokeResponseBody, String> {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            knobas_app::commands::site::site_open,
            knobas_app::commands::site::site_show,
            knobas_app::commands::site::site_hide,
            knobas_app::commands::site::site_do,
            knobas_app::commands::site::site_close,
            knobas_app::commands::site::site_allow,
            knobas_app::commands::site::site_disallow,
            knobas_app::commands::site::site_suggest,
            knobas_app::commands::site::site_reveal_download,
            knobas_app::commands::site::site_zoom,
            knobas_app::commands::site::site_fill,
            knobas_app::commands::site::sites,
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    app.manage(knobas_app::commands::app::Lifecycle::new());
    if up {
        app.manage(SiteState { sites: sites() });
    }
    let webview = tauri::WebviewWindowBuilder::<MockRuntime, _>::new(
        &app,
        "main",
        tauri::WebviewUrl::default(),
    )
    .build()
    .expect("mock webview");
    let outcome = tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: cmd.to_owned(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: LOCAL_ORIGIN.parse().expect("url"),
            body: body.into(),
            headers: HashMap::new().into_iter().collect(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .map_err(|error| format!("{error:?}"));
    if let Err(rejection) = &outcome {
        assert!(
            !rejection.contains("not allowed"),
            "{cmd} was refused before decoding: {rejection}"
        );
    }
    outcome
}

const INSETS: serde_json::Value = serde_json::Value::Null;

fn insets() -> serde_json::Value {
    json!({ "top": 93.0, "right": 0.0, "bottom": 28.0, "left": 0.0 })
}

fn code_of(rejection: &str) -> &str {
    ["invalid", "not_ready", "not_found", "internal"]
        .into_iter()
        .find(|code| rejection.contains(code))
        .unwrap_or(rejection)
}

#[test]
fn every_command_decodes_its_arguments() {
    let _ = INSETS;
    for (cmd, body) in [
        (
            "site_show",
            json!({ "source": "jira", "insets": insets(), "focus": true, "esc": true }),
        ),
        ("site_hide", json!({ "source": "jira" })),
        ("site_zoom", json!({ "level": 1.25 })),
        ("sites", json!({})),
        (
            "site_suggest",
            json!({ "id": "jira", "origin": "https://sso.example" }),
        ),
    ] {
        assert!(invoke(cmd, body, true).is_ok(), "{cmd}");
    }
    // The act's name on the wire is kebab-case.
    let answer = invoke(
        "site_do",
        json!({ "source": "jira", "act": "focus-shell", "force": false }),
        true,
    );
    assert_eq!(
        answer.map_err(|e| code_of(&e).to_owned()).err().as_deref(),
        Some("not_found"),
        "decoded, and the site is not open"
    );
}

#[test]
fn an_address_the_site_may_not_show_is_invalid() {
    for url in [
        "https://evil.test/",
        "javascript:alert(1)",
        "file:///etc/passwd",
        "http://jira.example/",
    ] {
        let answer = invoke(
            "site_open",
            json!({ "source": "jira", "url": url, "insets": insets(), "force": false }),
            true,
        );
        assert_eq!(code_of(&answer.expect_err(url)), "invalid", "{url}");
    }
    let answer = invoke(
        "site_open",
        json!({ "source": "gitea", "url": null, "insets": insets(), "force": false }),
        true,
    );
    assert_eq!(
        code_of(&answer.expect_err("off")),
        "invalid",
        "a source whose pages do not open inside knobas"
    );
}

#[test]
fn an_origin_with_a_path_a_user_or_another_scheme_is_invalid() {
    for origin in [
        "https://sso.example/path",
        "https://user@sso.example",
        "ftp://sso.example",
        "sso.example",
    ] {
        for cmd in ["site_allow", "site_disallow", "site_suggest"] {
            let answer = invoke(cmd, json!({ "id": "jira", "origin": origin }), true);
            assert_eq!(
                code_of(&answer.expect_err(origin)),
                "invalid",
                "{cmd} {origin}"
            );
        }
    }
}

#[test]
fn a_download_knobas_never_issued_is_invalid() {
    let answer = invoke("site_reveal_download", json!({ "id": 7 }), true);
    assert_eq!(code_of(&answer.expect_err("unknown id")), "invalid");
}

#[test]
fn before_the_sites_are_up_every_command_is_not_ready() {
    for (cmd, body) in [
        (
            "site_open",
            json!({ "source": "jira", "url": null, "insets": insets(), "force": false }),
        ),
        ("site_hide", json!({ "source": "jira" })),
        ("sites", json!({})),
        ("site_close", json!({ "source": "jira", "force": false })),
    ] {
        let answer = invoke(cmd, body, false);
        assert_eq!(code_of(&answer.expect_err(cmd)), "not_ready", "{cmd}");
    }
}

/// *Fill sign-in* for a site that is not open says so: nothing was filled.
#[test]
fn filling_a_site_that_is_not_open_is_not_found() {
    let answer = invoke("site_fill", json!({ "source": "jira" }), true);
    assert_eq!(code_of(&answer.expect_err("jira is not open")), "not_found");
}

/// Settings' Site section reads every source's setting through the command, over the database bring-up installed
/// (the final suite's mutants: `site_settings` answering nothing, and `sources` answering no source or a made-up one,
/// survived the lib's tests).
#[tokio::test(flavor = "multi_thread")]
async fn the_site_settings_name_every_source_with_what_is_stored() {
    let pool = knobas_db::test_util::scratch_database("site-settings-ipc")
        .await
        .pool(2)
        .await
        .expect("a pool onto this test's own database");
    crate::support::source_config(&pool, "jira", "jira", "Jira", &json!({})).await;
    crate::support::source_config(&pool, "wiki", "confluence", "Wiki", &json!({})).await;
    knobas_app::site::set_open_inside(&pool, "wiki", true)
        .await
        .expect("open inside");
    let lifecycle = knobas_app::commands::app::Lifecycle::new();
    lifecycle.install(knobas_app::AppState::over_pool(pool.clone()));
    lifecycle.set(knobas_app::DbState::Ready);
    // Off the test's runtime: the command runs on Tauri's, and this thread waits for it.
    let answer = tokio::task::spawn_blocking(move || {
        let app = tauri::test::mock_builder()
            .invoke_handler(tauri::generate_handler![
                knobas_app::commands::site::site_settings
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app");
        app.manage(lifecycle);
        let webview = tauri::WebviewWindowBuilder::<MockRuntime, _>::new(
            &app,
            "main",
            tauri::WebviewUrl::default(),
        )
        .build()
        .expect("mock webview");
        tauri::test::get_ipc_response(
            &webview,
            tauri::webview::InvokeRequest {
                cmd: "site_settings".to_owned(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: LOCAL_ORIGIN.parse().expect("url"),
                body: json!({}).into(),
                headers: HashMap::new().into_iter().collect(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        )
        .map_err(|error| format!("{error:?}"))
        .and_then(|body| {
            body.deserialize::<serde_json::Value>()
                .map_err(|e| e.to_string())
        })
    })
    .await
    .unwrap()
    .expect("site_settings answers");
    let rows: Vec<(String, bool, String)> = answer
        .as_array()
        .expect("a list")
        .iter()
        .map(|row| {
            (
                row["source"].as_str().unwrap_or_default().to_owned(),
                row["open_inside"].as_bool().unwrap_or_default(),
                row["base"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    // A site opens inside only where sites are offered (macOS and Windows):
    // on Linux the stored `true` reads off.
    assert_eq!(
        rows,
        [
            ("jira".to_owned(), false, "http://x".to_owned()),
            (
                "wiki".to_owned(),
                knobas_site::OFFERED,
                "http://x".to_owned()
            ),
        ]
    );
}

/// A password manager's site signs in with the source's own username and password where its adapter declares that
/// (ux30): Pleasant's does. *Turn off* stops the fill until it is turned on again (the final suite's mutants: the
/// write of *Turn off* and the source's sign-in method each survived the lib's tests).
#[tokio::test(flavor = "multi_thread")]
async fn a_site_fills_its_sources_own_credential_until_that_is_turned_off() {
    use knobas_secrets::{KeychainAccount, Secret};

    let pool = knobas_db::test_util::scratch_database("site-own-credential")
        .await
        .pool(2)
        .await
        .expect("a pool onto this test's own database");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('vault', 'pleasant', 'Team passwords', 'https://vault.example', 'user_password', $1)",
    )
    .bind(json!({ "username": "mara.lindqvist" }))
    .execute(&pool)
    .await
    .expect("a Pleasant source");
    let secrets: Arc<dyn knobas_secrets::SecretStore> =
        Arc::new(knobas_secrets::MemoryStore::new());
    secrets
        .put(
            &KeychainAccount::source("vault"),
            &Secret::just(
                knobas_source::AuthMethod::UserPassword,
                "the reader's password",
            ),
        )
        .unwrap();
    let registry = knobas_app::sources::Registry::default();

    let fill = knobas_app::site::sign_in_for_fill(&pool, &secrets, &registry, "vault")
        .await
        .expect("read");
    let fill = fill.expect("the source's own credential");
    assert_eq!(fill.username, "mara.lindqvist");
    assert_eq!(fill.password, "the reader's password");
    // Settings names it, on.
    let own = |settings: Vec<knobas_app::site::SiteSetting>| {
        settings
            .into_iter()
            .find(|s| s.source == "vault")
            .and_then(|s| s.own_sign_in)
            .map(|own| (own.user, own.on))
    };
    let reads = || knobas_app::site::SignInReads {
        secrets: &secrets,
        registry: &registry,
    };
    let sites = sites();
    let settings = knobas_app::site::settings_and_rules(&pool, Some(&sites), Some(reads()))
        .await
        .expect("settings");
    assert_eq!(own(settings), Some(("mara.lindqvist".to_owned(), true)));
    assert!(
        sites.sign_in_rule("vault").is_some_and(|rule| rule.saved),
        "the source's own credential, on, is a saved sign-in to the site"
    );

    knobas_app::site::set_sign_in_off(&pool, "vault", true)
        .await
        .expect("turn off");
    assert!(
        knobas_app::site::sign_in_for_fill(&pool, &secrets, &registry, "vault")
            .await
            .expect("read")
            .is_none(),
        "turned off, nothing is filled"
    );
    let settings = knobas_app::site::settings_and_rules(&pool, Some(&sites), Some(reads()))
        .await
        .expect("settings");
    assert_eq!(own(settings), Some(("mara.lindqvist".to_owned(), false)));
    assert!(
        sites.sign_in_rule("vault").is_some_and(|rule| !rule.saved),
        "turned off, the site has no sign-in saved"
    );
    knobas_app::site::set_sign_in_off(&pool, "vault", false)
        .await
        .expect("turn on");
    assert!(
        knobas_app::site::sign_in_for_fill(&pool, &secrets, &registry, "vault")
            .await
            .expect("read")
            .is_some(),
        "turned on again, the credential is filled"
    );
}

/// Before the adapters' descriptors are read, a site's role is unknown and it is confined; once they are, a source
/// that is no password manager is not. Each site's rules name every other site that opens inside, and not itself
/// (the ux30 end gate's mutants: the confinement's `||` turned `&&`, and the others' filter turned around, went
/// unseen).
#[tokio::test(flavor = "multi_thread")]
async fn a_site_is_confined_until_its_role_is_read_and_its_rules_name_the_others() {
    let pool = knobas_db::test_util::scratch_database("site-confined-others")
        .await
        .pool(2)
        .await
        .expect("a pool onto this test's own database");
    crate::support::source_config(&pool, "jira", "jira", "Jira", &json!({})).await;
    crate::support::source_config(&pool, "wiki", "confluence", "Wiki", &json!({})).await;
    for source in ["jira", "wiki"] {
        knobas_app::site::set_open_inside(&pool, source, true)
            .await
            .expect("open inside");
    }
    let sites = sites();
    let settings = knobas_app::site::settings_and_rules(&pool, Some(&sites), None)
        .await
        .expect("settings");
    assert!(
        settings.iter().all(|setting| setting.confined),
        "unknown roles are confined"
    );
    for (source, other) in [("jira", "wiki"), ("wiki", "jira")] {
        let rules = sites.rules(source).expect("its rules");
        assert!(rules.confined, "{source}");
        assert_eq!(
            rules
                .others
                .iter()
                .map(|o| o.source.as_str())
                .collect::<Vec<_>>(),
            [other],
            "{source}'s others"
        );
    }

    let secrets: Arc<dyn knobas_secrets::SecretStore> =
        Arc::new(knobas_secrets::MemoryStore::new());
    let registry = knobas_app::sources::Registry::default();
    let reads = knobas_app::site::SignInReads {
        secrets: &secrets,
        registry: &registry,
    };
    let settings = knobas_app::site::settings_and_rules(&pool, Some(&sites), Some(reads))
        .await
        .expect("settings");
    assert!(
        settings.iter().all(|setting| !setting.confined),
        "no password manager, read: not confined"
    );
    assert!(!sites.rules("wiki").expect("its rules").confined);
}
