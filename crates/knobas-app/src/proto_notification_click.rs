//! PROTOTYPE — throwaway, branch `prototype/notification-click`, issue #339.
//!
//! Question: does `notify_rust::NotificationHandle::wait_for_action` report a
//! banner click inside a signed Tauri bundle, from a worker thread?
//!
//! Drives itself at startup so nobody has to add a button: three
//! notifications in sequence, each telling the reader what to do, each
//! result appended to `~/knobas-proto-notification-click.log` and echoed as a
//! fourth, fire-and-forget notification. Nothing here is production code.

use std::io::Write;
use std::time::{Duration, Instant};

fn log(line: &str) {
    let stamp = chrono::Local::now().format("%H:%M:%S%.3f");
    let text = format!("{stamp}  {line}");
    tracing::info!(target: "proto_notification_click", "{text}");
    if let Some(home) = std::env::var_os("HOME") {
        let path = std::path::Path::new(&home).join("knobas-proto-notification-click.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{text}");
        }
    }
}

fn announce(title: &str, body: &str) {
    // Dropping the handle sends asynchronously (notify-rust's Drop impl).
    let _ = notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .show();
}

/// One step: build, show, wait; log what came back and how long it took.
fn step(n: u8, title: &str, body: &str, with_action: bool) {
    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(body);
    if with_action {
        // On XDG this is what makes a body click reportable at all; on the NS
        // backend it becomes the "Open" button. Item 3 of the checklist.
        notification.action("default", "Open");
    }
    log(&format!("step {n}: showing (with_action={with_action}) — {title}"));
    let started = Instant::now();
    match notification.show() {
        Ok(handle) => {
            log(&format!("step {n}: shown, waiting on the handle"));
            let mut outcome = String::from("<closure never called>");
            handle.wait_for_action(|action| outcome = action.to_owned());
            let secs = started.elapsed().as_secs_f32();
            log(&format!("step {n}: wait_for_action returned {outcome:?} after {secs:.1}s"));
            announce(
                &format!("knobas prototype: step {n} result"),
                &format!("{outcome:?} after {secs:.1}s"),
            );
        }
        Err(error) => log(&format!("step {n}: show failed: {error}")),
    }
}

pub fn start(identifier: String, dev: bool) {
    std::thread::Builder::new()
        .name("proto-notification-click".into())
        .spawn(move || {
            let backend = if cfg!(feature = "proto-un") { "UN (preview-macos-un)" } else { "NS" };
            log(&format!("=== start: backend={backend} dev={dev} identifier={identifier} pid={} ===", std::process::id()));
            #[cfg(target_os = "macos")]
            {
                let app = if dev { "com.apple.Terminal" } else { identifier.as_str() };
                let r = notify_rust::set_application(app);
                log(&format!("set_application({app:?}) -> {r:?}"));
            }
            // Give the window and the permission prompt a moment.
            std::thread::sleep(Duration::from_secs(8));
            step(1, "knobas prototype 1/3", "Click the BODY of this banner.", false);
            std::thread::sleep(Duration::from_secs(3));
            step(2, "knobas prototype 2/3", "Click the OPEN button on this one.", true);
            std::thread::sleep(Duration::from_secs(3));
            step(3, "knobas prototype 3/3", "Do NOTHING. Let this one slide away.", true);
            log("=== done ===");
        })
        .expect("proto thread");
}
