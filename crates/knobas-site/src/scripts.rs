//! The only scripts knobas runs in a site (decisions §4 item 3, §10). Each is
//! a named constant, run once at a moment the person caused, and its answer
//! is read as untrusted text. Nothing is added at document creation, nothing
//! listens and nothing stays.
//!
//! A source scan (`tests/scan.rs`) allows `evaluate_script` and
//! `evaluate_script_with_callback` only with these constants as their script.

/// Whether the page holds changes it has not saved: the page's own
/// `beforeunload` event, dispatched once, and whether a handler objected. A
/// page that would make a browser ask "Leave site?" answers `true`.
///
/// The text lives in `dirty-probe.js` so the front end's tests run the very
/// same script against pages in a DOM.
pub const DIRTY_PROBE: &str = include_str!("dirty-probe.js").trim_ascii_end();

/// The saved web sign-in's fill (decisions §13 (b), ux29): a function of
/// `(username, password, mode)` that finds the top document's one sign-in
/// form, types both, ticks "remember me" and submits; or, in the `look` mode,
/// only says whether such a form is there. Never run but through
/// [`fill_sign_in`], on a page natively checked to be on the source's own
/// origin (`sign_in::may_fill`).
///
/// The text lives in `fill-sign-in.js` so the front end's tests run the very
/// same script against recorded sign-in pages.
pub const FILL_SIGN_IN: &str = include_str!("fill-sign-in.js").trim_ascii_end();

/// What a fill does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillMode {
    /// Type, tick and submit.
    Fill,
    /// Only see whether a sign-in form is on the page.
    Look,
}

/// One run of [`FILL_SIGN_IN`]. The password never prints.
#[derive(Clone)]
pub struct SignInCall {
    pub username: String,
    pub password: String,
    pub mode: FillMode,
}

impl SignInCall {
    /// A look: no username or password goes into the page.
    #[must_use]
    pub fn look() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            mode: FillMode::Look,
        }
    }
}

impl std::fmt::Debug for SignInCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignInCall")
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

/// The script text of one call: [`FILL_SIGN_IN`] applied to the two values
/// as JSON string literals, which no value can break out of.
#[must_use]
pub fn fill_sign_in(call: &SignInCall) -> String {
    let quote = |text: &str| serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned());
    let mode = match call.mode {
        FillMode::Fill => "fill",
        FillMode::Look => "look",
    };
    format!(
        "({FILL_SIGN_IN})({},{},{})",
        quote(&call.username),
        quote(&call.password),
        quote(mode)
    )
}

/// What the page answered to [`DIRTY_PROBE`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dirty {
    /// It does not object to being left.
    Clean,
    /// It objects: a browser would ask.
    Changed,
    /// No answer in time, or one that is neither `true` nor `false`. Treated
    /// as [`Dirty::Changed`]: asking once too often is the safe side.
    Unknown,
}

impl Dirty {
    /// The answer as the engine hands it back: the JSON text of the
    /// script's value. Exactly `true` or `false`; anything else is
    /// [`Dirty::Unknown`].
    #[must_use]
    pub fn from_answer(answer: &str) -> Self {
        match answer {
            "false" => Self::Clean,
            "true" => Self::Changed,
            _ => Self::Unknown,
        }
    }

    /// Whether leaving needs the person's word.
    #[must_use]
    pub fn asks(self) -> bool {
        !matches!(self, Self::Clean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exactly_true_or_false_is_an_answer() {
        assert_eq!(Dirty::from_answer("false"), Dirty::Clean);
        assert_eq!(Dirty::from_answer("true"), Dirty::Changed);
        for odd in [
            "\"true\"",
            "\"false\"",
            "1",
            "0",
            "{}",
            "",
            "null",
            "undefined",
            " true",
            "TRUE",
        ] {
            assert_eq!(Dirty::from_answer(odd), Dirty::Unknown, "{odd:?}");
        }
    }

    #[test]
    fn unknown_asks() {
        assert!(!Dirty::Clean.asks());
        assert!(Dirty::Changed.asks());
        assert!(Dirty::Unknown.asks());
    }

    #[test]
    fn a_fill_call_quotes_its_values_and_never_prints_the_password() {
        let call = SignInCall {
            username: "bj\u{f6}rn".into(),
            password: "a\"),alert(1),(\"\n".into(),
            mode: FillMode::Fill,
        };
        let text = fill_sign_in(&call);
        assert!(text.starts_with("(function (username, password, mode) {"));
        // Each value is one JSON string literal: the password's quote and
        // line break are escaped, so it cannot end its string.
        assert!(
            text.ends_with(r#"})("björn","a\"),alert(1),(\"\n","fill")"#),
            "{text}"
        );
        let shown = format!("{call:?}");
        assert!(!shown.contains("alert"), "{shown}");
        assert!(
            shown.contains("Fill"),
            "the mode is what a log line is read for: {shown}"
        );
        assert!(fill_sign_in(&SignInCall::look()).ends_with(r#"})("","","look")"#));
    }

    #[test]
    fn the_probe_is_one_expression_with_no_trailing_newline() {
        assert!(DIRTY_PROBE.starts_with("(function(){"));
        assert!(DIRTY_PROBE.ends_with("})()"));
        assert!(!DIRTY_PROBE.contains('\n'));
    }
}
