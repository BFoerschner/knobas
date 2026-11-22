//! The live sites (decisions §14, §17, §19): their state, their rules, and
//! their native views.
//!
//! [`Sites`] is `Send + Sync` and holds, under one mutex, each site's state
//! and the rules every hook reads. The views themselves are not `Send`: they
//! live in a `thread_local!` map on the main thread, keyed by source id, and
//! every call on one runs there through [`Shell::on_main`]. No lock is held
//! across a call into a view, since a view may call its hooks before it
//! returns, and no command waits on the main thread from the main thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use crate::host::{Hooks, Rect, SiteHost, SiteView, ViewEvent, ViewSpec};
use crate::keys::{Claim, KeyName, Keyboard, Platform, Press};
use crate::policy::{Decision, Origin, Refusal, SiteRules, decide, download_name};
use crate::scripts::{Dirty, FillMode, SignInCall};
use crate::session;
use crate::sign_in::{FillAct, FillAnswer, SignInRule, is_sign_in_page, may_fill};
use crate::state::{
    KEPT_DEFAULT, KEPT_MAX, KEPT_MIN, Liveness, Site, SiteKeyboard, SitePhase, SiteStatus, choose,
    idle, room, sleepers,
};

/// How often *Forget…* tries to remove a session the platform still holds.
pub const FORGET_TRIES: u32 = 8;
/// Between two tries.
pub const FORGET_AGAIN: Duration = Duration::from_millis(500);

/// How long quitting waits, at most, for the sites' sessions to be written.
pub const QUIT_FLUSH: Duration = Duration::from_secs(3);

/// How long a page has to answer [`crate::scripts::DIRTY_PROBE`].
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(300);

/// The distance of a site's rectangle from the main window's four edges, in
/// logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, Serialize, TS)]
#[ts(export_to = "site.ts")]
pub struct SiteInsets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl SiteInsets {
    /// The rectangle in a window `width` by `height`.
    #[must_use]
    pub fn rect(&self, width: f64, height: f64) -> Rect {
        Rect {
            x: self.left,
            y: self.top,
            width: (width - self.left - self.right).max(1.0),
            height: (height - self.top - self.bottom).max(1.0),
        }
    }

    fn sane(self) -> Self {
        let clean = |v: f64| {
            if v.is_finite() {
                v.clamp(0.0, 100_000.0)
            } else {
                0.0
            }
        };
        Self {
            top: clean(self.top),
            right: clean(self.right),
            bottom: clean(self.bottom),
            left: clean(self.left),
        }
    }
}

/// What a closing, reloading or re-pointing command answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "site.ts")]
pub struct SiteAsked {
    /// The act happened.
    pub done: bool,
    /// Why it did not: the page holds changes it has not saved, and the
    /// person has to say.
    pub asks: Option<SiteAsks>,
}

impl SiteAsked {
    const DONE: Self = Self {
        done: true,
        asks: None,
    };
    const UNSAVED: Self = Self {
        done: false,
        asks: Some(SiteAsks::Unsaved),
    };
}

/// The one question a command can answer with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "site.ts")]
pub enum SiteAsks {
    Unsaved,
}

/// What `site_do` does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export_to = "site.ts")]
pub enum SiteAct {
    Back,
    Forward,
    Reload,
    Stop,
    Focus,
    FocusShell,
}

/// Where a navigation the site may not show went (`site:routed`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "site.ts")]
pub struct SiteRouted {
    pub source: String,
    pub to: SiteRoutedTo,
    /// The host, or the scheme for a refused address.
    pub host: String,
    /// The address, for the page to hand to `openExternal` or `openWeb`.
    /// Empty for a refused address.
    pub url: String,
    /// For [`SiteRoutedTo::Site`]: the source whose site takes it.
    pub other: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "site.ts")]
pub enum SiteRoutedTo {
    Browser,
    Site,
    Refused,
}

/// A download's progress (`site:download`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "site.ts")]
pub struct SiteDownload {
    pub source: String,
    pub id: u32,
    pub name: String,
    pub state: SiteDownloadState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "site.ts")]
pub enum SiteDownloadState {
    Started,
    Saved,
    Failed,
}

/// A view closed (`site:closed`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "site.ts")]
pub struct SiteClosed {
    pub source: String,
    pub why: SiteClosedWhy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "site.ts")]
pub enum SiteClosedWhy {
    /// The person closed it: the bar's ×, ⌘W.
    Asked,
    /// One more site was opened than *Sites kept open*.
    Room,
    /// Hidden for two hours with nothing unsaved.
    Idle,
    /// *Forget…*, or its source was deleted.
    Forgot,
    /// It never showed a page and the person declined the question.
    Declined,
}

/// A claimed key (`site:key`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "site.ts")]
pub struct SiteKey {
    pub source: String,
    /// A name, never a key code: `search`, `settings`, `timer`, `reload`,
    /// `back`, `forward`, `zoom-in`, `zoom-out`, `zoom-reset`, `close`,
    /// `bar`, `esc-armed`, `esc-leave`, `quit`.
    pub key: String,
}

/// Everything the controller tells the page.
#[derive(Clone, Debug, PartialEq)]
pub enum SiteEvent {
    Status(SiteStatus),
    Key(SiteKey),
    Routed(SiteRouted),
    Download(SiteDownload),
    Closed(SiteClosed),
}

impl SiteEvent {
    /// The event's name on the bridge.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Status(_) => "site:status",
            Self::Key(_) => "site:key",
            Self::Routed(_) => "site:routed",
            Self::Download(_) => "site:download",
            Self::Closed(_) => "site:closed",
        }
    }
}

/// What the controller needs from the application around it.
pub trait Shell: Send + Sync + 'static {
    /// Run `f` on the main thread: at once when already there.
    fn on_main(&self, f: Box<dyn FnOnce() + Send>);
    /// Run `f` on the main thread soon, never before this returns. What a
    /// hook wants done goes through here, since a hook runs inside the
    /// platform's own callback.
    fn later(&self, f: Box<dyn FnOnce() + Send>);
    /// Tell the page, never from inside the caller's own call.
    fn emit(&self, event: SiteEvent);
    /// Milliseconds on a clock that only goes forward.
    fn now(&self) -> f64;
    /// The main window's inner size in logical pixels. On the main thread.
    fn window_size(&self) -> (f64, f64);
    /// ⌘Q in a site on the Mac: quit, asking the sites first.
    fn quit(&self);
    /// A page on `source`'s own address committed for the first time in
    /// this view: its sign-in is kept from now (*Kept since*).
    fn landed(&self, source: &str);
    /// `source`'s site wants its saved web sign-in typed in (ux29): read it
    /// from the keychain item and hand it to [`Sites::fill_with`], or call
    /// [`Sites::fill_none`] when there is none. Never from inside the
    /// caller's own call.
    fn sign_in(&self, source: &str);
}

/// The fixed parts of the controller.
#[derive(Clone, Debug)]
pub struct Config {
    /// `Profile::dir`.
    pub profile_dir: PathBuf,
    /// `Profile::keychain_service()`.
    pub keychain_service: String,
    /// Where downloads go.
    pub downloads_dir: Option<PathBuf>,
    pub devtools: bool,
    pub platform: Platform,
}

/// Why a command could not act.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SiteError {
    #[error("{0}")]
    Invalid(String),
    #[error("this source's pages do not open inside knobas")]
    NotOffered,
    #[error("the site is not open")]
    NotOpen,
    #[error("the site could not be made: {0}")]
    Platform(String),
}

#[derive(Default)]
struct Book {
    sites: HashMap<String, Site>,
    rules: HashMap<String, SiteRules>,
    keyboards: HashMap<String, Keyboard>,
    insets: HashMap<String, SiteInsets>,
    /// Each closed site's last address, until knobas quits.
    last_url: HashMap<String, String>,
    /// The origins *Open in ⟨source⟩* named that went to the browser.
    suggestions: HashMap<String, Vec<Origin>>,
    downloads: HashMap<u32, (String, PathBuf)>,
    next_download: u32,
    zoom: f64,
    kept: u32,
    background: (u8, u8, u8, u8),
    /// The last status sent per site, so an unchanged one is not sent again.
    sent: HashMap<String, SiteStatus>,
    /// The site that held the keyboard when the window lost focus.
    keyboard_on_blur: Option<String>,
    /// Where each source's site asks for a sign-in, and whether one is
    /// saved (ux29).
    sign_ins: HashMap<String, SignInRule>,
}

/// How long a remade view waits for its old browser process to end
/// ([`Sites::reconfine`]); past it, making it again fails and says so.
pub const REMAKE_WAIT: Duration = Duration::from_secs(5);

thread_local! {
    /// The views, on the main thread only: `wry::WebView` is not `Send`.
    static VIEWS: RefCell<HashMap<String, Rc<dyn SiteView>>> = RefCell::new(HashMap::new());
    /// The platform, installed on the main thread at bring-up.
    static HOST: RefCell<Option<Rc<dyn SiteHost>>> = const { RefCell::new(None) };
}

fn view(source: &str) -> Option<Rc<dyn SiteView>> {
    VIEWS.with(|views| views.borrow().get(source).cloned())
}

fn host() -> Option<Rc<dyn SiteHost>> {
    HOST.with(|host| host.borrow().clone())
}

/// Install the platform. On the main thread, once, before any site opens.
pub fn install_host(host: Box<dyn SiteHost>) {
    let host: Rc<dyn SiteHost> = Rc::from(host);
    HOST.with(|slot| *slot.borrow_mut() = Some(host));
}

/// Every live site and its rules, behind one handle.
#[derive(Clone)]
pub struct Sites {
    inner: Arc<Inner>,
}

struct Inner {
    shell: Arc<dyn Shell>,
    config: Config,
    book: Mutex<Book>,
}

impl Sites {
    /// A controller with no site open. Deletes what the forget list names
    /// before any site is made.
    #[must_use]
    pub fn new(shell: Arc<dyn Shell>, config: Config) -> Self {
        let left = session::forget_listed(&config.profile_dir);
        if !left.is_empty() {
            tracing::warn!(
                count = left.len(),
                "site sessions still held; deleted at the next start"
            );
        }
        Self {
            inner: Arc::new(Inner {
                shell,
                config,
                book: Mutex::new(Book {
                    zoom: 1.0,
                    kept: KEPT_DEFAULT,
                    background: (0xf4, 0xf1, 0xea, 0xff),
                    ..Book::default()
                }),
            }),
        }
    }

    fn book(&self) -> MutexGuard<'_, Book> {
        self.inner
            .book
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn shell(&self) -> &Arc<dyn Shell> {
        &self.inner.shell
    }

    fn now(&self) -> f64 {
        self.inner.shell.now()
    }

    /// The rules of every source whose pages open inside knobas. A source
    /// missing from `rules` opens no site; one already open stays open until
    /// it is closed.
    ///
    /// A live site whose confinement changed (its allow-list, ux30) gets the
    /// new one on the main thread ([`Self::reconfine`]); its view holds the
    /// loads knobas starts until the platform's refusal follows.
    pub fn set_rules(&self, rules: HashMap<String, SiteRules>) {
        let changed: Vec<(String, Option<crate::confine::Confine>)> = {
            let mut book = self.book();
            let changed = book
                .sites
                .keys()
                .filter_map(|source| {
                    let old = book.rules.get(source).and_then(SiteRules::confine);
                    let new = rules.get(source).and_then(SiteRules::confine);
                    (old != new).then(|| (source.clone(), new))
                })
                .collect();
            book.rules = rules;
            changed
        };
        if changed.is_empty() {
            return;
        }
        let this = self.clone();
        self.shell().on_main(Box::new(move || {
            for (source, confine) in changed {
                this.reconfine(&source, confine, None);
            }
        }));
    }

    /// `source`'s live view takes what it may load now, then loads `then`
    /// when given. On the main thread.
    ///
    /// On a platform that reads it only when a view is made
    /// ([`SiteHost::remakes_to_confine`], Windows): the view is dropped, its
    /// browser process waited for (at most [`REMAKE_WAIT`]: WebView2 refuses
    /// other arguments on a session folder whose browser still runs), and
    /// the view made again at `then`, else where it was, as a reload. One
    /// that cannot be made again loads nothing and says so with *Retry*.
    fn reconfine(
        &self,
        source: &str,
        confine: Option<crate::confine::Confine>,
        then: Option<String>,
    ) {
        let Some(view) = view(source) else {
            return;
        };
        if !host().is_some_and(|host| host.remakes_to_confine()) {
            view.confine(confine);
            if let Some(url) = &then {
                view.load(url);
            }
            return;
        }
        let here = then.or_else(|| view.url());
        let url = {
            let now = self.now();
            let mut book = self.book();
            let Some(site) = book.sites.get_mut(source) else {
                return;
            };
            let url = here
                .or_else(|| site.url.clone())
                .unwrap_or_else(|| site.landing_url.clone());
            site.land(&url, now);
            url
        };
        let processes: Vec<u32> = view.browser_process().into_iter().collect();
        VIEWS.with(|views| views.borrow_mut().remove(source));
        drop(view);
        tracing::info!(
            source,
            "a confined site's origins changed: its view is made again"
        );
        let this = self.clone();
        let source = source.to_owned();
        if processes.is_empty() {
            self.shell()
                .later(Box::new(move || this.remade(&source, &url)));
        } else {
            let shell = Arc::clone(self.shell());
            std::thread::spawn(move || {
                crate::host::wait_for_exit(&processes, std::time::Instant::now() + REMAKE_WAIT);
                shell.on_main(Box::new(move || this.remade(&source, &url)));
            });
        }
    }

    /// The view [`Self::reconfine`] dropped, made again at `url`, unless the
    /// site closed meanwhile. On the main thread.
    fn remade(&self, source: &str, url: &str) {
        if view(source).is_some() || !self.book().sites.contains_key(source) {
            return;
        }
        self.make(source, url);
        if view(source).is_none() {
            if let Some(site) = self.book().sites.get_mut(source) {
                site.fail(crate::state::SiteFailure::Other(
                    crate::confine::NOT_CONFINED.to_owned(),
                ));
            }
            self.sync(source);
        }
    }

    /// Where each source's site asks for a sign-in and whether one is saved
    /// (ux29). A site whose saved sign-in came or went is armed again.
    pub fn set_sign_ins(&self, sign_ins: HashMap<String, SignInRule>) {
        let changed: Vec<String> = {
            let mut book = self.book();
            let mut changed = Vec::new();
            for site in book.sites.values() {
                let saved = sign_ins.get(&site.source).is_some_and(|rule| rule.saved);
                if saved != site.sign_in_saved {
                    changed.push(site.source.clone());
                }
            }
            for source in &changed {
                let saved = sign_ins.get(source).is_some_and(|rule| rule.saved);
                if let Some(site) = book.sites.get_mut(source) {
                    site.sign_in_saved = saved;
                    site.filler.rearm();
                }
            }
            book.sign_ins = sign_ins;
            changed
        };
        for source in changed {
            self.sync_later(&source);
        }
    }

    /// The rules of `source`, when its pages open inside knobas.
    #[must_use]
    pub fn rules(&self, source: &str) -> Option<SiteRules> {
        self.book().rules.get(source).cloned()
    }

    /// The paper of the current theme, for views made from now on.
    pub fn set_background(&self, rgba: (u8, u8, u8, u8)) {
        self.book().background = rgba;
    }

    /// *Sites kept open*.
    pub fn set_kept(&self, kept: u32) {
        self.book().kept = kept.clamp(KEPT_MIN, KEPT_MAX);
    }

    /// An origin *Open in ⟨source⟩* named that went to the browser: offered
    /// in Settings with *Allow* until knobas quits.
    pub fn suggest(&self, source: &str, origin: Origin) {
        let mut book = self.book();
        if book
            .rules
            .get(source)
            .is_some_and(|rules| rules.allows(&origin))
        {
            return;
        }
        let list = book.suggestions.entry(source.to_owned()).or_default();
        if !list.contains(&origin) {
            list.push(origin);
        }
    }

    /// The suggestions for `source`, minus what was allowed since.
    #[must_use]
    pub fn suggestions(&self, source: &str) -> Vec<Origin> {
        let book = self.book();
        let allowed = book.rules.get(source);
        book.suggestions
            .get(source)
            .map(|list| {
                list.iter()
                    .filter(|origin| !allowed.is_some_and(|rules| rules.allows(origin)))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether any site is alive.
    #[must_use]
    pub fn alive(&self) -> bool {
        !self.book().sites.is_empty()
    }

    /// Every live site's status.
    #[must_use]
    pub fn statuses(&self) -> Vec<SiteStatus> {
        let book = self.book();
        let mut out: Vec<SiteStatus> = book.sites.values().map(Site::status).collect();
        out.sort_by(|a, b| a.source.cmp(&b.source));
        out
    }

    fn spec(
        &self,
        source: &str,
        url: &str,
        bounds: Rect,
        book: &Book,
    ) -> Result<ViewSpec, SiteError> {
        let config = &self.inner.config;
        Ok(ViewSpec {
            source: source.to_owned(),
            url: url.to_owned(),
            bounds,
            data_dir: session::folder(&config.profile_dir, source)
                .map_err(|_| SiteError::Invalid("not a source id".into()))?,
            data_store: session::data_store_id(&config.keychain_service, source)
                .map_err(|_| SiteError::Invalid("not a source id".into()))?,
            background: book.background,
            zoom: book.zoom,
            devtools: config.devtools,
            confine: book.rules.get(source).and_then(SiteRules::confine),
        })
    }

    /// Send `source`'s status when it changed, and put its view on or off
    /// screen to match. On the main thread.
    fn sync(&self, source: &str) {
        let (status, visible, bounds_insets, focus) = {
            let mut book = self.book();
            let Some(site) = book.sites.get_mut(source) else {
                return;
            };
            let visible = site.visible();
            let focus = visible && std::mem::take(&mut site.focus_wanted);
            if focus {
                site.keyboard = SiteKeyboard::Site;
            }
            let status = site.status();
            let insets = book.insets.get(source).copied();
            if book.sent.get(source) == Some(&status) {
                (None, visible, insets, focus)
            } else {
                book.sent.insert(source.to_owned(), status.clone());
                (Some(status), visible, insets, focus)
            }
        };
        if let Some(view) = view(source) {
            if visible && let Some(insets) = bounds_insets {
                let (width, height) = self.shell().window_size();
                view.set_bounds(insets.rect(width, height));
            }
            view.set_visible(visible);
            if focus {
                view.focus();
            }
        }
        if let Some(status) = status {
            self.shell().emit(SiteEvent::Status(status));
        }
    }

    fn sync_later(&self, source: &str) {
        let this = self.clone();
        let source = source.to_owned();
        self.shell().later(Box::new(move || this.sync(&source)));
    }

    /// Ask `source`'s page whether it holds changes, with the timeout.
    async fn probe(&self, source: &str) -> Dirty {
        let (tx, rx) = tokio::sync::oneshot::channel::<Dirty>();
        let source = source.to_owned();
        self.shell().on_main(Box::new(move || match view(&source) {
            Some(view) => view.probe(Box::new(move |dirty| {
                let _ = tx.send(dirty);
            })),
            None => {
                let _ = tx.send(Dirty::Clean);
            }
        }));
        match tokio::time::timeout(PROBE_TIMEOUT, rx).await {
            Ok(Ok(dirty)) => dirty,
            _ => Dirty::Unknown,
        }
    }

    /// Make `source`'s view, or point it at `url`, through the question of
    /// decisions §10 unless `force`. With no `url`: the site as it is, else
    /// its last address, else the source's base.
    ///
    /// # Errors
    ///
    /// [`SiteError::NotOffered`] when the source's pages do not open inside
    /// knobas; [`SiteError::Invalid`] when `url` is not one the site may
    /// show.
    pub async fn open(
        &self,
        source: &str,
        url: Option<String>,
        insets: SiteInsets,
        force: bool,
    ) -> Result<SiteAsked, SiteError> {
        let (exists, target) = {
            let mut book = self.book();
            let rules = book
                .rules
                .get(source)
                .cloned()
                .ok_or(SiteError::NotOffered)?;
            if let Some(url) = &url
                && decide(&rules, url, true) != Decision::Allow
            {
                return Err(SiteError::Invalid(
                    "that address does not open in this site".into(),
                ));
            }
            book.insets.insert(source.to_owned(), insets.sane());
            let exists = book.sites.contains_key(source);
            let target = url.clone().or_else(|| {
                (!exists).then(|| {
                    book.last_url.get(source).cloned().unwrap_or_else(|| {
                        let base = &rules.base;
                        format!("{}{}/", base.origin, base.path)
                    })
                })
            });
            (exists, target)
        };
        if exists {
            let Some(target) = target else {
                return Ok(SiteAsked::DONE);
            };
            if !force && self.probe(source).await.asks() {
                return Ok(SiteAsked::UNSAVED);
            }
            let now = self.now();
            if let Some(site) = self.book().sites.get_mut(source) {
                site.land(&target, now);
            }
            let this = self.clone();
            let source = source.to_owned();
            self.shell().on_main(Box::new(move || {
                if let Some(view) = view(&source) {
                    if force {
                        view.leave_freely();
                    }
                    view.load(&target);
                }
                this.sync(&source);
            }));
            return Ok(SiteAsked::DONE);
        }
        let target = target.unwrap_or_default();
        let now = self.now();
        {
            let mut book = self.book();
            let mut site = Site::opening(source, &target, now);
            site.sign_in_saved = book.sign_ins.get(source).is_some_and(|rule| rule.saved);
            book.sites.insert(source.to_owned(), site);
            let platform = self.inner.config.platform;
            book.keyboards
                .insert(source.to_owned(), Keyboard::new(platform));
        }
        tracing::info!(source, host = %host_of(&target), "site made");
        let this = self.clone();
        let source_owned = source.to_owned();
        self.shell()
            .on_main(Box::new(move || this.make(&source_owned, &target)));
        self.make_room(source).await;
        Ok(SiteAsked::DONE)
    }

    /// Make the view. On the main thread.
    fn make(&self, source: &str, url: &str) {
        let Some(host) = host() else {
            self.failed_to_make(source, "no platform to make a site on");
            return;
        };
        let spec = {
            let book = self.book();
            let insets = book.insets.get(source).copied().unwrap_or_default();
            let (width, height) = self.shell().window_size();
            match self.spec(source, url, insets.rect(width, height), &book) {
                Ok(spec) => spec,
                Err(error) => {
                    drop(book);
                    self.failed_to_make(source, &error.to_string());
                    return;
                }
            }
        };
        match host.make(&spec, self.hooks(source)) {
            Ok(made) => {
                VIEWS.with(|views| views.borrow_mut().insert(source.to_owned(), Rc::from(made)));
                self.sync(source);
            }
            Err(error) => self.failed_to_make(source, &error),
        }
    }

    fn failed_to_make(&self, source: &str, why: &str) {
        tracing::warn!(source, error = why, "site not made");
        if let Some(site) = self.book().sites.get_mut(source) {
            site.fail(crate::state::SiteFailure::Other(why.to_owned()));
        }
        self.sync(source);
    }

    /// The hooks of `source`'s view.
    fn hooks(&self, source: &str) -> Hooks {
        let nav = self.clone();
        let nav_source = source.to_owned();
        let key = self.clone();
        let key_source = source.to_owned();
        let ev = self.clone();
        let ev_source = source.to_owned();
        let dl = self.clone();
        let dl_source = source.to_owned();
        let win = self.clone();
        let win_source = source.to_owned();
        Hooks {
            navigate: Rc::new(move |url| nav.navigate(&nav_source, url)),
            new_window: Rc::new(move |url| win.new_window(&win_source, url)),
            key: Rc::new(move |press| key.key(&key_source, press)),
            event: Rc::new(move |event| ev.event(&ev_source, event)),
            download: Rc::new(move |_url, suggested| dl.download(&dl_source, suggested)),
        }
    }

    /// A top-level navigation of `source`'s view: whether it loads.
    fn navigate(&self, source: &str, url: &str) -> bool {
        let decision = {
            let book = self.book();
            let (Some(rules), Some(site)) = (book.rules.get(source), book.sites.get(source)) else {
                // A site whose switch went off, or one being closed, shows
                // nothing new.
                return false;
            };
            decide(rules, url, site.landing)
        };
        let host = host_of(url);
        match decision {
            Decision::Allow => return true,
            Decision::Refuse(Refusal::Scheme(scheme)) => {
                let scheme = scheme.unwrap_or_default();
                tracing::info!(source, scheme = %scheme, "site navigation refused");
                self.shell().emit(SiteEvent::Routed(SiteRouted {
                    source: source.to_owned(),
                    to: SiteRoutedTo::Refused,
                    host: scheme,
                    url: String::new(),
                    other: None,
                }));
            }
            Decision::ToSite { source: other } => {
                tracing::info!(source, host = %host, other = %other, "site navigation routed to another site");
                self.shell().emit(SiteEvent::Routed(SiteRouted {
                    source: source.to_owned(),
                    to: SiteRoutedTo::Site,
                    host,
                    url: url.to_owned(),
                    other: Some(other),
                }));
            }
            Decision::Ask { origin } => {
                tracing::info!(source, host = %host, "site asks before another address");
                if let Some(site) = self.book().sites.get_mut(source) {
                    site.ask(origin, url);
                }
                self.sync_later(source);
            }
            Decision::Browser => {
                tracing::info!(source, host = %host, "site navigation routed to the browser");
                self.shell().emit(SiteEvent::Routed(SiteRouted {
                    source: source.to_owned(),
                    to: SiteRoutedTo::Browser,
                    host,
                    url: url.to_owned(),
                    other: None,
                }));
            }
        }
        false
    }

    /// A new-window request of `source`'s view: never a window. An address
    /// the site may show loads in the same view.
    pub fn new_window(&self, source: &str, url: &str) {
        if self.navigate(source, url) {
            let url = url.to_owned();
            let source = source.to_owned();
            self.shell().later(Box::new(move || {
                if let Some(view) = view(&source) {
                    view.load(&url);
                }
            }));
        }
    }

    /// A key going down in `source`'s view.
    fn key(&self, source: &str, press: &Press) -> Claim {
        let now = self.now();
        let claim = {
            let mut book = self.book();
            let leave = book.sites.get(source).is_some_and(|site| site.esc_target);
            let Some(keyboard) = book.keyboards.get_mut(source) else {
                return Claim {
                    pass: true,
                    name: None,
                };
            };
            let claim = keyboard.press(press, now, leave);
            if let Some(site) = book.sites.get_mut(source) {
                site.keyboard = SiteKeyboard::Site;
            }
            claim
        };
        if let Some(name) = claim.name {
            self.claimed(source, name);
        }
        claim
    }

    fn claimed(&self, source: &str, name: KeyName) {
        if name == KeyName::Quit {
            let shell = Arc::clone(self.shell());
            self.shell().later(Box::new(move || shell.quit()));
            return;
        }
        self.shell().emit(SiteEvent::Key(SiteKey {
            source: source.to_owned(),
            key: name.as_str().to_owned(),
        }));
    }

    /// A key knobas's own page would get, read natively on the Mac before
    /// the menu sees it (decisions §21): ⌘Q while a site is alive, ⌘W while
    /// a site's page is shown. Whether it was taken.
    pub fn shell_key(&self, press: &Press) -> bool {
        let shown = {
            let book = self.book();
            if book.sites.is_empty() {
                return false;
            }
            book.sites
                .values()
                .find(|site| site.wanted)
                .map(|site| site.source.clone())
        };
        let platform = self.inner.config.platform;
        let mut keys = Keyboard::new(platform);
        let claim = keys.press(press, 0.0, false);
        match claim.name {
            Some(KeyName::Quit) => {
                let shell = Arc::clone(self.shell());
                self.shell().later(Box::new(move || shell.quit()));
                true
            }
            Some(KeyName::Close) if shown.is_some() => {
                self.claimed(&shown.unwrap_or_default(), KeyName::Close);
                true
            }
            _ => false,
        }
    }

    /// What `source`'s view reported.
    fn event(&self, source: &str, event: ViewEvent) {
        let mut ended = false;
        let mut landed = false;
        let mut fill = None;
        let now = self.now();
        {
            let mut book = self.book();
            let rules = book.rules.get(source).cloned();
            let sign_in = book.sign_ins.get(source).cloned().unwrap_or_default();
            let Some(site) = book.sites.get_mut(source) else {
                return;
            };
            match event {
                ViewEvent::Committed(url) => {
                    if let Some(rules) = &rules {
                        site.commit(&url, rules);
                    }
                    site.loading = true;
                    site.filler.moved_on();
                    if site.own && !site.landed_own {
                        site.landed_own = true;
                        landed = true;
                    }
                }
                ViewEvent::Finished(url) => {
                    site.loading = false;
                    if let Some(rules) = &rules {
                        site.moved(&url, rules);
                        if site.own && site.phase == SitePhase::Live {
                            let page = is_sign_in_page(&rules.base, &sign_in, &url);
                            fill = site.filler.loaded(page, site.sign_in_saved, now);
                        }
                    }
                }
                ViewEvent::Title(raw) => site.titled(&raw),
                ViewEvent::History {
                    url,
                    can_back,
                    can_forward,
                    loading,
                } => {
                    site.can_back = can_back;
                    site.can_forward = can_forward;
                    if let Some(loading) = loading {
                        site.loading = loading;
                    }
                    if let (Some(url), Some(rules)) = (url, &rules) {
                        site.moved(&url, rules);
                    }
                }
                ViewEvent::Failed(failure) => {
                    // A page that shows keeps showing: a failed sub-load is
                    // the page's business.
                    if site.phase == SitePhase::Opening || !site.committed {
                        site.fail(failure);
                    } else {
                        site.loading = false;
                    }
                }
                ViewEvent::Ended => {
                    site.stop();
                    ended = true;
                }
                ViewEvent::Downloaded { id, ok } => {
                    let name = u32::try_from(id)
                        .ok()
                        .and_then(|id| book.downloads.get(&id).map(|(_, path)| (id, path.clone())));
                    drop(book);
                    if let Some((id, path)) = name {
                        self.shell().emit(SiteEvent::Download(SiteDownload {
                            source: source.to_owned(),
                            id,
                            name: file_name(&path),
                            state: if ok {
                                SiteDownloadState::Saved
                            } else {
                                SiteDownloadState::Failed
                            },
                        }));
                    }
                    return;
                }
            }
        }
        if ended {
            tracing::warn!(source, "site's process ended");
        }
        if landed {
            self.shell().landed(source);
        }
        if let Some(act) = fill {
            self.fill_act(source, act);
        }
        self.sync_later(source);
    }

    /// Do what the one-try rule asked: fetch the saved sign-in for a fill
    /// (through the shell, later), or look for the form now.
    fn fill_act(&self, source: &str, act: FillAct) {
        match act {
            FillAct::Fill { auto } => {
                tracing::info!(source, auto, "site sign-in asked for");
                let shell = Arc::clone(self.shell());
                let source = source.to_owned();
                self.shell().later(Box::new(move || shell.sign_in(&source)));
            }
            FillAct::Look => {
                let this = self.clone();
                let source = source.to_owned();
                self.shell()
                    .later(Box::new(move || this.run_fill(&source, SignInCall::look())));
            }
        }
    }

    /// *Fill sign-in*: the saved sign-in into the page now.
    ///
    /// # Errors
    ///
    /// [`SiteError::NotOpen`]; [`SiteError::Invalid`] when no sign-in is
    /// saved or the page is not on the source's own origin.
    pub fn press_fill(&self, source: &str) -> Result<(), SiteError> {
        let now = self.now();
        let act = {
            let mut book = self.book();
            let site = book.sites.get_mut(source).ok_or(SiteError::NotOpen)?;
            if !site.sign_in_saved {
                return Err(SiteError::Invalid(
                    "no sign-in is saved for this site".into(),
                ));
            }
            if !site.own {
                return Err(SiteError::Invalid(
                    "the saved sign-in goes only into the source's own page".into(),
                ));
            }
            site.filler.press(now)
        };
        self.fill_act(source, act);
        self.sync_later(source);
        Ok(())
    }

    /// The saved sign-in the shell read for `source`: typed into the page
    /// if, at this moment, the view's own address is on the source's own
    /// origin. Neither value is kept or logged.
    pub fn fill_with(&self, source: &str, username: String, password: String) {
        let this = self.clone();
        let source = source.to_owned();
        self.shell().on_main(Box::new(move || {
            this.run_fill(
                &source,
                SignInCall {
                    username,
                    password,
                    mode: FillMode::Fill,
                },
            );
        }));
    }

    /// The shell found no saved sign-in for `source`.
    pub fn fill_none(&self, source: &str) {
        {
            let mut book = self.book();
            if let Some(rule) = book.sign_ins.get_mut(source) {
                rule.saved = false;
            }
            if let Some(site) = book.sites.get_mut(source) {
                site.sign_in_saved = false;
                site.filler.refused();
            }
        }
        tracing::info!(source, "site sign-in: none saved");
        self.sync_later(source);
    }

    /// Run the fill script in `source`'s view, after the native origin
    /// check. On the main thread.
    fn run_fill(&self, source: &str, call: SignInCall) {
        let mode = call.mode;
        let Some(view) = view(source) else {
            if let Some(site) = self.book().sites.get_mut(source) {
                site.filler.refused();
            }
            return;
        };
        let live = view.url();
        let checked = {
            let book = self.book();
            match (book.rules.get(source), book.sites.get(source)) {
                (Some(rules), Some(site)) => may_fill(
                    rules,
                    site.sign_in_saved || mode == FillMode::Look,
                    site.url.as_deref(),
                    live.as_deref(),
                ),
                _ => return,
            }
        };
        if let Err(why) = checked {
            tracing::info!(source, why = ?why, mode = ?mode, "site sign-in refused");
            if let Some(site) = self.book().sites.get_mut(source) {
                match mode {
                    FillMode::Fill => site.filler.refused(),
                    FillMode::Look => site.filler.judged(FillAnswer::Odd, false),
                }
            }
            self.sync_later(source);
            return;
        }
        let this = self.clone();
        let owned = source.to_owned();
        view.sign_in(
            &call,
            Box::new(move |text| this.filled(&owned, mode, &text)),
        );
    }

    /// The fill script answered.
    fn filled(&self, source: &str, mode: FillMode, text: &str) {
        let answer = FillAnswer::from_answer(text);
        let now = self.now();
        let more = {
            let mut book = self.book();
            let rule = book.sign_ins.get(source).cloned().unwrap_or_default();
            let base = book.rules.get(source).map(|rules| rules.base.clone());
            let Some(site) = book.sites.get_mut(source) else {
                return;
            };
            match mode {
                FillMode::Fill => site.filler.answered(answer, now),
                FillMode::Look => {
                    let on_sign_in_page = match (&base, &site.url) {
                        (Some(base), Some(url)) => is_sign_in_page(base, &rule, url),
                        _ => false,
                    };
                    site.filler.judged(answer, on_sign_in_page);
                }
            }
            site.filler.tick(now)
        };
        tracing::info!(source, mode = ?mode, answer = ?answer, "site sign-in answered");
        if let Some(act) = more {
            self.fill_act(source, act);
        }
        self.sync_later(source);
    }

    /// A download starts in `source`'s view.
    fn download(&self, source: &str, suggested: &str) -> Option<(PathBuf, u64)> {
        let dir = self.inner.config.downloads_dir.clone()?;
        let name = download_name(suggested, |candidate| dir.join(candidate).exists());
        let path = dir.join(&name);
        let id = {
            let mut book = self.book();
            book.next_download += 1;
            let id = book.next_download;
            book.downloads.insert(id, (source.to_owned(), path.clone()));
            id
        };
        tracing::info!(source, "site download started");
        self.shell().emit(SiteEvent::Download(SiteDownload {
            source: source.to_owned(),
            id,
            name,
            state: SiteDownloadState::Started,
        }));
        Some((path, u64::from(id)))
    }

    /// The saved file of a download knobas recorded.
    ///
    /// # Errors
    ///
    /// [`SiteError::Invalid`] for an id knobas never issued.
    pub fn download_path(&self, id: u32) -> Result<PathBuf, SiteError> {
        self.book()
            .downloads
            .get(&id)
            .map(|(_, path)| path.clone())
            .ok_or_else(|| SiteError::Invalid("no such download".into()))
    }

    /// Show `source`'s view at `insets`, when it has a page. `focus`: give
    /// it the keyboard. `esc`: Esc has somewhere to go.
    pub fn show(&self, source: &str, insets: SiteInsets, focus: bool, esc: bool) {
        let now = self.now();
        {
            let mut book = self.book();
            book.insets.insert(source.to_owned(), insets.sane());
            let Some(site) = book.sites.get_mut(source) else {
                return;
            };
            site.want(true, now);
            site.esc_target = esc;
            let wake = site.asleep;
            site.asleep = false;
            if focus {
                site.focus_wanted = true;
            }
            if let Some(keys) = book.keyboards.get_mut(source) {
                keys.disarm();
            }
            tracing::debug!(source, wake, "site shown");
        }
        let this = self.clone();
        let source = source.to_owned();
        self.shell().on_main(Box::new(move || {
            if let Some(view) = view(&source) {
                view.sleep(false);
            }
            // Focus, when asked, goes with the first moment the view is on
            // screen: now, or at its first commit.
            this.sync(&source);
        }));
    }

    /// Hide `source`'s view. Its page keeps running.
    pub fn hide(&self, source: &str) {
        let now = self.now();
        let had_keyboard = {
            let mut book = self.book();
            let Some(site) = book.sites.get_mut(source) else {
                return;
            };
            site.want(false, now);
            site.focus_wanted = false;
            let had = site.keyboard == SiteKeyboard::Site;
            site.keyboard = SiteKeyboard::Shell;
            if let Some(keys) = book.keyboards.get_mut(source) {
                keys.disarm();
            }
            had
        };
        let this = self.clone();
        let source = source.to_owned();
        self.shell().on_main(Box::new(move || {
            this.sync(&source);
            if had_keyboard && let Some(host) = host() {
                host.focus_shell();
            }
        }));
    }

    /// The window's size changed: every shown view to its rectangle. On the
    /// main thread.
    pub fn relayout(&self) {
        let shown: Vec<(String, SiteInsets)> = {
            let book = self.book();
            book.sites
                .values()
                .filter(|site| site.visible())
                .filter_map(|site| {
                    book.insets
                        .get(&site.source)
                        .map(|insets| (site.source.clone(), *insets))
                })
                .collect()
        };
        if shown.is_empty() {
            return;
        }
        let (width, height) = self.shell().window_size();
        for (source, insets) in shown {
            if let Some(view) = view(&source) {
                view.set_bounds(insets.rect(width, height));
            }
        }
    }

    /// The main window gained or lost the keyboard. Regained, the shown site
    /// takes it back if it had it (decisions §8: wry gives it to the main web
    /// view). On the main thread.
    pub fn window_focused(&self, focused: bool) {
        if focused {
            let back = self.book().keyboard_on_blur.take();
            if let Some(source) = back {
                let visible = self.book().sites.get(&source).is_some_and(Site::visible);
                if visible && let Some(view) = view(&source) {
                    view.focus();
                }
            }
        } else {
            let mut book = self.book();
            book.keyboard_on_blur = book
                .sites
                .values()
                .find(|site| site.visible() && site.keyboard == SiteKeyboard::Site)
                .map(|site| site.source.clone());
        }
    }

    /// Back, forward, reload, stop, or the keyboard to the site or to
    /// knobas. Reload asks first, unless `force`.
    ///
    /// # Errors
    ///
    /// [`SiteError::NotOpen`] when the site has no view.
    pub async fn act(
        &self,
        source: &str,
        act: SiteAct,
        force: bool,
    ) -> Result<SiteAsked, SiteError> {
        let landing = {
            let book = self.book();
            let site = book.sites.get(source).ok_or(SiteError::NotOpen)?;
            site.url.clone().unwrap_or_else(|| site.landing_url.clone())
        };
        if act == SiteAct::Reload && !force {
            let needs_probe = self
                .book()
                .sites
                .get(source)
                .is_some_and(|site| site.committed);
            if needs_probe && self.probe(source).await.asks() {
                return Ok(SiteAsked::UNSAVED);
            }
        }
        let now = self.now();
        {
            let mut book = self.book();
            let Some(site) = book.sites.get_mut(source) else {
                return Err(SiteError::NotOpen);
            };
            match act {
                SiteAct::Reload => site.land(&landing, now),
                SiteAct::Focus => site.keyboard = SiteKeyboard::Site,
                SiteAct::FocusShell => site.keyboard = SiteKeyboard::Shell,
                SiteAct::Back | SiteAct::Forward | SiteAct::Stop => {}
            }
        }
        let this = self.clone();
        let source = source.to_owned();
        self.shell().on_main(Box::new(move || {
            let committed = this
                .book()
                .sites
                .get(&source)
                .is_some_and(|site| site.committed);
            if let Some(view) = view(&source) {
                match act {
                    SiteAct::Back => view.back(),
                    SiteAct::Forward => view.forward(),
                    // A view that never committed reloads by loading its
                    // landing address again.
                    SiteAct::Reload if committed => {
                        if force {
                            view.leave_freely();
                        }
                        view.reload();
                    }
                    SiteAct::Reload => view.load(&landing),
                    SiteAct::Stop => view.stop(),
                    SiteAct::Focus => view.focus(),
                    SiteAct::FocusShell => {
                        if let Some(host) = host() {
                            host.focus_shell();
                        }
                    }
                }
            } else if act == SiteAct::Reload {
                // A site with no view (its making failed, or a remake did,
                // ux30): *Retry* makes it again.
                this.make(&source, &landing);
            }
            this.sync(&source);
        }));
        Ok(SiteAsked::DONE)
    }

    /// The answer to the question of decisions §6 line 4. `allow`: the
    /// origin joins the site's allowed origins and the landing starts again;
    /// the caller stores it. Otherwise the stopped address opens in the
    /// browser, and the site goes back to its page or closes.
    ///
    /// # Errors
    ///
    /// [`SiteError::NotOpen`], or [`SiteError::Invalid`] when the site asks
    /// about no such origin.
    pub fn answer(
        &self,
        source: &str,
        origin: &str,
        allow: bool,
    ) -> Result<Option<String>, SiteError> {
        let origin = Origin::parse(origin).map_err(|e| SiteError::Invalid(e.to_string()))?;
        let now = self.now();
        let (reload, browser, close, confine) = {
            let mut book = self.book();
            let asked = book
                .sites
                .get(source)
                .ok_or(SiteError::NotOpen)?
                .asks
                .clone()
                .filter(|(asked, _)| *asked == origin)
                .ok_or_else(|| SiteError::Invalid("the site asks about another address".into()))?;
            if allow {
                // A confined site's refusal takes the new origin in before
                // the landing loads again (ux30).
                let mut confine = None;
                if let Some(rules) = book.rules.get_mut(source)
                    && !rules.allowed.contains(&origin)
                {
                    rules.allowed.push(origin.clone());
                    confine = rules.confine().map(Some);
                }
                let site = book.sites.get_mut(source).ok_or(SiteError::NotOpen)?;
                let landing = site.landing_url.clone();
                site.land(&landing, now);
                (Some(landing), None, false, confine)
            } else {
                let site = book.sites.get_mut(source).ok_or(SiteError::NotOpen)?;
                let close = site.declined();
                (None, Some(asked.1), close, None)
            }
        };
        tracing::info!(source, host = %origin.host, allow, "site question answered");
        if close {
            let this = self.clone();
            let source = source.to_owned();
            self.shell().on_main(Box::new(move || {
                this.drop_view(&source, SiteClosedWhy::Declined)
            }));
        } else {
            let this = self.clone();
            let source = source.to_owned();
            self.shell().on_main(Box::new(move || {
                match confine {
                    Some(confine) => this.reconfine(&source, confine, reload),
                    None => {
                        if let (Some(view), Some(url)) = (view(&source), &reload) {
                            view.load(url);
                        }
                    }
                }
                this.sync(&source);
            }));
        }
        Ok(browser)
    }

    /// Drop `source`'s view and state. On the main thread.
    fn drop_view(&self, source: &str, why: SiteClosedWhy) {
        let removed = VIEWS.with(|views| views.borrow_mut().remove(source));
        drop(removed);
        {
            let mut book = self.book();
            if let Some(site) = book.sites.remove(source) {
                if why == SiteClosedWhy::Forgot {
                    book.last_url.remove(source);
                } else if let Some(url) = site.url.or(Some(site.landing_url)) {
                    book.last_url.insert(source.to_owned(), url);
                }
            }
            book.keyboards.remove(source);
            book.sent.remove(source);
        }
        tracing::info!(source, why = ?why, "site closed");
        self.shell().emit(SiteEvent::Closed(SiteClosed {
            source: source.to_owned(),
            why,
        }));
    }

    async fn close_when(
        &self,
        source: &str,
        force: bool,
        why: SiteClosedWhy,
    ) -> Result<SiteAsked, SiteError> {
        let committed = {
            let book = self.book();
            let site = book.sites.get(source).ok_or(SiteError::NotOpen)?;
            site.committed
        };
        if !force && committed && self.probe(source).await.asks() {
            return Ok(SiteAsked::UNSAVED);
        }
        let had_keyboard = self
            .book()
            .sites
            .get(source)
            .is_some_and(|site| site.keyboard == SiteKeyboard::Site);
        let this = self.clone();
        let source = source.to_owned();
        self.shell().on_main(Box::new(move || {
            this.drop_view(&source, why);
            if had_keyboard && let Some(host) = host() {
                host.focus_shell();
            }
        }));
        Ok(SiteAsked::DONE)
    }

    /// Close `source`'s view, asking the page first unless `force`.
    ///
    /// # Errors
    ///
    /// [`SiteError::NotOpen`].
    pub async fn close(&self, source: &str, force: bool) -> Result<SiteAsked, SiteError> {
        self.close_when(source, force, SiteClosedWhy::Asked).await
    }

    /// Close `source`'s view, asking first unless `force`, and remove its
    /// session (decisions §5). A source with no view is forgotten at once.
    ///
    /// # Errors
    ///
    /// [`SiteError::Invalid`] for an id that is not a source id.
    pub async fn forget(&self, source: &str, force: bool) -> Result<SiteAsked, SiteError> {
        session::folder(&self.inner.config.profile_dir, source)
            .map_err(|_| SiteError::Invalid("not a source id".into()))?;
        let open = self.book().sites.contains_key(source);
        if open {
            let asked = self
                .close_when(source, force, SiteClosedWhy::Forgot)
                .await?;
            if !asked.done {
                return Ok(asked);
            }
        }
        self.book().last_url.remove(source);
        let spec = {
            let book = self.book();
            self.spec(source, "about:blank", Rect::default(), &book)?
        };
        // The platform may hold the store for a moment after its last view
        // closed (WebKit: "currently opened"; WebView2: its browser process
        // still runs): try again for a few seconds before listing it.
        let mut gone = false;
        for attempt in 0..FORGET_TRIES {
            if attempt > 0 {
                tokio::time::sleep(FORGET_AGAIN).await;
            }
            let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
            let spec = spec.clone();
            self.shell().on_main(Box::new(move || match host() {
                Some(host) => host.forget(
                    &spec,
                    Box::new(move |done| {
                        let _ = tx.send(done);
                    }),
                ),
                None => {
                    let _ = tx.send(true);
                }
            }));
            gone = matches!(
                tokio::time::timeout(Duration::from_secs(5), rx).await,
                Ok(Ok(true))
            );
            if gone {
                break;
            }
        }
        if !gone {
            tracing::warn!(
                source,
                "a site's session is still held; removed at the next start"
            );
            if let Err(error) = session::remember_to_forget(&self.inner.config.profile_dir, source)
            {
                tracing::warn!(%error, "a site's session could not be listed to forget");
            }
        }
        Ok(SiteAsked::DONE)
    }

    /// Probe every live site at once and answer the ones that object (the
    /// quit dialog).
    pub async fn unsaved(&self) -> Vec<SiteStatus> {
        let sources: Vec<String> = self
            .book()
            .sites
            .values()
            .filter(|site| site.committed)
            .map(|site| site.source.clone())
            .collect();
        let answers =
            futures_util::future::join_all(sources.iter().map(|source| self.probe(source))).await;
        let book = self.book();
        sources
            .iter()
            .zip(answers)
            .filter(|(_, dirty)| dirty.asks())
            .filter_map(|(source, _)| book.sites.get(source).map(Site::status))
            .collect()
    }

    /// Close sites until no more than *Sites kept open* are alive, the one
    /// shown longest ago first, never `keep` and never one that objects.
    async fn make_room(&self, keep: &str) {
        let (needed, candidates) = {
            let book = self.book();
            let live: Vec<Liveness<'_>> = book
                .sites
                .values()
                .map(|site| Liveness {
                    source: &site.source,
                    last_shown: site.last_shown,
                    wanted: site.wanted || site.source == keep,
                })
                .collect();
            let (needed, candidates) = room(&live, book.kept);
            (
                needed,
                candidates
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            )
        };
        if needed == 0 {
            return;
        }
        let mut answers = HashMap::new();
        for source in &candidates {
            answers.insert(source.clone(), self.probe(source).await);
        }
        let refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
        for source in choose(&refs, needed, |source| {
            answers.get(source) == Some(&Dirty::Clean)
        }) {
            let this = self.clone();
            let source = source.to_owned();
            self.shell().on_main(Box::new(move || {
                this.drop_view(&source, SiteClosedWhy::Room)
            }));
        }
    }

    /// The clock: a landing that timed out fails, a hidden site sleeps, one
    /// hidden for two hours with nothing unsaved closes, and on the Mac the
    /// shown site's address is read (a `pushState` raises no event).
    pub async fn tick(&self) {
        let now = self.now();
        let mut fills: Vec<(String, FillAct)> = Vec::new();
        let (changed, asleep, idle_ones, poll): (
            Vec<String>,
            Vec<String>,
            Vec<String>,
            Vec<String>,
        ) = {
            let mut book = self.book();
            let mut changed = Vec::new();
            for site in book.sites.values_mut() {
                if site.tick(now) {
                    changed.push(site.source.clone());
                }
                if let Some(act) = site.filler.tick(now) {
                    fills.push((site.source.clone(), act));
                }
            }
            let asleep: Vec<String> = sleepers(book.sites.values(), now)
                .into_iter()
                .map(str::to_owned)
                .collect();
            for source in &asleep {
                if let Some(site) = book.sites.get_mut(source) {
                    site.asleep = true;
                }
            }
            let idle_ones = idle(book.sites.values(), now)
                .into_iter()
                .map(str::to_owned)
                .collect();
            let poll = book
                .sites
                .values()
                .filter(|site| site.visible())
                .map(|site| site.source.clone())
                .collect();
            (changed, asleep, idle_ones, poll)
        };
        for (source, act) in fills {
            self.fill_act(&source, act);
        }
        let this = self.clone();
        self.shell().on_main(Box::new(move || {
            for source in &asleep {
                if let Some(view) = view(source) {
                    view.sleep(true);
                }
            }
            for source in &poll {
                if let Some(event) = view(source).and_then(|view| view.poll()) {
                    this.event(source, event);
                }
            }
            for source in &changed {
                this.sync(source);
            }
        }));
        for source in idle_ones {
            if self.probe(&source).await == Dirty::Clean {
                let this = self.clone();
                self.shell().on_main(Box::new(move || {
                    this.drop_view(&source, SiteClosedWhy::Idle)
                }));
            }
        }
    }

    /// The window's zoom level, on every live view and each new one.
    pub fn zoom(&self, level: f64) {
        let level = if level.is_finite() {
            level.clamp(0.25, 5.0)
        } else {
            1.0
        };
        let sources: Vec<String> = {
            let mut book = self.book();
            book.zoom = level;
            book.sites.keys().cloned().collect()
        };
        self.shell().on_main(Box::new(move || {
            for source in sources {
                if let Some(view) = view(&source) {
                    view.zoom(level);
                }
            }
        }));
    }

    /// The main window moved. On the main thread.
    pub fn window_moved(&self) {
        let sources: Vec<String> = self.book().sites.keys().cloned().collect();
        for source in sources {
            if let Some(view) = view(&source) {
                view.window_moved();
            }
        }
    }

    /// Drop every view, then wait at most [`QUIT_FLUSH`] for the sites'
    /// sessions to be written, so a sign-in made a moment before quitting
    /// is there at the next start (ux29). On the main thread, before the
    /// database stops.
    pub fn shutdown(&self) {
        let views = VIEWS.with(|views| std::mem::take(&mut *views.borrow_mut()));
        let alive = !views.is_empty();
        let processes: Vec<u32> = views
            .values()
            .filter_map(|view| view.browser_process())
            .collect();
        drop(views);
        self.book().sites.clear();
        if alive && let Some(host) = host() {
            let started = std::time::Instant::now();
            host.flush(&processes, started + QUIT_FLUSH);
            tracing::info!(
                ms = started.elapsed().as_secs_f64() * 1000.0,
                "site sessions written"
            );
        }
    }
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default()
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
