//! What the site controller asks of a platform (decisions §19): make a view,
//! and act on one. `wry_host.rs` implements it over wry on macOS and Windows;
//! `fake.rs` records every call for the tests, on every platform.
//!
//! Every method runs on the main thread: `wry::WebView` is not `Send`.

use std::path::PathBuf;
use std::rc::Rc;

use crate::keys::{Claim, Press};
use crate::scripts::Dirty;
use crate::state::SiteFailure;

/// A rectangle in the main window, in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// What a view is made with.
#[derive(Clone, Debug)]
pub struct ViewSpec {
    pub source: String,
    pub url: String,
    pub bounds: Rect,
    /// Windows: the WebView2 user data folder (decisions §5).
    pub data_dir: PathBuf,
    /// macOS 14 and later: the `WKWebsiteDataStore` identifier.
    pub data_store: [u8; 16],
    /// The theme's paper, so no white rectangle flashes before the first
    /// commit. RGBA.
    pub background: (u8, u8, u8, u8),
    /// The window's zoom level.
    pub zoom: f64,
    /// Developer tools: a debug build only.
    pub devtools: bool,
}

/// What a view reports, on the main thread, as it happens.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewEvent {
    /// A top-level page committed at this address.
    Committed(String),
    /// A load finished.
    Finished(String),
    /// The page's title, raw.
    Title(String),
    /// The address, history and loading state, read from the view.
    History {
        url: Option<String>,
        can_back: bool,
        can_forward: bool,
        /// `None` where the platform's event does not say.
        loading: Option<bool>,
    },
    /// Nothing loaded.
    Failed(SiteFailure),
    /// The site's process ended.
    Ended,
    /// A download finished: the id knobas gave it, and whether it saved.
    Downloaded { id: u64, ok: bool },
}

/// A view's key hook: what to do with a key going down.
pub type KeyHook = Rc<dyn Fn(&Press) -> Claim>;
/// A view's download hook: from the address and the suggested name, where
/// to save and the id knobas gave it, or `None` to refuse.
pub type DownloadHook = Rc<dyn Fn(&str, &str) -> Option<(PathBuf, u64)>>;

/// The callbacks a view calls on the main thread. Each does no work beyond
/// reading the rules and posting what follows (decisions §19): WebView2
/// forbids making a web view from inside one of its callbacks.
#[derive(Clone)]
pub struct Hooks {
    /// A top-level navigation, a redirect hop or a new-window request:
    /// whether it loads in this view.
    pub navigate: Rc<dyn Fn(&str) -> bool>,
    /// A new-window request: never a window. The address goes through the
    /// same rule, and loads in this view when the site may show it.
    pub new_window: Rc<dyn Fn(&str)>,
    /// A key going down while the view holds the keyboard.
    pub key: KeyHook,
    /// Everything else the view reports.
    pub event: Rc<dyn Fn(ViewEvent)>,
    /// A download starts from `url` with the name the site suggested: the
    /// full path to save it at, and the id knobas gave it, or `None` to
    /// refuse it.
    pub download: DownloadHook,
}

/// Errors a platform reports, as text: they go to the log and the
/// rectangle, never further.
pub type HostError = String;

/// One site's native view.
pub trait SiteView {
    fn load(&self, url: &str);
    fn set_bounds(&self, bounds: Rect);
    fn set_visible(&self, visible: bool);
    /// The keyboard to the view.
    fn focus(&self);
    fn back(&self);
    fn forward(&self);
    fn reload(&self);
    fn stop(&self);
    fn zoom(&self, level: f64);
    /// Run [`crate::scripts::DIRTY_PROBE`] and hand its answer over, on the
    /// main thread, at most once. Never called back means no answer: the
    /// caller's timeout decides.
    fn probe(&self, answer: Box<dyn FnOnce(Dirty) + Send>);
    /// Sleep while hidden, or wake.
    fn sleep(&self, asleep: bool);
    /// Read the address, history and loading state now (the Mac has no
    /// event for `pushState`).
    fn poll(&self) -> Option<ViewEvent>;
    /// The main window moved (Windows: a page's drop-down follows it).
    fn window_moved(&self) {}
    /// The person already answered "leave anyway" in the bar: the page's
    /// own "Leave site?" for the next navigation, within [`LEAVE_FREELY`],
    /// is answered yes rather than asked a second time.
    fn leave_freely(&self) {}
}

/// How long a "leave anyway" answers the page's own leave dialog.
pub const LEAVE_FREELY: std::time::Duration = std::time::Duration::from_secs(3);

/// When the person last said "leave anyway" for one view.
#[derive(Clone, Default)]
pub struct LeaveFreely(Rc<std::cell::Cell<Option<std::time::Instant>>>);

impl LeaveFreely {
    /// The person said leave.
    pub fn arm(&self) {
        self.0.set(Some(std::time::Instant::now()));
    }

    /// Whether a leave dialog now is answered yes; it answers once.
    #[must_use]
    pub fn take(&self) -> bool {
        self.0.take().is_some_and(|at| at.elapsed() < LEAVE_FREELY)
    }
}

/// The platform: makes views, gives the keyboard back to knobas's own page,
/// and removes sessions.
pub trait SiteHost {
    /// Make a hidden view of `spec`. The view calls `hooks` from then on.
    ///
    /// # Errors
    ///
    /// The platform's, as text.
    fn make(&self, spec: &ViewSpec, hooks: Hooks) -> Result<Box<dyn SiteView>, HostError>;
    /// The keyboard to knobas's own web view.
    fn focus_shell(&self);
    /// Remove `source`'s session, after its view was dropped. `true` when it
    /// is gone; `false` when it must wait for the next start (the folder is
    /// still held).
    fn forget(&self, spec: &ViewSpec) -> bool;
}
