//! One site's state, and how long sites live (decisions §14). Pure: the
//! clock is handed in, in milliseconds on any clock that only goes forward.

use serde::Serialize;
use ts_rs::TS;

use crate::keys::ESC_AGAIN_MS;
use crate::policy::{Origin, SiteRules, clean_title};

/// A landing with no page committed after this long has failed.
pub const LANDING_TIMEOUT_MS: f64 = 10_000.0;
/// A site hidden this long with nothing unsaved is closed.
pub const IDLE_CLOSE_MS: f64 = 2.0 * 60.0 * 60.0 * 1000.0;
/// A site hidden this long sleeps (Windows: `TrySuspend`).
pub const SLEEP_AFTER_MS: f64 = 10.0 * 60.0 * 1000.0;
/// *Sites kept open* until set, and its bounds.
pub const KEPT_DEFAULT: u32 = 2;
pub const KEPT_MIN: u32 = 1;
pub const KEPT_MAX: u32 = 6;

/// Where a site is, as its rectangle draws it. A closed site has no state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "site.ts")]
pub enum SitePhase {
    /// A landing, nothing committed yet: the view is hidden.
    Opening,
    /// A redirect during the landing went to an origin nobody allowed: the
    /// rectangle asks whether it may continue there.
    Asks,
    /// A page committed: the view shows it.
    Live,
    /// Nothing loaded: a timeout, a certificate refused, no such host.
    Failed,
    /// The site's process ended.
    Stopped,
}

/// Why nothing loaded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
#[ts(export_to = "site.ts")]
pub enum SiteFailure {
    /// No page committed in [`LANDING_TIMEOUT_MS`].
    Timeout,
    /// The server's certificate is not trusted on this computer. Never
    /// bypassed.
    Certificate,
    /// No such host.
    NotFound,
    /// The connection was refused or reset.
    Refused,
    /// The platform's own short reason.
    Other(String),
}

/// Which side holds the keyboard for a site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "site.ts")]
pub enum SiteKeyboard {
    Site,
    Shell,
}

/// One live site, as the page draws it (`site:status`). Every field is text
/// or a flag; nothing is HTML.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[ts(export_to = "site.ts")]
pub struct SiteStatus {
    pub source: String,
    pub phase: SitePhase,
    /// The committed top-level address.
    pub url: Option<String>,
    /// The page's title, cleaned, at most 200 characters.
    pub title: String,
    /// The address is on the source's own origin.
    pub own: bool,
    pub loading: bool,
    pub can_back: bool,
    pub can_forward: bool,
    /// The view is on screen.
    pub shown: bool,
    pub keyboard: SiteKeyboard,
    /// The origin the rectangle asks about, while [`SitePhase::Asks`].
    pub asks_origin: Option<String>,
    pub failure: Option<SiteFailure>,
    pub esc_again_ms: u32,
    /// *Fill sign-in* is drawn: a web sign-in is saved, the page is on the
    /// source's own origin, and no fill is under way (ux29).
    pub fill: bool,
    /// What the bar says about the saved sign-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sign_in: Option<crate::sign_in::SiteSignIn>,
}

/// One site's state.
#[derive(Clone, Debug)]
pub struct Site {
    pub source: String,
    pub phase: SitePhase,
    /// From a load knobas started until a page on `own(S)` commits.
    pub landing: bool,
    /// The address the landing started at, loaded again when an origin is
    /// allowed (decisions §6).
    pub landing_url: String,
    pub url: Option<String>,
    pub title: String,
    pub own: bool,
    pub loading: bool,
    pub can_back: bool,
    pub can_forward: bool,
    /// The page asked for the view on screen (`site_show`), and has not
    /// hidden it since.
    pub wanted: bool,
    /// Esc has somewhere to go: the site was opened from a knobas page.
    pub esc_target: bool,
    pub keyboard: SiteKeyboard,
    /// While [`SitePhase::Asks`]: the origin, and the address that was
    /// stopped.
    pub asks: Option<(Origin, String)>,
    pub failure: Option<SiteFailure>,
    /// A page committed since the view was made.
    pub committed: bool,
    /// When the current landing started.
    pub landing_at: f64,
    /// When the view was last on screen, or made.
    pub last_shown: f64,
    /// When the view went off screen; `None` while it is on.
    pub hidden_since: Option<f64>,
    /// The view sleeps.
    pub asleep: bool,
    /// A page on `own(S)` committed in this view.
    pub landed_own: bool,
    /// The page asked for the keyboard in the site while nothing was on
    /// screen yet: it takes it when the view first shows.
    pub focus_wanted: bool,
    /// A web sign-in is saved for the source (ux29).
    pub sign_in_saved: bool,
    /// The saved sign-in's one-try rule.
    pub filler: crate::sign_in::Filler,
}

impl Site {
    /// A site whose view is being made at `url`.
    #[must_use]
    pub fn opening(source: &str, url: &str, now: f64) -> Self {
        Self {
            source: source.to_owned(),
            phase: SitePhase::Opening,
            landing: true,
            landing_url: url.to_owned(),
            url: None,
            title: String::new(),
            own: false,
            loading: true,
            can_back: false,
            can_forward: false,
            wanted: false,
            esc_target: false,
            keyboard: SiteKeyboard::Shell,
            asks: None,
            failure: None,
            committed: false,
            landing_at: now,
            last_shown: now,
            hidden_since: Some(now),
            asleep: false,
            landed_own: false,
            focus_wanted: false,
            sign_in_saved: false,
            filler: crate::sign_in::Filler::default(),
        }
    }

    /// Whether the native view is on screen: the page wants it, and a page
    /// has committed and is not stopped behind a question or a failure.
    #[must_use]
    pub fn visible(&self) -> bool {
        self.wanted && self.committed && self.phase == SitePhase::Live
    }

    /// knobas starts a load at `url`: a reload, a new address, the landing
    /// again. A view that shows a page keeps showing it until the next one
    /// commits.
    pub fn land(&mut self, url: &str, now: f64) {
        self.landing = true;
        self.landing_url = url.to_owned();
        self.landing_at = now;
        self.loading = true;
        self.asks = None;
        self.failure = None;
        self.phase = if self.committed && matches!(self.phase, SitePhase::Live) {
            SitePhase::Live
        } else {
            SitePhase::Opening
        };
    }

    /// A top-level page committed at `url`. A page on `own(S)` ends the
    /// landing; one on an allowed address that is not the source's own does
    /// not (decisions §14 tests).
    pub fn commit(&mut self, url: &str, rules: &SiteRules) {
        let own = url::Url::parse(url)
            .ok()
            .and_then(|parsed| Origin::of(&parsed))
            .is_some_and(|origin| origin == *rules.own());
        self.url = Some(url.to_owned());
        self.own = own;
        if own {
            self.landing = false;
        }
        self.committed = true;
        self.asks = None;
        self.failure = None;
        if matches!(
            self.phase,
            SitePhase::Opening | SitePhase::Failed | SitePhase::Stopped | SitePhase::Asks
        ) {
            self.phase = SitePhase::Live;
        }
    }

    /// The top-level address changed without a load (`pushState`), or the
    /// history moved.
    pub fn moved(&mut self, url: &str, rules: &SiteRules) {
        if self.committed {
            self.own = url::Url::parse(url)
                .ok()
                .and_then(|parsed| Origin::of(&parsed))
                .is_some_and(|origin| origin == *rules.own());
            self.url = Some(url.to_owned());
        }
    }

    /// The page's title changed.
    pub fn titled(&mut self, raw: &str) {
        self.title = clean_title(raw);
    }

    /// A redirect of the landing went to `origin`, which nobody allowed: the
    /// navigation to `url` was stopped and the rectangle asks.
    pub fn ask(&mut self, origin: Origin, url: &str) {
        self.phase = SitePhase::Asks;
        self.asks = Some((origin, url.to_owned()));
        self.loading = false;
    }

    /// The person answered the question with *Open in browser*: the site
    /// goes back to the page it showed, or, with none, it had nothing to
    /// show. `true` when the view should close.
    pub fn declined(&mut self) -> bool {
        self.asks = None;
        self.landing = false;
        if self.committed {
            self.phase = SitePhase::Live;
            false
        } else {
            true
        }
    }

    /// Nothing loaded.
    pub fn fail(&mut self, failure: SiteFailure) {
        self.phase = SitePhase::Failed;
        self.failure = Some(failure);
        self.loading = false;
    }

    /// The site's process ended.
    pub fn stop(&mut self) {
        self.phase = SitePhase::Stopped;
        self.loading = false;
        self.committed = false;
    }

    /// The clock moved to `now`: a landing that committed nothing for
    /// [`LANDING_TIMEOUT_MS`] failed. `true` when this changed the phase.
    pub fn tick(&mut self, now: f64) -> bool {
        if self.phase == SitePhase::Opening && now - self.landing_at >= LANDING_TIMEOUT_MS {
            self.fail(SiteFailure::Timeout);
            return true;
        }
        false
    }

    /// The page wants the view on screen or off it, at `now`.
    pub fn want(&mut self, on: bool, now: f64) {
        self.wanted = on;
        if on {
            self.last_shown = now;
            self.hidden_since = None;
        } else if self.hidden_since.is_none() {
            self.last_shown = now;
            self.hidden_since = Some(now);
        }
    }

    /// The status the page draws.
    #[must_use]
    pub fn status(&self) -> SiteStatus {
        SiteStatus {
            source: self.source.clone(),
            phase: self.phase,
            url: self.url.clone(),
            title: self.title.clone(),
            own: self.own,
            loading: self.loading,
            can_back: self.can_back,
            can_forward: self.can_forward,
            shown: self.visible(),
            keyboard: self.keyboard,
            asks_origin: self.asks.as_ref().map(|(origin, _)| origin.to_string()),
            failure: self.failure.clone(),
            esc_again_ms: ESC_AGAIN_MS,
            fill: self.sign_in_saved
                && self.own
                && self.committed
                && self.phase == SitePhase::Live
                && !self.filler.busy(),
            sign_in: self.filler.said.or_else(|| {
                (self.sign_in_saved && self.committed && !self.own && self.url.is_some())
                    .then_some(crate::sign_in::SiteSignIn::Provider)
            }),
        }
    }
}

/// One live site, as the bound reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Liveness<'a> {
    pub source: &'a str,
    /// When it was last on screen.
    pub last_shown: f64,
    /// It is on screen now, or the page wants it.
    pub wanted: bool,
}

/// The sites that may be closed to bring the count down to `kept`, the one
/// shown longest ago first, never one on screen, and how many must go. The
/// caller asks each in turn whether it holds changes and closes the clean
/// ones until enough are gone: one that objects is never closed for room,
/// so the count can stay over.
#[must_use]
pub fn room<'a>(live: &[Liveness<'a>], kept: u32) -> (usize, Vec<&'a str>) {
    let kept = kept.clamp(KEPT_MIN, KEPT_MAX) as usize;
    let needed = live.len().saturating_sub(kept);
    let mut candidates: Vec<&Liveness<'a>> = live.iter().filter(|site| !site.wanted).collect();
    candidates.sort_by(|a, b| a.last_shown.total_cmp(&b.last_shown));
    (
        needed,
        candidates.into_iter().map(|site| site.source).collect(),
    )
}

/// From `candidates` in order, the first `needed` that `clean` says hold
/// nothing unsaved.
#[must_use]
pub fn choose<'a>(
    candidates: &[&'a str],
    needed: usize,
    clean: impl Fn(&str) -> bool,
) -> Vec<&'a str> {
    candidates
        .iter()
        .copied()
        .filter(|source| clean(source))
        .take(needed)
        .collect()
}

/// The hidden sites that have been off screen for [`IDLE_CLOSE_MS`] at
/// `now`: to be asked, and closed when clean.
#[must_use]
pub fn idle<'a>(sites: impl IntoIterator<Item = &'a Site>, now: f64) -> Vec<&'a str> {
    sites
        .into_iter()
        .filter(|site| {
            !site.wanted
                && site
                    .hidden_since
                    .is_some_and(|at| now - at >= IDLE_CLOSE_MS)
        })
        .map(|site| site.source.as_str())
        .collect()
}

/// The hidden sites that should sleep at `now`, and the ones that should
/// wake (on screen again).
#[must_use]
pub fn sleepers<'a>(sites: impl IntoIterator<Item = &'a Site>, now: f64) -> Vec<&'a str> {
    sites
        .into_iter()
        .filter(|site| {
            !site.asleep
                && !site.wanted
                && site
                    .hidden_since
                    .is_some_and(|at| now - at >= SLEEP_AFTER_MS)
        })
        .map(|site| site.source.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Base;

    fn rules() -> SiteRules {
        SiteRules {
            base: Base::parse("https://jira.example").unwrap(),
            allowed: vec![Origin::parse("https://sso.example").unwrap()],
            others: vec![],
        }
    }

    #[test]
    fn a_new_site_is_opening_and_hidden() {
        let site = Site::opening("jira", "https://jira.example/", 0.0);
        assert_eq!(site.phase, SitePhase::Opening);
        assert!(site.landing);
        assert!(!site.visible());
        assert!(site.status().url.is_none());
    }

    #[test]
    fn the_view_shows_only_after_its_first_commit() {
        let r = rules();
        let mut site = Site::opening("jira", "https://jira.example/", 0.0);
        site.want(true, 1.0);
        assert!(!site.visible(), "wanted, nothing committed");
        site.commit("https://jira.example/secure/Dashboard.jspa", &r);
        assert_eq!(site.phase, SitePhase::Live);
        assert!(site.visible());
        assert!(site.own);
        assert!(!site.landing);
        site.want(false, 2.0);
        assert!(!site.visible());
    }

    #[test]
    fn a_commit_on_an_allowed_address_that_is_not_own_keeps_landing() {
        let r = rules();
        let mut site = Site::opening("jira", "https://jira.example/", 0.0);
        site.commit("https://sso.example/login", &r);
        assert!(site.landing);
        assert!(!site.own);
        assert_eq!(site.phase, SitePhase::Live);
        site.commit("https://jira.example/", &r);
        assert!(!site.landing);
    }

    #[test]
    fn a_question_hides_the_view_and_an_answer_goes_back() {
        let r = rules();
        let mut site = Site::opening("jira", "https://jira.example/", 0.0);
        site.want(true, 0.0);
        site.ask(
            Origin::parse("https://idp.example").unwrap(),
            "https://idp.example/sso?x",
        );
        assert_eq!(site.phase, SitePhase::Asks);
        assert_eq!(
            site.status().asks_origin.as_deref(),
            Some("https://idp.example")
        );
        assert!(!site.visible());
        // Declined with nothing ever shown: nothing to show.
        assert!(site.declined());
        // With a page shown before, it shows that page again.
        let mut shown = Site::opening("jira", "https://jira.example/", 0.0);
        shown.want(true, 0.0);
        shown.commit("https://jira.example/", &r);
        shown.land("https://jira.example/other", 5.0);
        shown.ask(
            Origin::parse("https://idp.example").unwrap(),
            "https://idp.example/",
        );
        assert!(!shown.declined());
        assert_eq!(shown.phase, SitePhase::Live);
        assert!(shown.visible());
    }

    #[test]
    fn a_landing_with_no_commit_fails_after_ten_seconds() {
        let mut site = Site::opening("jira", "https://jira.example/", 1000.0);
        assert!(!site.tick(10_999.0));
        assert!(site.tick(11_000.0));
        assert_eq!(site.phase, SitePhase::Failed);
        assert_eq!(site.failure, Some(SiteFailure::Timeout));
        // Reload starts it again.
        site.land("https://jira.example/", 20_000.0);
        assert_eq!(site.phase, SitePhase::Opening);
        assert!(site.failure.is_none());
    }

    #[test]
    fn a_live_site_reloading_keeps_showing_its_page() {
        let r = rules();
        let mut site = Site::opening("jira", "https://jira.example/", 0.0);
        site.want(true, 0.0);
        site.commit("https://jira.example/", &r);
        site.land("https://jira.example/", 50.0);
        assert_eq!(site.phase, SitePhase::Live);
        assert!(site.visible());
        assert!(!site.tick(60_000.0), "a live page never times out");
    }

    #[test]
    fn a_stopped_process_hides_the_view() {
        let r = rules();
        let mut site = Site::opening("jira", "https://jira.example/", 0.0);
        site.want(true, 0.0);
        site.commit("https://jira.example/", &r);
        site.stop();
        assert_eq!(site.phase, SitePhase::Stopped);
        assert!(!site.visible());
    }

    #[test]
    fn a_title_is_cleaned_on_the_way_in() {
        let mut site = Site::opening("jira", "https://jira.example/", 0.0);
        site.titled("PAY-1\n<script>\u{202E}");
        assert_eq!(site.title, "PAY-1 <script>");
    }

    fn live<'a>(source: &'a str, last_shown: f64, wanted: bool) -> Liveness<'a> {
        Liveness {
            source,
            last_shown,
            wanted,
        }
    }

    #[test]
    fn the_bound_offers_the_one_shown_longest_ago_first_and_never_the_shown_one() {
        let sites = [
            live("a", 30.0, false),
            live("b", 10.0, false),
            live("c", 50.0, true),
        ];
        let (needed, candidates) = room(&sites, 2);
        assert_eq!(needed, 1);
        assert_eq!(candidates, vec!["b", "a"]);
        assert_eq!(choose(&candidates, needed, |_| true), vec!["b"]);
    }

    #[test]
    fn the_bound_skips_one_that_objects() {
        let sites = [
            live("a", 30.0, false),
            live("b", 10.0, false),
            live("c", 50.0, true),
        ];
        let (needed, candidates) = room(&sites, 2);
        assert_eq!(choose(&candidates, needed, |s| s != "b"), vec!["a"]);
        // When all object, the count stays over.
        assert!(choose(&candidates, needed, |_| false).is_empty());
    }

    #[test]
    fn the_bound_is_clamped() {
        let sites = [
            live("a", 1.0, false),
            live("b", 2.0, false),
            live("c", 3.0, false),
        ];
        assert_eq!(room(&sites, 0).0, 2, "at least one stays");
        assert_eq!(room(&sites, 99).0, 0);
    }

    #[test]
    fn two_hours_hidden_is_idle_and_never_the_shown_one() {
        let mut hidden = Site::opening("a", "https://a.example/", 0.0);
        hidden.want(false, 0.0);
        let mut shown = Site::opening("b", "https://b.example/", 0.0);
        shown.want(true, 0.0);
        assert!(idle([&hidden, &shown], IDLE_CLOSE_MS - 1.0).is_empty());
        assert_eq!(idle([&hidden, &shown], IDLE_CLOSE_MS), vec!["a"]);
        assert_eq!(sleepers([&hidden, &shown], SLEEP_AFTER_MS), vec!["a"]);
        hidden.asleep = true;
        assert!(sleepers([&hidden, &shown], SLEEP_AFTER_MS).is_empty());
    }
}
