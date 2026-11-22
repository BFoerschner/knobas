//! The window's look through Tauri's own IPC pipeline (ADR-0055): `window_look_set` refuses a mode or a colour it
//! cannot keep, and keeps a good one in the profile's folder, where the next start reads it before the page paints.

use std::collections::HashMap;

use knobas_app::Profile;
use serde_json::json;
use tauri::Manager;
use tauri::ipc::CallbackFn;
use tauri::test::MockRuntime;

#[cfg(any(windows, target_os = "android"))]
const LOCAL_ORIGIN: &str = "http://tauri.localhost";
#[cfg(not(any(windows, target_os = "android")))]
const LOCAL_ORIGIN: &str = "tauri://localhost";

fn invoke(profile: Profile, body: serde_json::Value) -> Result<(), String> {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            knobas_app::commands::window::window_look_set
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    app.manage(profile);
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
            cmd: "window_look_set".to_owned(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: LOCAL_ORIGIN.parse().expect("url"),
            body: body.into(),
            headers: HashMap::new().into_iter().collect(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .map(|_| ())
    .map_err(|error| format!("{error:?}"))
}

/// A mode that is not `system`, `light` or `dark` is refused and nothing is kept; a good look is kept in the profile's
/// folder (the final suite's mutants: `window_look_set` answering `Ok` without a look survived the lib's tests).
#[test]
fn a_look_is_kept_in_the_profile_and_a_bad_one_refused() {
    let data = tempfile::tempdir().unwrap();
    let profile = Profile::new(false, data.path());
    let file = profile.dir.join(knobas_app::window_look::FILE);

    let refused = invoke(
        profile.clone(),
        json!({ "mode": "purple", "light": "#E4E8EC", "dark": "#1A2027" }),
    );
    assert!(
        refused.as_ref().is_err_and(|e| e.contains("invalid")),
        "{refused:?}"
    );
    assert!(!file.exists(), "a refused look keeps nothing");

    invoke(
        profile.clone(),
        json!({ "mode": "dark", "light": "#E4E8EC", "dark": "#1A2027" }),
    )
    .expect("a good look is kept");
    let kept =
        knobas_app::window_look::read(&profile.dir).expect("the look is in the profile's folder");
    assert_eq!(
        kept,
        knobas_app::window_look::WindowLook::new("dark", "#E4E8EC", "#1A2027").unwrap()
    );
}
