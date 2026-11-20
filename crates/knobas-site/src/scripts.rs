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
    fn the_probe_is_one_expression_with_no_trailing_newline() {
        assert!(DIRTY_PROBE.starts_with("(function(){"));
        assert!(DIRTY_PROBE.ends_with("})()"));
        assert!(!DIRTY_PROBE.contains('\n'));
    }
}
