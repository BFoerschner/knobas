//! A saved web sign-in, typed into the site's own form (ADR-0050, decisions
//! §13 (b), ux29). Pure: when knobas may fill, which page is a sign-in page,
//! and the one-try rule. The script is [`crate::scripts::FILL_SIGN_IN`]; the
//! username and password come from the source's keychain item through the
//! shell, at the moment of the fill, and are never kept here.
//!
//! - **Where.** Only into a page whose top-level origin is exactly the
//!   source's own, read from the view at that moment and again equal to the
//!   committed one ([`may_fill`]). Never on an allowed address: a sign-in
//!   provider's page is the provider's.
//! - **When.** On *Fill sign-in*, and by itself once a page loads on a path
//!   the adapter declares (`/login.jsp`), or on any page of the own origin
//!   for an adapter that declares `any_page` (Uptime Kuma's dialog).
//! - **One try.** After a fill, a sign-in form still on the page means the
//!   site did not take it: the bar says so and knobas waits for a press.

use url::Url;

use crate::policy::{Base, Origin, SiteRules};

/// How long a fill looks for a form that has not rendered yet: a
/// single-page application draws its sign-in dialog after its scripts ran.
pub const LOOK_FOR_MS: f64 = 6000.0;
/// Between two looks.
pub const LOOK_EVERY_MS: f64 = 500.0;
/// After a fill that did not navigate, when to see whether the form is gone.
pub const CHECK_AFTER_MS: f64 = 3000.0;
/// After a fill, from the next page's load to the check.
pub const CHECK_SETTLE_MS: f64 = 300.0;

/// Where a source's site shows its sign-in form (the descriptor's
/// `web_sign_in`), and whether a sign-in is saved for it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignInRule {
    /// A web sign-in is stored in the source's keychain item.
    pub saved: bool,
    /// Paths after the base URL that show the sign-in form.
    pub paths: Vec<String>,
    /// The form can stand on any page of the own origin.
    pub any_page: bool,
}

/// Why a fill was refused natively.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRefusal {
    /// No sign-in is saved for the source.
    NotSaved,
    /// The page is not on the source's own origin.
    NotOwn,
    /// The view is between addresses: what it shows is not what committed.
    Moving,
}

fn origin_of(url: &str) -> Option<Origin> {
    Url::parse(url).ok().as_ref().and_then(Origin::of)
}

/// Whether knobas may type the saved sign-in into the page now. `committed`
/// is the top-level address the site last committed; `live` is the view's
/// own address, read natively at this moment. Both must be on exactly the
/// source's own origin.
///
/// # Errors
///
/// The [`FillRefusal`].
pub fn may_fill(
    rules: &SiteRules,
    saved: bool,
    committed: Option<&str>,
    live: Option<&str>,
) -> Result<(), FillRefusal> {
    if !saved {
        return Err(FillRefusal::NotSaved);
    }
    let own = &rules.base.origin;
    let committed = committed.and_then(origin_of);
    let live = live.and_then(origin_of);
    match (committed, live) {
        (Some(committed), Some(live)) if committed == *own && live == *own => Ok(()),
        (Some(committed), Some(live)) if committed == *own && live != committed => {
            Err(FillRefusal::Moving)
        }
        _ => Err(FillRefusal::NotOwn),
    }
}

/// Whether `url` is the source's sign-in page: on its own origin, at a path
/// the adapter declares after the base path, or anywhere for `any_page`.
#[must_use]
pub fn is_sign_in_page(base: &Base, rule: &SignInRule, url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    if Origin::of(&parsed).as_ref() != Some(&base.origin) {
        return false;
    }
    if rule.any_page {
        return true;
    }
    let path = parsed.path();
    let Some(rest) = path.strip_prefix(base.path.as_str()) else {
        return false;
    };
    rule.paths.iter().any(|declared| declared == rest)
}

/// What the fill script answered (its JSON text).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillAnswer {
    /// It typed both fields and submitted the form.
    Filled,
    /// (A look.) A sign-in form is on the page.
    Form,
    /// No sign-in form on the top document.
    NoForm,
    /// Two password fields: a change-password form, left alone.
    Ambiguous,
    /// The page asks for a one-time code: that one is the person's.
    SecondFactor,
    /// Anything else, or no answer.
    Odd,
}

impl FillAnswer {
    /// The answer as the engine hands it back: the JSON text of a string.
    #[must_use]
    pub fn from_answer(text: &str) -> Self {
        match serde_json::from_str::<String>(text.trim()).as_deref() {
            Ok("filled") => Self::Filled,
            Ok("form") => Self::Form,
            Ok("no form") => Self::NoForm,
            Ok("ambiguous") => Self::Ambiguous,
            Ok("second factor") => Self::SecondFactor,
            _ => Self::Odd,
        }
    }
}

/// What the bar says about the sign-in (`SiteStatus::sign_in`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "site.ts")]
pub enum SiteSignIn {
    /// A fill is on its way, or knobas is seeing whether the site took it.
    Filling,
    /// "⟨source⟩ did not take the saved sign-in."
    NotTaken,
    /// The page asks for a code knobas does not type.
    SecondFactor,
    /// A press found no sign-in form on the page.
    NoForm,
    /// A press found a form with two password fields.
    Ambiguous,
    /// The page is on another origin, a sign-in provider's: knobas fills
    /// only the source's own form.
    Provider,
}

/// What the controller does next for one site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillAct {
    /// Fetch the saved sign-in and fill. `auto`: knobas started it.
    Fill { auto: bool },
    /// See whether a sign-in form is still on the page.
    Look,
}

/// One site's fill state: the one-try rule.
#[derive(Clone, Debug, PartialEq)]
pub struct Filler {
    /// A sign-in page may be filled by itself.
    pub armed: bool,
    /// The fill in flight was started by knobas.
    auto: bool,
    /// A fill is asked for and has not answered.
    asked: bool,
    /// Look again for a form until then.
    looking_until: Option<f64>,
    /// The next look for a form that has not rendered.
    next_try: Option<f64>,
    /// A fill was submitted: see at this time whether it was taken.
    check_at: Option<f64>,
    /// A fill was submitted and not yet judged.
    submitted: bool,
    /// What the bar says.
    pub said: Option<SiteSignIn>,
}

impl Default for Filler {
    fn default() -> Self {
        Self {
            armed: true,
            auto: false,
            asked: false,
            looking_until: None,
            next_try: None,
            check_at: None,
            submitted: false,
            said: None,
        }
    }
}

impl Filler {
    /// Whether a fill is under way: asked, looking or being judged.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.asked || self.next_try.is_some() || self.submitted
    }

    /// A page on the own origin finished loading. `sign_in_page`: it is a
    /// sign-in page by the adapter's declaration; `saved`: a sign-in is
    /// stored.
    pub fn loaded(&mut self, sign_in_page: bool, saved: bool, now: f64) -> Option<FillAct> {
        if self.submitted {
            self.check_at = Some(now + CHECK_SETTLE_MS);
            return None;
        }
        if self.armed && saved && sign_in_page && !self.asked {
            self.armed = false;
            self.auto = true;
            self.asked = true;
            self.looking_until = Some(now + LOOK_FOR_MS);
            self.said = Some(SiteSignIn::Filling);
            return Some(FillAct::Fill { auto: true });
        }
        None
    }

    /// He pressed *Fill sign-in*.
    pub fn press(&mut self, now: f64) -> FillAct {
        self.armed = false;
        self.auto = false;
        self.asked = true;
        self.submitted = false;
        self.check_at = None;
        self.next_try = None;
        self.looking_until = Some(now + LOOK_FOR_MS);
        self.said = Some(SiteSignIn::Filling);
        FillAct::Fill { auto: false }
    }

    /// A sign-in was saved, changed or removed: the next sign-in page is
    /// filled again by itself.
    pub fn rearm(&mut self) {
        *self = Self::default();
    }

    /// The fill could not run: refused natively, or nothing saved.
    pub fn refused(&mut self) {
        let auto = self.auto;
        *self = Self {
            armed: auto,
            ..Self::default()
        };
    }

    /// The fill script answered.
    pub fn answered(&mut self, answer: FillAnswer, now: f64) {
        self.asked = false;
        match answer {
            FillAnswer::Filled => {
                self.looking_until = None;
                self.next_try = None;
                self.submitted = true;
                self.check_at = Some(now + CHECK_AFTER_MS);
                self.said = Some(SiteSignIn::Filling);
            }
            FillAnswer::NoForm | FillAnswer::Form | FillAnswer::Odd
                if self
                    .looking_until
                    .is_some_and(|until| now + LOOK_EVERY_MS <= until) =>
            {
                self.next_try = Some(now + LOOK_EVERY_MS);
            }
            FillAnswer::SecondFactor => {
                self.looking_until = None;
                self.next_try = None;
                self.said = Some(SiteSignIn::SecondFactor);
            }
            FillAnswer::Ambiguous => {
                self.looking_until = None;
                self.next_try = None;
                // Knobas started it on a page that is no sign-in after all:
                // nothing was typed, so nothing was tried.
                self.armed = self.auto;
                self.said = (!self.auto).then_some(SiteSignIn::Ambiguous);
            }
            FillAnswer::NoForm | FillAnswer::Form | FillAnswer::Odd => {
                self.looking_until = None;
                self.next_try = None;
                // Signed in already, most likely: nothing was typed.
                self.armed = self.auto || self.armed;
                self.said = (!self.auto).then_some(SiteSignIn::NoForm);
            }
        }
    }

    /// A look after a fill answered.
    pub fn judged(&mut self, answer: FillAnswer) {
        self.submitted = false;
        self.check_at = None;
        match answer {
            FillAnswer::Form | FillAnswer::Filled | FillAnswer::Ambiguous => {
                self.armed = false;
                self.said = Some(SiteSignIn::NotTaken);
            }
            FillAnswer::SecondFactor => {
                self.armed = false;
                self.said = Some(SiteSignIn::SecondFactor);
            }
            FillAnswer::NoForm | FillAnswer::Odd => {
                self.armed = true;
                self.said = None;
            }
        }
    }

    /// The clock: a look again for a form, or the check after a fill.
    pub fn tick(&mut self, now: f64) -> Option<FillAct> {
        if let Some(at) = self.next_try
            && now >= at
            && !self.asked
        {
            self.next_try = None;
            self.asked = true;
            return Some(FillAct::Fill { auto: self.auto });
        }
        if let Some(at) = self.check_at
            && now >= at
        {
            self.check_at = None;
            return Some(FillAct::Look);
        }
        None
    }

    /// He went to another page by hand while nothing was being filled: a
    /// note about the last fill goes.
    pub fn moved_on(&mut self) {
        if !self.busy()
            && matches!(
                self.said,
                Some(SiteSignIn::NoForm | SiteSignIn::Ambiguous | SiteSignIn::Filling)
            )
        {
            self.said = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> SiteRules {
        SiteRules {
            base: Base::parse("https://jira.example/jira").unwrap(),
            allowed: vec![Origin::parse("https://sso.example").unwrap()],
            others: vec![],
        }
    }

    #[test]
    fn a_fill_needs_a_saved_sign_in_and_exactly_the_own_origin() {
        let r = rules();
        let own = Some("https://jira.example/jira/login.jsp");
        assert_eq!(may_fill(&r, true, own, own), Ok(()));
        assert_eq!(may_fill(&r, false, own, own), Err(FillRefusal::NotSaved));
        // An allowed address is not the own one.
        let sso = Some("https://sso.example/login");
        assert_eq!(may_fill(&r, true, sso, sso), Err(FillRefusal::NotOwn));
        // In a redirect: committed on its own origin, showing another.
        assert_eq!(may_fill(&r, true, own, sso), Err(FillRefusal::Moving));
        // Another port, scheme or host is another origin.
        for other in [
            "http://jira.example/jira/login.jsp",
            "https://jira.example:8443/jira/login.jsp",
            "https://evil.jira.example/jira/login.jsp",
        ] {
            assert_eq!(
                may_fill(&r, true, Some(other), Some(other)),
                Err(FillRefusal::NotOwn),
                "{other}"
            );
        }
        assert_eq!(may_fill(&r, true, None, own), Err(FillRefusal::NotOwn));
        // Neither address the source's own, and not the same one: not own,
        // never "between addresses".
        assert_eq!(
            may_fill(&r, true, sso, Some("https://other.example/x")),
            Err(FillRefusal::NotOwn)
        );
        assert_eq!(may_fill(&r, true, own, None), Err(FillRefusal::NotOwn));
        assert_eq!(
            may_fill(&r, true, Some("about:blank"), Some("about:blank")),
            Err(FillRefusal::NotOwn)
        );
    }

    #[test]
    fn a_sign_in_page_is_a_declared_path_after_the_base_on_the_own_origin() {
        let base = rules().base;
        let rule = SignInRule {
            saved: true,
            paths: vec!["/login.jsp".into()],
            any_page: false,
        };
        assert!(is_sign_in_page(
            &base,
            &rule,
            "https://jira.example/jira/login.jsp"
        ));
        assert!(is_sign_in_page(
            &base,
            &rule,
            "https://jira.example/jira/login.jsp?os_destination=%2F"
        ));
        assert!(!is_sign_in_page(
            &base,
            &rule,
            "https://jira.example/login.jsp"
        ));
        assert!(!is_sign_in_page(
            &base,
            &rule,
            "https://jira.example/jira/secure/Dashboard.jspa"
        ));
        assert!(!is_sign_in_page(
            &base,
            &rule,
            "https://sso.example/jira/login.jsp"
        ));
        let anywhere = SignInRule {
            any_page: true,
            ..SignInRule::default()
        };
        assert!(is_sign_in_page(
            &base,
            &anywhere,
            "https://jira.example/dashboard"
        ));
        assert!(!is_sign_in_page(
            &base,
            &anywhere,
            "https://sso.example/dashboard"
        ));
    }

    #[test]
    fn the_answer_is_the_json_text_of_a_known_word() {
        assert_eq!(FillAnswer::from_answer("\"filled\""), FillAnswer::Filled);
        assert_eq!(FillAnswer::from_answer("\"no form\""), FillAnswer::NoForm);
        assert_eq!(
            FillAnswer::from_answer("\"second factor\""),
            FillAnswer::SecondFactor
        );
        for odd in ["filled", "", "null", "true", "\"FILLED\"", "{}"] {
            assert_eq!(FillAnswer::from_answer(odd), FillAnswer::Odd, "{odd:?}");
        }
    }

    #[test]
    fn a_sign_in_page_is_filled_by_itself_once_per_landing() {
        let mut f = Filler::default();
        assert_eq!(
            f.loaded(true, true, 0.0),
            Some(FillAct::Fill { auto: true })
        );
        // The same landing reported twice, or another load while asked: no
        // second fill.
        assert_eq!(f.loaded(true, true, 10.0), None);
        f.answered(FillAnswer::Filled, 20.0);
        // The sign-in page again after the fill: judged, never filled again.
        assert_eq!(f.loaded(true, true, 500.0), None);
        assert_eq!(f.tick(500.0 + CHECK_SETTLE_MS), Some(FillAct::Look));
        f.judged(FillAnswer::Form);
        assert_eq!(f.said, Some(SiteSignIn::NotTaken));
        assert_eq!(f.loaded(true, true, 2000.0), None, "until he presses");
        assert_eq!(f.press(3000.0), FillAct::Fill { auto: false });
    }

    #[test]
    fn a_taken_sign_in_arms_the_next_expiry() {
        let mut f = Filler::default();
        f.loaded(true, true, 0.0);
        f.answered(FillAnswer::Filled, 0.0);
        assert_eq!(f.loaded(false, true, 400.0), None);
        assert_eq!(f.tick(800.0), Some(FillAct::Look));
        f.judged(FillAnswer::NoForm);
        assert_eq!(f.said, None);
        assert_eq!(
            f.loaded(true, true, 9000.0),
            Some(FillAct::Fill { auto: true })
        );
    }

    #[test]
    fn nothing_saved_or_no_sign_in_page_fills_nothing() {
        let mut f = Filler::default();
        assert_eq!(f.loaded(true, false, 0.0), None);
        assert_eq!(f.loaded(false, true, 0.0), None);
        assert!(f.armed);
    }

    #[test]
    fn a_form_not_drawn_yet_is_looked_for_until_the_time_is_up() {
        let mut f = Filler::default();
        f.loaded(true, true, 0.0);
        f.answered(FillAnswer::NoForm, 10.0);
        assert_eq!(f.tick(200.0), None);
        assert_eq!(f.tick(510.0), Some(FillAct::Fill { auto: true }));
        f.answered(FillAnswer::NoForm, 520.0);
        // Past the time: signed in already, quietly, and armed again.
        f.answered(FillAnswer::NoForm, LOOK_FOR_MS);
        assert_eq!(f.tick(LOOK_FOR_MS + 1000.0), None);
        assert_eq!(f.said, None, "knobas's own look says nothing");
        assert!(f.armed);
        assert_eq!(
            f.loaded(true, true, LOOK_FOR_MS + 2000.0),
            Some(FillAct::Fill { auto: true })
        );
    }

    #[test]
    fn a_press_that_finds_nothing_says_so() {
        let mut f = Filler::default();
        f.press(0.0);
        f.answered(FillAnswer::NoForm, LOOK_FOR_MS);
        assert_eq!(f.said, Some(SiteSignIn::NoForm));
        let mut f = Filler::default();
        f.press(0.0);
        f.answered(FillAnswer::Ambiguous, 0.0);
        assert_eq!(f.said, Some(SiteSignIn::Ambiguous));
    }

    #[test]
    fn a_second_factor_is_never_typed() {
        let mut f = Filler::default();
        f.loaded(true, true, 0.0);
        f.answered(FillAnswer::SecondFactor, 0.0);
        assert_eq!(f.said, Some(SiteSignIn::SecondFactor));
        assert_eq!(f.tick(10_000.0), None);
        // After a fill, a page that asks for a code.
        let mut f = Filler::default();
        f.press(0.0);
        f.answered(FillAnswer::Filled, 0.0);
        assert_eq!(f.tick(CHECK_AFTER_MS), Some(FillAct::Look));
        f.judged(FillAnswer::SecondFactor);
        assert_eq!(f.said, Some(SiteSignIn::SecondFactor));
    }

    #[test]
    fn every_known_word_is_read() {
        for (word, answer) in [
            ("form", FillAnswer::Form),
            ("ambiguous", FillAnswer::Ambiguous),
            ("filled", FillAnswer::Filled),
            ("no form", FillAnswer::NoForm),
            ("second factor", FillAnswer::SecondFactor),
        ] {
            assert_eq!(
                FillAnswer::from_answer(&format!("{word:?}")),
                answer,
                "{word}"
            );
        }
    }

    #[test]
    fn a_fill_is_busy_while_asked_looking_or_judged_each_alone() {
        assert!(!Filler::default().busy());
        let asked = Filler {
            asked: true,
            ..Filler::default()
        };
        let looking = Filler {
            next_try: Some(1.0),
            ..Filler::default()
        };
        let submitted = Filler {
            submitted: true,
            ..Filler::default()
        };
        assert!(asked.busy() && looking.busy() && submitted.busy());
        let mut f = Filler::default();
        f.press(1000.0);
        f.answered(FillAnswer::Filled, 1000.0);
        f.judged(FillAnswer::NoForm);
        assert!(!f.busy());
    }

    /// The clock is not at zero: the look window and the retry are counted
    /// from the landing and the press.
    #[test]
    fn the_look_window_runs_from_the_moment_of_the_landing_or_the_press() {
        let mut f = Filler::default();
        f.loaded(true, true, 50_000.0);
        f.answered(FillAnswer::NoForm, 50_010.0);
        assert_eq!(f.tick(50_510.0), Some(FillAct::Fill { auto: true }));
        let mut f = Filler::default();
        f.press(80_000.0);
        f.answered(FillAnswer::NoForm, 80_010.0);
        assert_eq!(f.tick(80_510.0), Some(FillAct::Fill { auto: false }));
        f.answered(FillAnswer::NoForm, 80_000.0 + LOOK_FOR_MS);
        assert_eq!(f.said, Some(SiteSignIn::NoForm), "the window ended");
    }

    #[test]
    fn a_refused_fill_arms_again_only_when_knobas_started_it() {
        let mut f = Filler::default();
        f.loaded(true, true, 0.0);
        f.refused();
        assert!(f.armed && !f.busy());
        assert_eq!(f.said, None);
        let mut f = Filler::default();
        f.press(0.0);
        f.refused();
        assert!(!f.armed && !f.busy());
        assert_eq!(f.said, None, "the bar stops saying Filling");
    }

    #[test]
    fn moving_on_clears_a_note_about_a_finished_fill_and_nothing_else() {
        let mut f = Filler::default();
        f.press(0.0);
        f.answered(FillAnswer::NoForm, LOOK_FOR_MS);
        assert_eq!(f.said, Some(SiteSignIn::NoForm));
        f.moved_on();
        assert_eq!(f.said, None);
        // While a fill is under way its note stays.
        let mut f = Filler::default();
        f.press(0.0);
        f.moved_on();
        assert_eq!(f.said, Some(SiteSignIn::Filling));
        // What the site did with it stays until the next fill.
        let mut f = Filler::default();
        f.press(0.0);
        f.answered(FillAnswer::Filled, 0.0);
        f.judged(FillAnswer::Form);
        f.moved_on();
        assert_eq!(f.said, Some(SiteSignIn::NotTaken));
    }

    #[test]
    fn a_new_sign_in_arms_again() {
        let mut f = Filler::default();
        f.loaded(true, true, 0.0);
        f.answered(FillAnswer::Filled, 0.0);
        f.tick(CHECK_AFTER_MS);
        f.judged(FillAnswer::Form);
        f.rearm();
        assert_eq!(f.said, None);
        assert_eq!(
            f.loaded(true, true, 1.0),
            Some(FillAct::Fill { auto: true })
        );
    }
}
