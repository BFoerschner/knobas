//! *Forget…* on macOS ends a site's session, also one used earlier in this run.
//!
//! The final suite's real window found it (2026-11-22): Confluence signed in, *Forget…*, and the next open in the same
//! run was signed in again, and so was the one after a quit. WebKit's network process kept the store's cookies in
//! memory after its files were removed, handed them to the next view with that identifier, and wrote them back at the
//! quit. This puts a cookie into a store, forgets the store with `knobas_site::mac::forget`, and asks a store with the
//! same identifier for its cookies: none may be left.
//!
//! WebKit wants the main thread and a running run loop, so this test has no harness (`harness = false`) and spins
//! the main run loop itself. macOS 14 or later; nothing to test on another system.

#[cfg(target_os = "macos")]
fn main() {
    mac::run();
}

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
mod mac {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use block2::RcBlock;
    use std::io::{Read, Write};

    use objc2::rc::{Allocated, Retained, autoreleasepool};
    use objc2::runtime::AnyObject;
    use objc2::{MainThreadMarker, class, msg_send};
    use objc2_foundation::{
        NSArray, NSDate, NSDefaultRunLoopMode, NSHTTPCookie, NSPoint, NSRect, NSRunLoop, NSSize,
        NSString, NSURL, NSURLRequest, NSUUID,
    };
    use objc2_web_kit::{WKWebViewConfiguration, WKWebsiteDataStore};

    const NAME: &str = "knobas-forget-test";
    /// A session cookie, kept in memory only, as `JSESSIONID` is.
    const SESSION: &str = "knobas-forget-session";

    /// A page on 127.0.0.1 whose every answer sets `NAME` for an hour, as a product's "remember me" does.
    fn serve() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("its address").port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut buf = [0_u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nSet-Cookie: {NAME}=signed-in; Max-Age=3600; Path=/\r\nSet-Cookie: {SESSION}=live; Path=/\r\nContent-Type: text/html\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok").as_bytes());
            }
        });
        port
    }

    fn spin(done: impl Fn() -> bool, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            NSRunLoop::currentRunLoop().runMode_beforeDate(
                unsafe { NSDefaultRunLoopMode },
                &NSDate::dateWithTimeIntervalSinceNow(0.05),
            );
        }
    }

    fn cookies(store: &WKWebsiteDataStore) -> Vec<String> {
        let got: Rc<RefCell<Option<Vec<String>>>> = Rc::new(RefCell::new(None));
        autoreleasepool(|_| {
            let into = Rc::clone(&got);
            let block = RcBlock::new(move |all: std::ptr::NonNull<NSArray<NSHTTPCookie>>| {
                // SAFETY: WebKit hands a valid array for the length of the call.
                let all = unsafe { all.as_ref() };
                *into.borrow_mut() = Some(all.iter().map(|c| c.name().to_string()).collect());
            });
            // SAFETY: a WebKit call on the main thread, where the store was made.
            unsafe { store.httpCookieStore().getAllCookies(&block) };
        });
        spin(|| got.borrow().is_some(), "the store's cookies");
        got.borrow_mut().take().unwrap_or_default()
    }

    fn store(id: [u8; 16], mtm: MainThreadMarker) -> objc2::rc::Retained<WKWebsiteDataStore> {
        // SAFETY: on the main thread (`mtm`).
        unsafe { WKWebsiteDataStore::dataStoreForIdentifier(&NSUUID::from_bytes(id), mtm) }
    }

    /// `mac::forget` as `Sites::forget` drives it: tried again for a few seconds while WebKit holds the store.
    fn forget(id: [u8; 16]) -> bool {
        for _ in 0..8 {
            let answer: Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(None));
            let into = Arc::clone(&answer);
            knobas_site::mac::forget(id, Box::new(move |ok| *into.lock().unwrap() = Some(ok)));
            spin(|| answer.lock().unwrap().is_some(), "the forget to answer");
            if *answer.lock().unwrap() == Some(true) {
                return true;
            }
            let pause = Instant::now() + Duration::from_millis(500);
            spin(|| Instant::now() > pause, "a pause");
        }
        false
    }

    pub fn run() {
        if !knobas_site::mac::sessions_persist() {
            println!("mac_forget: skipped, no data stores by identifier before macOS 14");
            return;
        }
        let mtm = MainThreadMarker::new().expect("the test's main thread");
        let seed = std::process::id().to_le_bytes();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_le_bytes();
        let mut id = [0_u8; 16];
        id[..4].copy_from_slice(&seed);
        id[4..].copy_from_slice(&nanos[..12]);

        // A site's view: a page whose answer sets a lasting cookie, loaded into the store, then the view and the
        // store let go, as `Sites::forget` drops a site's view before it forgets the store.
        let port = serve();
        // knobas's own page: a second view, on the default store, alive the whole time, as in the app.
        let _shell = autoreleasepool(|_| unsafe {
            let config = WKWebViewConfiguration::new(mtm);
            let view: Allocated<AnyObject> = msg_send![class!(WKWebView), alloc];
            let view: Retained<AnyObject> = msg_send![view, initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(400.0, 300.0)), configuration: &*config];
            let url = NSURL::URLWithString(&NSString::from_str(&format!(
                "http://127.0.0.1:{port}/shell"
            )))
            .expect("a url");
            let _: Option<Retained<AnyObject>> =
                msg_send![&*view, loadRequest: &*NSURLRequest::requestWithURL(&url)];
            view
        });
        let before = autoreleasepool(|_| {
            let store = store(id, mtm);
            // SAFETY: WebKit and AppKit calls on the main thread (`mtm`), with the types the selectors take.
            unsafe {
                let config = WKWebViewConfiguration::new(mtm);
                config.setWebsiteDataStore(&store);
                let view: Allocated<AnyObject> = msg_send![class!(WKWebView), alloc];
                let view: Retained<AnyObject> = msg_send![view, initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(400.0, 300.0)), configuration: &*config];
                let url =
                    NSURL::URLWithString(&NSString::from_str(&format!("http://127.0.0.1:{port}/")))
                        .expect("a url");
                let _: Option<Retained<AnyObject>> =
                    msg_send![&*view, loadRequest: &*NSURLRequest::requestWithURL(&url)];
                spin(
                    || {
                        let c = cookies(&store);
                        c.iter().any(|n| n == NAME) && c.iter().any(|n| n == SESSION)
                    },
                    "the page's cookies",
                );
                let _: () = msg_send![&*view, stopLoading];
            }
            let names = cookies(&store);
            // The app's first try comes while WebKit still holds the store (its log: forgotten 0.65 s after the
            // close, on a later try).
            let first: Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(None));
            let into = Arc::clone(&first);
            knobas_site::mac::forget(id, Box::new(move |ok| *into.lock().unwrap() = Some(ok)));
            spin(|| first.lock().unwrap().is_some(), "the first forget");
            println!(
                "mac_forget: the first try, with the view alive, answered {:?}",
                *first.lock().unwrap()
            );
            names
        });
        assert!(
            before.iter().any(|n| n == NAME),
            "the cookie was not set: {before:?}"
        );
        let forgotten = forget(id);
        let left = autoreleasepool(|_| cookies(&store(id, mtm)));
        // Leave nothing behind, whatever the answer.
        forget(id);
        if let Some(exe) = std::env::current_exe()
            .ok()
            .and_then(|e| e.file_name().map(|n| n.to_owned()))
            && let Some(home) = std::env::var_os("HOME")
        {
            let _ = std::fs::remove_dir_all(
                std::path::Path::new(&home).join("Library/WebKit").join(exe),
            );
        }
        assert!(forgotten, "mac::forget never removed the store");
        assert!(
            left.is_empty(),
            "a forgotten store still hands out its cookies: {left:?}"
        );
        println!("test mac_forget::a_forgotten_store_hands_out_no_cookie ... ok");
    }
}
