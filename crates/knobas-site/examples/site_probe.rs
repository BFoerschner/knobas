//! The probe binary (ADR-0050, decisions §2, §24): knobas-site's own host and
//! controller, run against a local test site in a real window, with a log
//! line per check. Run by hand on each platform when wry, Tauri or this crate
//! changes; it needs a window, so it is not in `just check`.
//!
//! One `tao` window plays knobas's main window, with a stand-in shell page in
//! a plain wry web view that, like knobas's, has a custom scheme of its own
//! (`knobas-render`) and an IPC handler. Under its bar, sites are made by
//! `knobas_site::wry_host::WryHost` and driven through `knobas_site::Sites`,
//! the code knobas ships. A small HTTP server in the same process plays the
//! site on `http://127.0.0.1:<port>`; the same server reached as
//! `http://localhost:<port>` is another origin. A page reports what it sees by
//! fetching `/report` on its own origin, so nothing is taken on trust from a
//! script of knobas's.
//!
//! ```text
//! SITE_PROBE_DIR=<scratch> cargo run -p knobas-site --example site_probe
//! SITE_PROBE_DIR=<same> SITE_PROBE_SECOND=1 cargo run …   # the sessions kept
//! ```
//!
//! The checklist (decisions §24), each a `check` line, PASS or FAIL:
//! 1. the page sees no `__TAURI_INTERNALS__`, no `isTauri`, cannot post to
//!    `window.ipc` to any effect, and cannot fetch `knobas-render://`,
//!    `tauri://` or `ipc://`;
//! 2. a second site on the same origin gets no cookie of the first; a second
//!    run gets the first run's cookies back;
//! 3. the rule sees a redirect's target and cancels it (the bar asks while
//!    landing); a frame on another origin loads; a link to another origin
//!    goes to the browser;
//! 4. `window.open` opens no window; a `target=_blank` link on the site's own
//!    origin loads in the same view;
//! 5. a download is saved under the Downloads folder knobas names;
//! 6. the probe answers clean, then changed after the page objects;
//! 7. a key typed in the site reaches it, a claimed key does not, Esc is
//!    passed once and taken the second time;
//! 9. microphone, location, clipboard read and notifications are refused;
//! 10. making a view, and the shell page's longest frame around it;
//! 11. (Windows) at the quit each live site names its browser process, and
//!     (11b) the quit's wait returns once every named browser process has
//!     ended, before its deadline (`win::wait_for_exit`, ux29);
//! 12. a lasting cookie a site set one second before the quit is there in the
//!     second run.
//!
//! The first run quits with its two sites still open, as knobas quits, so the
//! quit's own path (`Sites::shutdown`, then the host's `flush`) runs. Checks
//! 11 and 11b have no automated run (nothing runs a test on Windows): a diff
//! touching `win::wait_for_exit`, `browser_process` or `Sites::shutdown`
//! reruns this probe on Windows with each mutant of them applied by hand
//! (the decision note `2026-11-21-a-site-keeps-its-sign-in-and-signs-itself-in.md`).
//!
//! Item 8 (`confirm()`'s sheet on the Mac, WebView2's dialog on Windows) asks
//! for a person and is on the hand list.

#![allow(clippy::too_many_lines, clippy::missing_panics_doc)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use knobas_site::keys::Platform;
use knobas_site::policy::{Base, OtherSite, SiteRules};
use knobas_site::sites::{Config, Shell, SiteEvent, SiteInsets, install_host};
use knobas_site::{Sites, wry_host::WryHost};
use serde_json::{Value, json};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;

static START: OnceLock<Instant> = OnceLock::new();
static PORT: AtomicU16 = AtomicU16::new(0);
static RECORD: Mutex<Vec<Value>> = Mutex::new(Vec::new());
static LOG: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
/// How many sites were open when the first run quit (checks 11 and 11b).
static LIVE: Mutex<usize> = Mutex::new(0);

fn ms() -> u64 {
    u64::try_from(START.get().map_or(0, |s| s.elapsed().as_millis())).unwrap_or(0)
}

fn log(kind: &str, data: Value) {
    let line = json!({ "t": ms(), "kind": kind, "data": data });
    println!("{line}");
    if let Some(file) = LOG.get() {
        let _ = writeln!(file.lock().unwrap(), "{line}");
    }
    RECORD.lock().unwrap().push(line);
}

fn port() -> u16 {
    PORT.load(Ordering::Relaxed)
}

fn recorded(kind: &str, pick: impl Fn(&Value) -> bool) -> Vec<Value> {
    RECORD
        .lock()
        .unwrap()
        .iter()
        .filter(|line| line["kind"] == kind && pick(&line["data"]))
        .cloned()
        .collect()
}

// ------------------------------------------------------------------ the site

const PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>probe site</title>
<body style="font:14px system-ui;background:#fff;margin:24px">
<h1 id="h">site page</h1><input id="f" placeholder="site field">
<a id="blank" href="/page?name=blank-target" target="_blank">target blank</a>
<script>
const q = new URLSearchParams(location.search); const name = q.get('name') || '';
document.getElementById('h').textContent = 'site page ' + name + ' @ ' + location.origin;
document.title = 'probe ' + name;
const r = (o) => fetch('/report?' + new URLSearchParams(Object.assign({name, origin: location.origin}, o))).catch(() => {});
const timed = (u) => { const c = new AbortController(); setTimeout(() => c.abort(), 1500); return fetch(u, {signal: c.signal}); };
(async () => {
  const env = { ev: 'env', tauriInternals: typeof window.__TAURI_INTERNALS__, isTauri: typeof window.isTauri,
    ipc: typeof window.ipc, cookie: document.cookie, ua: navigator.userAgent, top: String(window.top === window) };
  try { window.ipc.postMessage('{"cmd":"report"}'); env.ipcPost = 'sent'; } catch (e) { env.ipcPost = 'threw ' + e.name; }
  for (const u of ['knobas-render://localhost/x', 'tauri://localhost/index.html', 'http://tauri.localhost/index.html', 'ipc://localhost/x', 'http://ipc.localhost/x', 'http://knobas-render.localhost/x']) {
    try { const res = await timed(u); env['fetch ' + u] = 'read ' + res.status; } catch (e) { env['fetch ' + u] = 'failed'; }
  }
  document.cookie = 'js=' + name + '; max-age=86400; path=/';
  if (name === 'a' && window.top === window) {
    const fr = document.createElement('iframe'); fr.src = 'http://localhost:__PORT__/page?name=framed'; document.body.appendChild(fr);
  }
  if (name === 'other') setTimeout(() => { location.href = 'http://localhost:__PORT__/page?name=offsite'; }, 300);
  if (name === 'popup') setTimeout(() => { window.open('http://localhost:__PORT__/page?name=popup-target'); setTimeout(() => document.getElementById('blank').click(), 400); }, 300);
  if (name === 'dl') setTimeout(() => { location.href = '/download'; }, 300);
  if (name === 'dirty') addEventListener('beforeunload', (e) => { e.preventDefault(); e.returnValue = ''; });
  if (name === 'keys') document.getElementById('f').focus();
  if (name === 'perms') {
    const settle = (p, ms) => Promise.race([p, new Promise((res) => setTimeout(() => res('no answer'), ms))]);
    env.mic = await settle(navigator.mediaDevices ? navigator.mediaDevices.getUserMedia({audio: true}).then(() => 'granted', (e) => 'refused ' + e.name) : Promise.resolve('no mediaDevices'), 2500);
    env.geo = await settle(new Promise((res) => navigator.geolocation ? navigator.geolocation.getCurrentPosition(() => res('granted'), (e) => res('refused ' + e.code)) : res('no geolocation')), 2500);
    env.clip = await settle(navigator.clipboard && navigator.clipboard.readText ? navigator.clipboard.readText().then(() => 'granted', (e) => 'refused ' + e.name) : Promise.resolve('no clipboard'), 2500);
    env.notify = typeof Notification === 'undefined' ? 'no Notification' : await settle(Notification.requestPermission().then(String, (e) => 'threw ' + e.name), 2500);
  }
  r(env);
})();
addEventListener('keydown', (e) => r({ ev: 'keydown', key: e.key, meta: e.metaKey, ctrl: e.ctrlKey }));
</script>"#;

const SHELL: &str = r#"<!doctype html><meta charset="utf-8"><title>probe shell</title>
<body style="margin:0;font:13px system-ui;background:#e4e8ec">
<div style="height:88px;background:#17212b;color:#fff;display:flex;align-items:center;padding:0 16px">knobas (the probe's stand-in shell)</div>
<script>
let last = performance.now(), worst = 0, since = performance.now();
function frame(now) { worst = Math.max(worst, now - last); last = now;
  if (now - since > 1000) { window.ipc.postMessage(JSON.stringify({ worstFrame: worst })); worst = 0; since = now; }
  requestAnimationFrame(frame); }
requestAnimationFrame(frame);
addEventListener('keydown', (e) => window.ipc.postMessage(JSON.stringify({ shellKey: e.key })));
</script>"#;

fn decode(query: &str) -> String {
    let bytes = query.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("20");
                out.push(u8::from_str_radix(hex, 16).unwrap_or(b'?'));
                i += 2;
            }
            other => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn respond(stream: &mut TcpStream, status: &str, headers: &[(&str, String)], body: &[u8]) {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn handle(mut stream: TcpStream) {
    let Ok(clone) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(clone);
    let mut first = String::new();
    if reader.read_line(&mut first).unwrap_or(0) == 0 {
        return;
    }
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let target = first.split_whitespace().nth(1).unwrap_or("/").to_string();
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let host = headers.get("host").cloned().unwrap_or_default();
    let cookie = headers.get("cookie").cloned().unwrap_or_default();
    match path {
        "/report" => {
            let fields: serde_json::Map<String, Value> = decode(query)
                .split('&')
                .filter_map(|pair| pair.split_once('='))
                .map(|(k, v)| (k.to_owned(), Value::String(v.to_owned())))
                .collect();
            log("report", json!({ "host": host, "fields": fields }));
            respond(
                &mut stream,
                "204 No Content",
                &[("Access-Control-Allow-Origin", "*".into())],
                b"",
            );
        }
        "/page" => {
            log(
                "request",
                json!({ "host": host, "query": decode(query), "cookie": cookie, "dest": headers.get("sec-fetch-dest") }),
            );
            let mut extra = vec![
                ("Content-Type", "text/html; charset=utf-8".to_string()),
                ("Cache-Control", "no-store".to_string()),
            ];
            if !cookie.contains("sid=") {
                let name = decode(query)
                    .split('=')
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned();
                extra.push((
                    "Set-Cookie",
                    format!("sid={name}; HttpOnly; Max-Age=86400; Path=/"),
                ));
            }
            respond(
                &mut stream,
                "200 OK",
                &extra,
                PAGE.replace("__PORT__", &port().to_string()).as_bytes(),
            );
        }
        "/late" => {
            log(
                "request",
                json!({ "host": host, "query": "late", "cookie": cookie }),
            );
            respond(
                &mut stream,
                "200 OK",
                &[
                    ("Content-Type", "text/html; charset=utf-8".to_string()),
                    ("Cache-Control", "no-store".to_string()),
                    ("Set-Cookie", "late=1; Max-Age=86400; Path=/".to_string()),
                ],
                b"<!doctype html><title>late</title><p>a lasting cookie, set just before the quit",
            );
        }
        "/redirect-out" => {
            log("request", json!({ "host": host, "query": "redirect-out" }));
            respond(
                &mut stream,
                "302 Found",
                &[(
                    "Location",
                    format!("http://localhost:{}/page?name=redirected-out", port()),
                )],
                b"",
            );
        }
        "/download" => {
            log("request", json!({ "host": host, "query": "download" }));
            respond(
                &mut stream,
                "200 OK",
                &[
                    ("Content-Type", "application/octet-stream".into()),
                    (
                        "Content-Disposition",
                        "attachment; filename=\"../../probe.txt\"".into(),
                    ),
                ],
                b"hello from the probe",
            );
        }
        _ => respond(&mut stream, "404 Not Found", &[], b""),
    }
}

// --------------------------------------------------------------- the shell

type Task = Box<dyn FnOnce() + Send>;

struct ProbeShell {
    proxy: Mutex<EventLoopProxy<Task>>,
    main: std::thread::ThreadId,
    size: Mutex<(f64, f64)>,
}

impl Shell for ProbeShell {
    fn on_main(&self, f: Task) {
        if std::thread::current().id() == self.main {
            f();
        } else {
            let _ = self.proxy.lock().unwrap().send_event(f);
        }
    }
    fn later(&self, f: Task) {
        let _ = self.proxy.lock().unwrap().send_event(f);
    }
    fn emit(&self, event: SiteEvent) {
        let payload = match &event {
            SiteEvent::Status(s) => serde_json::to_value(s),
            SiteEvent::Key(k) => serde_json::to_value(k),
            SiteEvent::Routed(r) => serde_json::to_value(r),
            SiteEvent::Download(d) => serde_json::to_value(d),
            SiteEvent::Closed(c) => serde_json::to_value(c),
        }
        .unwrap_or(Value::Null);
        log(event.name(), payload);
    }
    #[allow(clippy::cast_precision_loss)]
    fn now(&self) -> f64 {
        START
            .get()
            .map_or(0.0, |s| s.elapsed().as_secs_f64() * 1000.0)
    }
    fn window_size(&self) -> (f64, f64) {
        *self.size.lock().unwrap()
    }
    fn quit(&self) {
        log("quit-asked", json!({}));
    }
    fn landed(&self, source: &str) {
        log("landed", json!({ "source": source }));
    }
    /// The probe keeps no saved sign-in: the ask is logged and nothing is typed.
    fn sign_in(&self, source: &str) {
        log("sign-in-asked", json!({ "source": source }));
    }
}

const INSETS: SiteInsets = SiteInsets {
    top: 88.0,
    right: 0.0,
    bottom: 0.0,
    left: 0.0,
};

// ---------------------------------------------------------------- the keys

#[cfg(target_os = "macos")]
mod post {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{NSPoint, NSString};

    /// A key press posted to the app's own queue, as the window server would.
    pub fn key(chars: &str, code: u16, command: bool) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        let Some(window) = app.keyWindow().or_else(|| app.mainWindow()) else {
            super::log("post-key", serde_json::json!("no key window"));
            return;
        };
        let flags = if command {
            NSEventModifierFlags::Command
        } else {
            NSEventModifierFlags::empty()
        };
        let text = NSString::from_str(chars);
        let now: f64 = {
            // SAFETY: NSProcessInfo's own uptime.
            let info: objc2::rc::Retained<objc2::runtime::AnyObject> =
                unsafe { objc2::msg_send![objc2::class!(NSProcessInfo), processInfo] };
            unsafe { objc2::msg_send![&*info, systemUptime] }
        };
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                kind, NSPoint::new(0.0, 0.0), flags, now, window.windowNumber(), None, &text, &text, false, code,
            );
            if let Some(event) = event {
                app.postEvent_atStart(&event, false);
            }
        }
    }

    pub const ESC: (&str, u16, bool) = ("\u{1b}", 53, false);
    pub const X: (&str, u16, bool) = ("x", 7, false);
    pub const CLAIMED: (&str, u16, bool) = ("k", 40, true);
}

#[cfg(windows)]
mod post {
    use std::sync::Mutex;

    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_VSC, MapVirtualKeyW};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
        PostMessageW, WM_KEYDOWN, WM_KEYUP,
    };
    use windows_core::BOOL;

    /// The Chromium windows the shell's own web view had before any site was made.
    static SHELLS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

    fn chromium_windows() -> Vec<(isize, bool)> {
        unsafe extern "system" fn child(hwnd: HWND, lparam: LPARAM) -> BOOL {
            // SAFETY: the vector handed in below, for the length of the enumeration.
            let found = unsafe { &mut *(lparam.0 as *mut Vec<(isize, bool)>) };
            let mut name = [0u16; 64];
            // SAFETY: Win32 queries of a window handle the enumeration gave.
            let len = unsafe { GetClassNameW(hwnd, &mut name) };
            if String::from_utf16_lossy(&name[..usize::try_from(len).unwrap_or(0)])
                == "Chrome_WidgetWin_1"
            {
                found.push((hwnd.0 as isize, unsafe { IsWindowVisible(hwnd) }.as_bool()));
            }
            true.into()
        }
        unsafe extern "system" fn top(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let mut pid = 0_u32;
            // SAFETY: Win32 queries of a window handle the enumeration gave.
            unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut pid)) };
            if pid == std::process::id() {
                // SAFETY: the same vector, passed on.
                let _ = unsafe { EnumChildWindows(Some(hwnd), Some(child), lparam) };
            }
            true.into()
        }
        let mut found: Vec<(isize, bool)> = Vec::new();
        // SAFETY: the enumeration calls back on this thread with our vector.
        let _ = unsafe { EnumWindows(Some(top), LPARAM(&raw mut found as isize)) };
        found
    }

    /// Before any site is made: the shell's own Chromium windows.
    pub fn note_shell() {
        *SHELLS.lock().unwrap() = chromium_windows().into_iter().map(|(h, _)| h).collect();
    }

    /// The shown site's Chromium window: a key posted there raises
    /// `AcceleratorKeyPressed` before the page sees it (decisions §22), with
    /// or without the window in the foreground.
    fn target() -> Option<HWND> {
        let shells = SHELLS.lock().unwrap().clone();
        chromium_windows()
            .into_iter()
            .find(|(h, visible)| *visible && !shells.contains(h))
            .map(|(h, _)| HWND(h as _))
    }

    pub fn key(_chars: &str, vk: u16, _command: bool) {
        let Some(hwnd) = target() else {
            super::log("post-key", serde_json::json!("no site Chromium window"));
            return;
        };
        // SAFETY: messages posted to a window of this process.
        unsafe {
            let scan = MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC);
            let down = LPARAM((1 | (scan << 16)) as isize);
            let up = LPARAM((1 | (scan << 16) | (1 << 30) | (1 << 31)) as isize);
            let _ = PostMessageW(Some(hwnd), WM_KEYDOWN, WPARAM(usize::from(vk)), down);
            let _ = PostMessageW(Some(hwnd), WM_KEYUP, WPARAM(usize::from(vk)), up);
        }
    }

    pub const ESC: (&str, u16, bool) = ("", 0x1B, false);
    pub const X: (&str, u16, bool) = ("", 0x58, false);
    /// F5 alone is knobas's on Windows (a posted key carries no Ctrl).
    pub const CLAIMED: (&str, u16, bool) = ("", 0x74, false);
}

// ----------------------------------------------------------- the quit's wait

/// `WryHost` with the quit's wait watched (checks 11 and 11b): what `flush`
/// is handed, how long it takes, and which of the named browser processes
/// still run when it returns.
struct ProbeHost<H>(H);

impl<H: knobas_site::host::SiteHost> knobas_site::host::SiteHost for ProbeHost<H> {
    fn make(
        &self,
        spec: &knobas_site::host::ViewSpec,
        hooks: knobas_site::host::Hooks,
    ) -> Result<Box<dyn knobas_site::host::SiteView>, knobas_site::host::HostError> {
        self.0.make(spec, hooks)
    }
    fn focus_shell(&self) {
        self.0.focus_shell();
    }
    fn forget(&self, spec: &knobas_site::host::ViewSpec, done: Box<dyn FnOnce(bool) + Send>) {
        self.0.forget(spec, done);
    }
    fn flush(&self, processes: &[u32], deadline: Instant) {
        let started = Instant::now();
        self.0.flush(processes, deadline);
        let took = started.elapsed();
        let running: Vec<u32> = processes
            .iter()
            .copied()
            .filter(|pid| still_runs(*pid))
            .collect();
        let live = *LIVE.lock().unwrap();
        log(
            "flush",
            json!({ "processes": processes, "live": live, "ms": took.as_secs_f64() * 1000.0, "running": running, "deadline_hit": Instant::now() >= deadline }),
        );
        if cfg!(windows) && live > 0 {
            check(
                "11 each live site names its browser process at the quit",
                processes.len() == live && processes.iter().all(|pid| *pid != 0),
                json!({ "processes": processes, "live": live }),
            );
            check(
                "11b the quit waits until every named browser process has ended",
                !processes.is_empty() && running.is_empty() && Instant::now() < deadline,
                json!({ "ms": took.as_secs_f64() * 1000.0, "running": running }),
            );
        }
    }
}

/// Whether a process still runs: a zero-wait on its handle (Windows); off Windows nothing is named, so nothing runs.
#[cfg(windows)]
fn still_runs(pid: u32) -> bool {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn WaitForSingleObject(handle: isize, millis: u32) -> u32;
        fn CloseHandle(handle: isize) -> i32;
    }
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_TIMEOUT: u32 = 0x0000_0102;
    // SAFETY: plain Win32 calls on a handle this function opens and closes.
    unsafe {
        let handle = OpenProcess(SYNCHRONIZE, 0, pid);
        if handle == 0 {
            return false;
        }
        let state = WaitForSingleObject(handle, 0);
        CloseHandle(handle);
        state == WAIT_TIMEOUT
    }
}

#[cfg(not(windows))]
fn still_runs(_pid: u32) -> bool {
    false
}

// --------------------------------------------------------------- the steps

fn on_main(proxy: &EventLoopProxy<Task>, f: impl FnOnce() + Send + 'static) {
    let _ = proxy.send_event(Box::new(f));
}

fn sleep(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

fn steps(sites: Sites, proxy: EventLoopProxy<Task>, second: bool) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("a runtime");
    let p = port();
    let base = format!("http://127.0.0.1:{p}");
    let url = |name: &str| format!("{base}/page?name={name}");
    let open = |source: &str, at: String, force: bool| {
        let answer = rt.block_on(sites.open(source, Some(at.clone()), INSETS, force));
        log(
            "open",
            json!({ "source": source, "url": at, "answer": format!("{answer:?}") }),
        );
        answer
    };
    sleep(1500);
    #[cfg(windows)]
    on_main(&proxy, post::note_shell);
    sleep(300);
    let started = ms();
    let _ = open("a", url(if second { "a-second" } else { "a" }), false);
    sites.show("a", INSETS, true, true);
    sleep(3000);
    log("step", json!({ "made_at": started }));
    let _ = open("b", url(if second { "b-second" } else { "b-first" }), false);
    sites.show("b", INSETS, false, true);
    sleep(2500);
    if second {
        verdicts(true);
        // Nothing of the probe's stays on the machine: both sessions go, as *Forget…* removes them.
        for source in ["a", "b"] {
            let forgot = rt.block_on(sites.forget(source, true));
            log(
                "forgot",
                json!({ "source": source, "answer": format!("{forgot:?}") }),
            );
        }
        sleep(1500);
        return finish(&sites, &proxy, &rt);
    }
    let _ = &rt;
    sites.hide("b");
    sites.show("a", INSETS, true, true);
    let _ = open("a", url("other"), true);
    sleep(2500);
    let _ = open("a", format!("{base}/redirect-out"), true);
    sleep(2000);
    let asked = sites
        .statuses()
        .into_iter()
        .find(|s| s.source == "a")
        .map(|s| s.asks_origin.clone());
    log("asked", json!({ "origin": asked }));
    if let Some(Some(origin)) = asked {
        let declined = sites.answer("a", &origin, false);
        log("declined", json!({ "browser": format!("{declined:?}") }));
    }
    sleep(500);
    let _ = open("a", url("popup"), true);
    sleep(3000);
    let _ = open("a", url("dl"), true);
    sleep(2500);
    let _ = open("a", url("dirty"), true);
    sleep(1500);
    let clean_b = rt.block_on(sites.unsaved());
    log(
        "unsaved",
        json!({ "sources": clean_b.iter().map(|s| s.source.clone()).collect::<Vec<_>>() }),
    );
    let close = rt.block_on(sites.close("a", false));
    log("close-asked", json!({ "answer": format!("{close:?}") }));
    let _ = open("a", url("keys"), true);
    sites.show("a", INSETS, true, true);
    sleep(2000);
    for (what, key, gap) in [
        ("x", post::X, 600),
        ("esc", post::ESC, 250),
        ("esc-again", post::ESC, 800),
        ("claimed", post::CLAIMED, 800),
    ] {
        log("post-key", json!({ "key": what }));
        on_main(&proxy, move || post::key(key.0, key.1, key.2));
        // A posted event waits for the event loop's next turn: wake it, so the key is handled now and not with the next.
        for _ in 0..3 {
            sleep(15);
            on_main(&proxy, || {});
        }
        sleep(gap - 45);
    }
    let _ = open("a", url("perms"), true);
    sleep(14000);
    verdicts(false);
    // Check 12's cookie, one second before the quit; both sites stay open for checks 11 and 11b.
    let _ = open("a", format!("{base}/late"), true);
    sleep(1000);
    *LIVE.lock().unwrap() = sites.statuses().len();
    log("quitting", json!({ "live": *LIVE.lock().unwrap() }));
    quit(&sites, &proxy);
}

/// The second run's end: both sites closed (they were forgotten), then the quit.
fn finish(sites: &Sites, proxy: &EventLoopProxy<Task>, rt: &tokio::runtime::Runtime) {
    for source in ["a", "b"] {
        let _ = rt.block_on(sites.close(source, true));
    }
    sleep(800);
    quit(sites, proxy);
}

/// knobas's quit: `Sites::shutdown` on the main thread, with whatever sites are still open, then the exit.
fn quit(sites: &Sites, proxy: &EventLoopProxy<Task>) {
    let done = sites.clone();
    on_main(proxy, move || {
        done.shutdown();
        log("done", json!({}));
        std::process::exit(0);
    });
    sleep(5000);
}

fn field(line: &Value, name: &str) -> String {
    line["fields"][name].as_str().unwrap_or_default().to_owned()
}

fn check(id: &str, pass: bool, detail: Value) {
    log("check", json!({ "id": id, "pass": pass, "detail": detail }));
}

fn verdicts(second: bool) {
    let p = port();
    if second {
        let a = recorded("request", |d| d["query"].as_str() == Some("name=a-second"));
        let b = recorded("request", |d| d["query"].as_str() == Some("name=b-second"));
        let a_cookie = a
            .first()
            .map(|l| l["data"]["cookie"].as_str().unwrap_or_default().to_owned())
            .unwrap_or_default();
        let b_cookie = b
            .first()
            .map(|l| l["data"]["cookie"].as_str().unwrap_or_default().to_owned())
            .unwrap_or_default();
        check(
            "12 a lasting cookie set one second before the quit is back",
            a_cookie.contains("late=1"),
            json!({ "a": a_cookie }),
        );
        check(
            "2b the second run gets each site's own cookies back",
            a_cookie.contains("sid=a")
                && b_cookie.contains("sid=b-first")
                && !b_cookie.contains("sid=a"),
            json!({ "a": a_cookie, "b": b_cookie }),
        );
        return;
    }
    let env = recorded("report", |d| {
        d["fields"]["ev"] == "env" && d["fields"]["name"] == "a"
    })
    .into_iter()
    .next();
    let env = env.map(|l| l["data"].clone()).unwrap_or(Value::Null);
    let fetches: Vec<(String, String)> = env["fields"]
        .as_object()
        .map(|o| {
            o.iter()
                .filter(|(k, _)| k.starts_with("fetch "))
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                .collect()
        })
        .unwrap_or_default();
    check(
        "1 the page reaches nothing of knobas",
        field(&env, "tauriInternals") == "undefined"
            && field(&env, "isTauri") == "undefined"
            && !fetches.is_empty()
            && fetches.iter().all(|(_, v)| v == "failed")
            && recorded("shell-ipc", |d| d.get("cmd").is_some()).is_empty(),
        json!({ "env": env["fields"], "shell_got_site_ipc": !recorded("shell-ipc", |d| d.get("cmd").is_some()).is_empty() }),
    );
    let b = recorded("request", |d| d["query"].as_str() == Some("name=b-first"));
    let b_cookie = b
        .first()
        .map(|l| l["data"]["cookie"].as_str().unwrap_or_default().to_owned());
    check(
        "2 a second site on the same origin starts with no cookie of the first",
        b_cookie.as_deref() == Some(""),
        json!({ "b_first_request_cookie": b_cookie }),
    );
    let framed = !recorded("report", |d| d["fields"]["name"] == "framed").is_empty();
    check("3a a frame on another origin loads", framed, json!({}));
    let offsite = recorded("site:routed", |d| {
        d["to"] == "browser" && d["url"].as_str().is_some_and(|u| u.contains("offsite"))
    });
    let offsite_seen =
        !recorded("request", |d| d["query"].as_str() == Some("name=offsite")).is_empty();
    let offsite_shown = !recorded("report", |d| d["fields"]["name"] == "offsite").is_empty();
    check(
        "3b a link to another origin goes to the browser and is never shown",
        !offsite.is_empty() && !offsite_shown,
        json!({ "routed": offsite.len(), "first_request_reached_server": offsite_seen }),
    );
    let asked = recorded("asked", |d| {
        d["origin"].as_str() == Some(&format!("http://localhost:{p}"))
    });
    let redirected_shown =
        !recorded("report", |d| d["fields"]["name"] == "redirected-out").is_empty();
    check(
        "3c a redirect to an unknown origin while landing stops and asks",
        !asked.is_empty() && !redirected_shown,
        json!({}),
    );
    let popup = recorded("site:routed", |d| {
        d["url"]
            .as_str()
            .is_some_and(|u| u.contains("popup-target"))
    });
    let popup_shown = !recorded("report", |d| d["fields"]["name"] == "popup-target").is_empty();
    let blank_here = !recorded("report", |d| {
        d["fields"]["name"] == "blank-target"
            && d["host"].as_str() == Some(&format!("127.0.0.1:{p}"))
    })
    .is_empty();
    check(
        "4 window.open opens no window; target=_blank on its own origin loads in place",
        !popup.is_empty() && !popup_shown && blank_here,
        json!({ "popup_routed": popup.len(), "blank_in_place": blank_here }),
    );
    let saved = recorded("site:download", |d| d["state"] == "saved");
    let dir = std::env::var("SITE_PROBE_DIR")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Downloads");
    // The name as saved: WebView2 cleans a hostile name itself before knobas sees it (`../../probe.txt` arrives as
    // `_.._probe.txt`); either way the file is in Downloads and nowhere else.
    let on_disk = std::fs::read_dir(&dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with("probe.txt"))
    }) && !dir.parent().is_some_and(|up| up.join("probe.txt").exists());
    check(
        "5 a download is saved under Downloads, its name cleaned",
        !saved.is_empty() && on_disk,
        json!({ "saved": saved.len(), "in": dir.display().to_string(), "files": std::fs::read_dir(&dir).map(|e| e.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>()).unwrap_or_default() }),
    );
    let unsaved = recorded("unsaved", |d| {
        d["sources"]
            .as_array()
            .is_some_and(|a| a.iter().any(|s| s == "a"))
    });
    let close_asked = recorded("close-asked", |d| {
        d["answer"].as_str().is_some_and(|a| a.contains("Unsaved"))
    });
    check(
        "6 the probe answers changed for a page that objects, and close asks",
        !unsaved.is_empty() && !close_asked.is_empty(),
        json!({}),
    );
    let typed = !recorded("report", |d| {
        d["fields"]["ev"] == "keydown"
            && d["fields"]["name"] == "keys"
            && d["fields"]["key"]
                .as_str()
                .is_some_and(|k| k.eq_ignore_ascii_case("x"))
    })
    .is_empty();
    let escapes = recorded("report", |d| {
        d["fields"]["ev"] == "keydown"
            && d["fields"]["name"] == "keys"
            && d["fields"]["key"] == "Escape"
    })
    .len();
    let armed = !recorded("site:key", |d| d["key"] == "esc-armed").is_empty();
    let left = !recorded("site:key", |d| d["key"] == "esc-leave").is_empty();
    let claimed_name = if cfg!(windows) { "reload" } else { "search" };
    let claimed = !recorded("site:key", |d| d["key"] == claimed_name).is_empty();
    let claimed_reached = !recorded("report", |d| {
        d["fields"]["ev"] == "keydown"
            && d["fields"]["name"] == "keys"
            && (d["fields"]["key"] == "F5" || d["fields"]["key"] == "k")
    })
    .is_empty();
    // A posted key reaches the native hook whatever window is in front; it reaches the page only when the window has
    // the keyboard, which a probe started from another machine's shell may not: `typed` is said, the hand list checks it.
    let _ = escapes;
    check(
        "7 the hook passes the first Esc and takes the second; a claimed key never reaches the page",
        armed && left && claimed && !claimed_reached,
        json!({ "typed": typed, "escapes_in_page": escapes, "armed": armed, "left": left, "claimed": claimed, "claimed_reached_page": claimed_reached }),
    );
    let perms = recorded("report", |d| {
        d["fields"]["name"] == "perms" && d["fields"]["ev"] == "env"
    })
    .into_iter()
    .next();
    let perms = perms
        .map(|l| l["data"]["fields"].clone())
        .unwrap_or(Value::Null);
    let refused = |k: &str| {
        let v = perms[k].as_str().unwrap_or_default();
        !v.is_empty() && v != "granted"
    };
    check(
        "9 microphone, location, clipboard read and notifications are refused",
        refused("mic")
            && refused("geo")
            && refused("clip")
            && perms["notify"].as_str().is_some_and(|v| v != "granted"),
        perms.clone(),
    );
    let worst = recorded("shell-ipc", |d| d.get("worstFrame").is_some())
        .iter()
        .filter_map(|l| l["data"]["worstFrame"].as_f64())
        .fold(0.0_f64, f64::max);
    check("10 timings", true, json!({ "shell_worst_frame_ms": worst }));
}

fn main() {
    START.set(Instant::now()).ok();
    let dir = std::env::var_os("SITE_PROBE_DIR").map_or_else(
        || std::env::temp_dir().join("knobas-site-probe"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&dir).expect("the probe's folder");
    let second = std::env::var_os("SITE_PROBE_SECOND").is_some();
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(if second { "probe-2.log" } else { "probe-1.log" }))
    {
        LOG.set(Mutex::new(file)).ok();
    }
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("knobas_site=debug")
        .try_init();
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind the site");
    PORT.store(listener.local_addr().unwrap().port(), Ordering::Relaxed);
    log(
        "start",
        json!({ "port": port(), "webview": wry::webview_version().unwrap_or_default(), "dir": dir.display().to_string(), "os": std::env::consts::OS }),
    );
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || handle(stream));
        }
    });

    let event_loop = EventLoopBuilder::<Task>::with_user_event().build();
    let window = Rc::new(
        WindowBuilder::new()
            .with_title("knobas site probe")
            .with_inner_size(tao::dpi::LogicalSize::new(1200.0, 800.0))
            .build(&event_loop)
            .expect("the window"),
    );
    // The stand-in shell: a scheme of its own and an IPC handler, as knobas's page has.
    let shell_view = wry::WebViewBuilder::new()
        .with_html(SHELL)
        .with_custom_protocol("knobas-render".into(), |_, _| {
            log("shell-scheme-hit", json!({}));
            wry::http::Response::builder()
                .body(std::borrow::Cow::Borrowed(&b"knobas's own"[..]))
                .unwrap()
        })
        .with_ipc_handler(|request| {
            let body = request.body();
            let value: Value = serde_json::from_str(body).unwrap_or(json!({ "raw": body }));
            if value
                .get("worstFrame")
                .is_some_and(|w| w.as_f64().unwrap_or(0.0) < 40.0)
            {
                return;
            }
            log("shell-ipc", value);
        })
        .build(&*window)
        .expect("the shell's web view");

    let proxy = event_loop.create_proxy();
    let size = window.inner_size().to_logical::<f64>(window.scale_factor());
    let shell = Arc::new(ProbeShell {
        proxy: Mutex::new(proxy.clone()),
        main: std::thread::current().id(),
        size: Mutex::new((size.width, size.height)),
    });
    let sites = Sites::new(
        shell.clone(),
        Config {
            profile_dir: dir.clone(),
            keychain_service: "dev.knobas.site-probe".into(),
            downloads_dir: Some(dir.join("Downloads")),
            devtools: false,
            platform: if cfg!(target_os = "macos") {
                Platform::Mac
            } else {
                Platform::Windows
            },
        },
    );
    std::fs::create_dir_all(dir.join("Downloads")).ok();
    let shell_base = Base::parse(&format!("http://127.0.0.1:{}", port())).unwrap();
    let rules = |me: &str| SiteRules {
        base: shell_base.clone(),
        allowed: vec![],
        others: ["a", "b"]
            .iter()
            .filter(|o| **o != me)
            .map(|o| OtherSite {
                source: (*o).into(),
                base: shell_base.clone(),
            })
            .collect(),
    };
    sites.set_rules(HashMap::from([
        ("a".to_owned(), rules("a")),
        ("b".to_owned(), rules("b")),
    ]));
    let focus_window = Rc::clone(&window);
    let _ = focus_window;
    install_host(Box::new(ProbeHost(WryHost::new(
        Rc::clone(&window),
        Rc::new(|| log("focus-shell", json!({}))),
    ))));
    #[cfg(target_os = "macos")]
    {
        let keys = sites.clone();
        knobas_site::mac::set_shell_keys(Rc::new(move |press| keys.shell_key(press)));
    }

    let steps_sites = sites.clone();
    std::thread::spawn(move || steps(steps_sites, proxy, second));
    let _keep = shell_view;
    let relayout = sites.clone();
    event_loop.run(move |event, _, control| {
        *control = ControlFlow::Wait;
        match event {
            Event::UserEvent(task) => task(),
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => {
                let size = window.inner_size().to_logical::<f64>(window.scale_factor());
                *shell.size.lock().unwrap() = (size.width, size.height);
                relayout.relayout();
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control = ControlFlow::Exit,
            _ => {}
        }
    });
}
