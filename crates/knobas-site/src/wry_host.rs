//! The platform over wry (decisions §3, §11, §21, §22): a site is a raw
//! `wry::WebView` made with `build_as_child` on knobas's main window. It is
//! given no IPC handler, no custom protocol and no initialization script
//! (`tests/scan.rs` refuses the five names), so the page in it reaches nothing
//! of knobas.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use raw_window_handle::HasWindowHandle;
use wry::{NewWindowResponse, PageLoadEvent, WebContext, WebView, WebViewBuilder};

use crate::confine::{Confine, Gate, NOT_CONFINED};
use crate::host::{Hooks, HostError, LeaveFreely, Rect, SiteHost, SiteView, ViewEvent, ViewSpec};
use crate::state::SiteFailure;
use crate::scripts::{DIRTY_PROBE, Dirty, SignInCall, fill_sign_in};

/// knobas's main window, and how to give its own web view the keyboard.
pub struct WryHost<W: HasWindowHandle + 'static> {
    window: W,
    focus_shell: Rc<dyn Fn()>,
}

impl<W: HasWindowHandle + 'static> WryHost<W> {
    /// The host over `window`. `focus_shell` gives knobas's own web view the
    /// keyboard: Tauri's `Webview::set_focus` (decisions §21 item 3: wry's
    /// `focus_parent` gives it to nobody on the Mac).
    pub fn new(window: W, focus_shell: Rc<dyn Fn()>) -> Self {
        Self {
            window,
            focus_shell,
        }
    }
}

/// `bounds`, measured in knobas's page, in the content view wry places a
/// child in. On the Mac the page may start below the content view's top.
fn placed(view: &WebView, bounds: Rect) -> wry::Rect {
    #[cfg(target_os = "macos")]
    {
        wry_rect(crate::glue::under_bar(
            bounds,
            crate::mac::under_title(view),
        ))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = view;
        wry_rect(bounds)
    }
}

fn wry_rect(bounds: Rect) -> wry::Rect {
    wry::Rect {
        position: dpi::LogicalPosition::new(bounds.x, bounds.y).into(),
        size: dpi::LogicalSize::new(bounds.width, bounds.height).into(),
    }
}

/// A view's confinement (ux30): what it may load now, shared with the
/// platform's handlers so a change applies at once, and the gate its loads
/// wait at while the refusal is not in place.
#[derive(Clone)]
pub struct Confined {
    pub rules: Rc<RefCell<Option<Confine>>>,
    pub gate: Rc<Gate>,
    pub source: String,
    pub event: Rc<dyn Fn(ViewEvent)>,
    /// The rule list being compiled (macOS): an older one that lands late is
    /// dropped.
    pub generation: Rc<Cell<u64>>,
    /// Whether the request handler is registered (Windows).
    pub registered: Rc<Cell<bool>>,
}

impl Confined {
    /// Put the current rules in place on `view`, holding loads until they
    /// are. With no rules, nothing is refused and loads go through.
    fn apply(&self, view: &WebView) {
        let rules = self.rules.borrow().clone();
        #[cfg(target_os = "macos")]
        crate::mac::confine(view, self, rules.as_ref());
        #[cfg(windows)]
        {
            if rules.is_some() && !self.registered.get() {
                match crate::win::confine(view, self) {
                    Ok(()) => self.registered.set(true),
                    Err(error) => {
                        self.refused(&error.to_string());
                        return;
                    }
                }
            }
            if let Some(url) = self.gate.open() {
                load_now(view, &url);
            }
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = (view, rules);
    }

    /// The refusal could not be put in place: nothing loads, and the site
    /// says so with *Retry* (fail closed).
    pub fn refused(&self, why: &str) {
        tracing::warn!(source = %self.source, error = why, "a site's refusal of other websites is not in place: it loads nothing");
        self.gate.shut();
        (self.event)(ViewEvent::Failed(SiteFailure::Other(NOT_CONFINED.to_owned())));
    }
}

fn load_now(view: &WebView, url: &str) {
    if let Err(error) = view.load_url(url) {
        tracing::warn!(%error, "a site could not load an address");
    }
}

/// One site's view, and what must live as long as it.
pub struct WryView {
    view: WebView,
    confined: Confined,
    /// The person said "leave anyway": the page's own leave dialog is answered.
    leaving: LeaveFreely,
    /// The session folder's context (Windows), kept beside its view.
    _context: Box<WebContext>,
    #[cfg(target_os = "macos")]
    _mac: crate::mac::Wrapped,
    #[cfg(windows)]
    _windows: crate::win::Hardened,
}

impl<W: HasWindowHandle + 'static> SiteHost for WryHost<W> {
    #[allow(clippy::too_many_lines)]
    fn make(&self, spec: &ViewSpec, hooks: Hooks) -> Result<Box<dyn SiteView>, HostError> {
        let mut context = Box::new(WebContext::new(Some(spec.data_dir.clone())));
        // Downloads: the path knobas chose, and the id it gave it, by URL,
        // since the Mac reports no path when one completes.
        let ids: Rc<RefCell<HashMap<String, u64>>> = Rc::default();
        let started_ids = Rc::clone(&ids);
        let started_hook = Rc::clone(&hooks.download);
        let done_ids = Rc::clone(&ids);
        let done_hook = Rc::clone(&hooks.event);
        let nav_hook = Rc::clone(&hooks.navigate);
        let new_hook = Rc::clone(&hooks.new_window);
        let title_hook = Rc::clone(&hooks.event);
        let load_hook = Rc::clone(&hooks.event);
        let (r, g, b, a) = spec.background;
        let confined = Confined {
            rules: Rc::new(RefCell::new(spec.confine.clone())),
            // A confined view loads nothing until its refusal is in place.
            gate: Rc::new(if spec.confine.is_some() {
                Gate::waiting(&spec.url)
            } else {
                Gate::opened()
            }),
            source: spec.source.clone(),
            event: Rc::clone(&hooks.event),
            generation: Rc::default(),
            registered: Rc::default(),
        };
        let builder = WebViewBuilder::new_with_web_context(&mut context);
        #[allow(unused_mut)]
        let mut builder = if spec.confine.is_some() {
            builder
        } else {
            builder.with_url(spec.url.as_str())
        }
        .with_bounds(wry_rect(spec.bounds))
            .with_visible(false)
            .with_focused(false)
            .with_devtools(spec.devtools)
            .with_clipboard(false)
            .with_general_autofill_enabled(false)
            .with_hotkeys_zoom(false)
            .with_background_color((r, g, b, a))
            .with_back_forward_navigation_gestures(true)
            .with_accept_first_mouse(true)
            .with_navigation_handler(move |url| nav_hook(&url))
            .with_new_window_req_handler(move |url, _features| {
                new_hook(&url);
                NewWindowResponse::Deny
            })
            .with_download_started_handler(move |url, path| {
                let suggested = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match started_hook(&url, &suggested) {
                    Some((chosen, id)) => {
                        *path = chosen;
                        started_ids.borrow_mut().insert(url, id);
                        true
                    }
                    None => false,
                }
            })
            .with_download_completed_handler(move |url, _path: Option<PathBuf>, ok| {
                if let Some(id) = done_ids.borrow_mut().remove(&url) {
                    done_hook(ViewEvent::Downloaded { id, ok });
                }
            })
            .with_document_title_changed_handler(move |title| title_hook(ViewEvent::Title(title)))
            .with_on_page_load_handler(move |event, url| {
                load_hook(match event {
                    PageLoadEvent::Started => ViewEvent::Committed(url),
                    PageLoadEvent::Finished => ViewEvent::Finished(url),
                });
            });
        #[cfg(target_os = "macos")]
        {
            builder = crate::mac::builder(builder, spec, &hooks);
        }
        #[cfg(windows)]
        {
            builder = crate::win::builder(builder);
        }
        let started = std::time::Instant::now();
        let view = builder
            .build_as_child(&self.window)
            .map_err(|error| error.to_string())?;
        tracing::info!(
            ms = started.elapsed().as_secs_f64() * 1000.0,
            "site view built"
        );
        let _ = view.zoom(spec.zoom);
        let _ = view.set_bounds(placed(&view, spec.bounds));
        let leaving = LeaveFreely::default();
        #[cfg(target_os = "macos")]
        let mac = crate::mac::wrap(&view, &spec.source, &hooks, &leaving, &confined.rules);
        #[cfg(windows)]
        let windows = crate::win::harden(&view, &hooks, &leaving);
        if spec.confine.is_some() {
            confined.apply(&view);
        }
        Ok(Box::new(WryView {
            view,
            confined,
            leaving,
            _context: context,
            #[cfg(target_os = "macos")]
            _mac: mac,
            #[cfg(windows)]
            _windows: windows,
        }))
    }

    fn focus_shell(&self) {
        (self.focus_shell)();
    }

    fn flush(&self, processes: &[u32], deadline: std::time::Instant) {
        #[cfg(windows)]
        crate::win::wait_for_exit(processes, deadline);
        #[cfg(target_os = "macos")]
        crate::mac::flush(deadline);
        let _ = (processes, deadline);
    }

    fn forget(&self, spec: &ViewSpec, done: Box<dyn FnOnce(bool) + Send>) {
        #[cfg(target_os = "macos")]
        crate::mac::forget(spec.data_store, done);
        #[cfg(windows)]
        done(crate::glue::removed(&std::fs::remove_dir_all(
            &spec.data_dir,
        )));
    }
}

impl SiteView for WryView {
    fn load(&self, url: &str) {
        let gate = &self.confined.gate;
        if gate.is_shut() {
            // *Retry* after the refusal failed: try to put it in place again.
            gate.hold();
            let _ = gate.load(url);
            self.confined.apply(&self.view);
            return;
        }
        match gate.load(url) {
            Some(url) => load_now(&self.view, &url),
            None => tracing::debug!("a site's load waits for its refusal of other websites"),
        }
    }

    fn confine(&self, confine: Option<Confine>) {
        if *self.confined.rules.borrow() == confine {
            return;
        }
        *self.confined.rules.borrow_mut() = confine;
        self.confined.apply(&self.view);
    }

    fn set_bounds(&self, bounds: Rect) {
        let _ = self.view.set_bounds(placed(&self.view, bounds));
    }

    fn set_visible(&self, visible: bool) {
        let _ = self.view.set_visible(visible);
        // Windows: a hidden site holds less memory at once (decisions §14).
        #[cfg(windows)]
        crate::win::memory(&self.view, visible);
    }

    fn focus(&self) {
        let _ = self.view.focus();
    }

    fn back(&self) {
        #[cfg(target_os = "macos")]
        crate::mac::go(&self.view, crate::mac::Go::Back);
        #[cfg(windows)]
        crate::win::go(&self.view, crate::win::Go::Back);
    }

    fn forward(&self) {
        #[cfg(target_os = "macos")]
        crate::mac::go(&self.view, crate::mac::Go::Forward);
        #[cfg(windows)]
        crate::win::go(&self.view, crate::win::Go::Forward);
    }

    fn reload(&self) {
        if self.confined.gate.is_open() {
            let _ = self.view.reload();
        }
    }

    fn stop(&self) {
        #[cfg(target_os = "macos")]
        crate::mac::go(&self.view, crate::mac::Go::Stop);
        #[cfg(windows)]
        crate::win::go(&self.view, crate::win::Go::Stop);
    }

    fn zoom(&self, level: f64) {
        let _ = self.view.zoom(level);
    }

    fn probe(&self, answer: Box<dyn FnOnce(Dirty) + Send>) {
        let answer = std::sync::Mutex::new(Some(answer));
        let asked = self
            .view
            .evaluate_script_with_callback(DIRTY_PROBE, move |text| {
                let taken = answer.lock().ok().and_then(|mut slot| slot.take());
                if let Some(answer) = taken {
                    answer(Dirty::from_answer(text.trim()));
                }
            });
        if let Err(error) = asked {
            tracing::debug!(%error, "a site's page could not be asked about changes");
        }
    }

    fn url(&self) -> Option<String> {
        self.view.url().ok().and_then(crate::glue::page_address)
    }

    fn sign_in(&self, call: &SignInCall, answer: Box<dyn FnOnce(String) + Send>) {
        let answer = std::sync::Mutex::new(Some(answer));
        // The script carries the password: it is built here, handed to the
        // engine, and dropped; nothing logs it (wry's own span is the name
        // `wry::eval` alone).
        let asked =
            self.view
                .evaluate_script_with_callback(fill_sign_in(call).as_str(), move |text| {
                    let taken = answer.lock().ok().and_then(|mut slot| slot.take());
                    if let Some(answer) = taken {
                        answer(text);
                    }
                });
        if let Err(error) = asked {
            tracing::debug!(%error, "a site's page could not be asked to sign in");
        }
    }

    fn sleep(&self, asleep: bool) {
        #[cfg(windows)]
        crate::win::sleep(&self.view, asleep);
        #[cfg(not(windows))]
        let _ = asleep;
    }

    fn poll(&self) -> Option<ViewEvent> {
        #[cfg(target_os = "macos")]
        {
            Some(crate::mac::poll(&self.view))
        }
        #[cfg(windows)]
        {
            None
        }
    }

    fn window_moved(&self) {
        #[cfg(windows)]
        crate::win::moved(&self.view);
    }

    fn browser_process(&self) -> Option<u32> {
        #[cfg(windows)]
        {
            crate::win::browser_process(&self.view)
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    fn leave_freely(&self) {
        self.leaving.arm();
        // WebView2 asks "Leave site?" itself unless its own dialogs are off.
        #[cfg(windows)]
        crate::win::own_dialogs(&self.view, false);
    }
}
