//! **A stop signal is a quit** (the ux30 end gate, 2026-11-23): a `SIGTERM`
//! to knobas stopped the process and left its embedded PostgreSQL running, on
//! Linux and on the Mac, because a signal raises none of Tauri's exit events.
//!
//! The test raises a real `SIGTERM` at this process. Without a listener that
//! ends the whole test binary, which is how it failed before the fix. With
//! one, the signal runs the exit handler on the mock app, as the app's own
//! listener does through the event loop's exit, and the profile's own server
//! must be gone. A later signal to this binary ends it at once
//! (`stop_signals`' second-signal rule), the same as the default would.

use std::time::Duration;

use knobas_app::sources::{Registry, SecretStoreKind};
use knobas_app::stop_signals::{self, StopSignal};
use knobas_app::{DbState, Lifecycle, Profile};
use std::sync::Arc;
use tauri::Manager;

/// The port the server serving `root_dir` recorded in `postmaster.pid` (line 4).
fn port_of(root_dir: &std::path::Path) -> u16 {
    let pid_file = std::fs::read_to_string(root_dir.join("data").join("postmaster.pid"))
        .expect("a running server's postmaster.pid");
    pid_file.lines().nth(3).unwrap().trim().parse().unwrap()
}

fn port_answers(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(500),
    )
    .is_ok()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sigterm_stops_the_database_as_a_quit_does() {
    let data = tempfile::tempdir().unwrap();
    let profile = Profile::new(false, data.path());
    let root = profile.db_config(None).root_dir;
    let registry = Registry::default();
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    app.manage(profile.clone());
    app.manage(SecretStoreKind::Memory);
    app.manage(Lifecycle::with_registry(Arc::new(registry.clone())));
    app.manage(knobas_app::capture::CaptureState::new());
    knobas_app::bring_up(app.handle(), &profile, None, registry).await;
    assert_eq!(app.state::<Lifecycle>().get(), DbState::Ready);
    let port = port_of(&root);
    assert!(port_answers(port), "the profile's own server is up");

    let (quit_tx, quit_rx) = std::sync::mpsc::channel();
    let handle = app.handle().clone();
    stop_signals::listen(move |signal| {
        knobas_app::shutdown(&handle);
        quit_tx.send(signal).unwrap();
    })
    .expect("the handlers are in place");
    rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::TERM)
        .expect("a SIGTERM to this process");

    let signal = tokio::task::spawn_blocking(move || quit_rx.recv_timeout(Duration::from_secs(90)))
        .await
        .unwrap()
        .expect("the signal runs the quit");
    assert_eq!(signal, StopSignal::Term);
    assert!(
        !port_answers(port),
        "the server answered on its port after the SIGTERM's quit"
    );
    assert!(
        !root.join("data").join("postmaster.pid").exists(),
        "no postmaster is left on the data directory"
    );
}
