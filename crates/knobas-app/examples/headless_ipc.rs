//! The app's backend with no window (the still-rows pass of PR #2479,
//! 2026-11-11): every command `run` registers, on Tauri's mock runtime, over
//! a demo profile, answered over HTTP on 127.0.0.1 so headless Chrome can
//! draw a built front end on a real database (`?fake-ipc&fake-real=<port>`,
//! `app/src/lib/shell/dev/fake-tauri.ts`).
//!
//! ```sh
//! cp -cR final-home /tmp/my-home                      # never the original
//! HOME=/tmp/my-home cargo run -p knobas-app --features test-util --example headless_ipc -- \
//!   --data-dir "/tmp/my-home/Library/Application Support/dev.knobas.desktop" --port 5399
//! ```
//!
//! **A demo profile under the data directory named, and nothing else.** No
//! default: a run that named no directory would open the real profile. Shows
//! no notification, opens no window, and emits its events to nobody. Its
//! scheduler syncs as the app's would, so a product that answers is synced.
//!
//! `--timings <file>` (the instant-open pass, 2026-11-12) appends one JSON
//! line a command: its name, when it arrived and left (ms since the epoch),
//! the backend's time on it and the answer's bytes, so a page open splits
//! into what the backend spent and what the page did.
//!
//! One request is `POST /invoke` with `{"cmd": "...", "args": {...}}`, and
//! the answer `{"ok": value}` or `{"err": value}`, the rejection as the
//! webview would see it. `POST /quit` stops it as the app's quit does: the
//! scheduler, then the database. Killed instead, it leaves the profile's
//! PostgreSQL running (`pg_ctl stop -D <profile>/db/data` stops that).

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::CallbackFn;
use tauri::test::MockRuntime;

const USAGE: &str =
    "usage: headless_ipc --data-dir <app data dir> --port <port> [--timings <file>]";

fn main() {
    let mut args = std::env::args().skip(1);
    let mut data_dir: Option<PathBuf> = None;
    let mut port: Option<u16> = None;
    let mut timings: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--port" => port = args.next().and_then(|p| p.parse().ok()),
            "--timings" => timings = args.next().map(PathBuf::from),
            _ => {
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
        }
    }
    let (Some(data_dir), Some(port)) = (data_dir, port) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };

    let app = tauri::test::mock_builder()
        .invoke_handler(knobas_app::invoke_handler())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    let handle = app.handle().clone();
    let profile = knobas_app::Profile::new(true, &data_dir);
    eprintln!("headless_ipc: profile {}", profile.dir.display());
    let quiet: Arc<dyn knobas_app::notify::Backend> =
        Arc::new(|_: &knobas_app::notify::NotificationDraft| {
            Err("headless: no notifications".to_owned())
        });
    knobas_app::manage_app_state(&handle, profile.clone(), quiet);
    tauri::async_runtime::block_on(knobas_app::bring_up(
        &handle,
        &profile,
        None,
        knobas_app::sources::Registry::for_process(),
    ));
    eprintln!(
        "headless_ipc: database {:?}",
        tauri::Manager::state::<knobas_app::Lifecycle>(&handle).get()
    );

    let webview = tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .expect("mock webview");
    let timings = timings.map(|path| {
        Arc::new(std::sync::Mutex::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .expect("timings file"),
        ))
    });
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    eprintln!("headless_ipc: answering on 127.0.0.1:{port}");
    for stream in listener.incoming().flatten() {
        let webview = webview.clone();
        let handle = handle.clone();
        let timings = timings.clone();
        std::thread::spawn(move || {
            if let Ok(true) = serve(stream, &webview, timings.as_deref()) {
                eprintln!("headless_ipc: stopping");
                knobas_app::shutdown(&handle);
                std::process::exit(0);
            }
        });
    }
}

/// Answers one request; `Ok(true)` when it asked to stop.
fn serve(
    stream: TcpStream,
    webview: &tauri::WebviewWindow<MockRuntime>,
    timings: Option<&std::sync::Mutex<std::fs::File>>,
) -> std::io::Result<bool> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut words = line.split_whitespace();
    let method = words.next().unwrap_or("").to_owned();
    let quit = method == "POST" && words.next() == Some("/quit");
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let since_epoch = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or_default()
    };
    let arrived = since_epoch();
    let clock = std::time::Instant::now();
    let answer = if method == "POST" && !quit {
        invoke(webview, &body)
    } else {
        String::new()
    };
    if let Some(file) = timings
        && method == "POST"
        && !quit
    {
        let request: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
        let line = serde_json::json!({
            "cmd": request["cmd"],
            "args": request["args"],
            "arrived": arrived,
            "left": since_epoch(),
            "ms": clock.elapsed().as_secs_f64() * 1000.0,
            "bytes": answer.len(),
        });
        if let Ok(mut file) = file.lock() {
            let _ = writeln!(file, "{line}");
        }
    }
    let mut out = stream;
    let written = write!(
        out,
        "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type\r\n\
         Access-Control-Allow-Methods: POST, OPTIONS\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{answer}",
        answer.len()
    );
    written.map(|()| quit)
}

fn invoke(webview: &tauri::WebviewWindow<MockRuntime>, body: &[u8]) -> String {
    let request: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let cmd = request["cmd"].as_str().unwrap_or_default().to_owned();
    let args = request
        .get("args")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let outcome = tauri::test::get_ipc_response(
        webview,
        tauri::webview::InvokeRequest {
            cmd,
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("url"),
            body: args.into(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    );
    let value = match outcome {
        Ok(body) => {
            serde_json::json!({ "ok": body.deserialize::<serde_json::Value>().unwrap_or_default() })
        }
        Err(rejection) => serde_json::json!({ "err": rejection }),
    };
    value.to_string()
}
