//! The order of versions (decisions D5).
//!
//! Plain semver compares a pre-release's identifiers as text, so it puts `3.2.9-ux9` above
//! `3.2.9-ux10` (measured). knobas's rule: the numbers, then a release above its own
//! pre-releases, then each identifier as a word and a trailing number, a missing number counting
//! as less than any number.

use std::cmp::Ordering;

pub use semver::Version;

/// `text` as a version, a leading `v` (a tag) dropped.
#[must_use]
pub fn parse(text: &str) -> Option<Version> {
    let text = text.trim();
    let bare = text.strip_prefix('v').unwrap_or(text);
    Version::parse(bare).ok()
}

/// knobas's order of two versions.
#[must_use]
pub fn order(a: &Version, b: &Version) -> Ordering {
    let core = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if core != Ordering::Equal {
        return core;
    }
    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => identifiers(a.pre.as_str()).cmp(&identifiers(b.pre.as_str())),
    }
}

/// Whether `candidate` is newer than `current` by [`order`]. Text that is no version is newer
/// than nothing.
#[must_use]
pub fn newer(candidate: &str, current: &str) -> bool {
    match (parse(candidate), parse(current)) {
        (Some(candidate), Some(current)) => order(&candidate, &current) == Ordering::Greater,
        _ => false,
    }
}

/// Whether `version` carries a pre-release part (`3.2.9-ux28`).
#[must_use]
pub fn is_pre_release(version: &str) -> bool {
    parse(version).is_some_and(|version| !version.pre.is_empty())
}

/// A pre-release split on `.`, each identifier as its leading text and its trailing digits.
fn identifiers(pre: &str) -> Vec<(String, Option<u64>)> {
    pre.split('.')
        .map(|id| {
            let cut = id.trim_end_matches(|c: char| c.is_ascii_digit()).len();
            let (word, digits) = id.split_at(cut);
            (word.to_owned(), digits.parse().ok())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(a: &str, b: &str) -> Ordering {
        order(&parse(a).unwrap(), &parse(b).unwrap())
    }

    /// Section 2's table, as measured with the spike.
    #[test]
    fn the_table_of_section_two() {
        for (a, b, want) in [
            ("3.2.9-ux27", "3.2.9", Ordering::Less),
            ("3.2.9-ux27", "3.2.9-ux28", Ordering::Less),
            ("3.2.9-ux9", "3.2.9-ux10", Ordering::Less),
            ("3.2.9-ux9", "3.2.9-ux27", Ordering::Less),
            ("3.2.9-ux99", "3.2.9-ux100", Ordering::Less),
            ("3.2.9", "3.2.10-ux1", Ordering::Less),
            ("3.2.8", "3.2.9-ux4", Ordering::Less),
            ("3.2.9-ux27", "3.2.9-winperf1", Ordering::Less),
            ("3.2.9-rc1", "3.2.9-ux27", Ordering::Less),
            ("3.2.9-ux.9", "3.2.9-ux.10", Ordering::Less),
            ("3.2.9-ux28", "3.2.9-ux28", Ordering::Equal),
            ("3.2.10", "3.2.9", Ordering::Greater),
        ] {
            assert_eq!(cmp(a, b), want, "{a} against {b}");
            assert_eq!(cmp(b, a), want.reverse(), "{b} against {a}");
        }
    }

    #[test]
    fn a_release_is_newer_than_its_own_pre_releases() {
        assert!(newer("3.2.9", "3.2.9-ux27"));
        assert!(newer("v3.2.9", "3.2.9-ux99"));
        assert!(!newer("3.2.9-ux27", "3.2.9"));
    }

    #[test]
    fn text_that_is_no_version_is_never_newer() {
        assert!(!newer("latest", "3.2.9"));
        assert!(!newer("3.2.10", "dev"));
    }

    #[test]
    fn a_tag_parses_without_its_v() {
        assert_eq!(parse("v3.2.9-ux28"), parse("3.2.9-ux28"));
        assert!(is_pre_release("v3.2.9-ux28"));
        assert!(!is_pre_release("3.2.9"));
    }
}
