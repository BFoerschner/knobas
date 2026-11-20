//! A ticket key typed with a space, or with no separator, read as the key
//! (ux25, Björn: "typing a ticket key and id with space instead of - doesn't
//! match. I want to be able to type like pay 123 and it shows up pay-123").
//!
//! `pay 123`, `PAY 123`, `pay123`, `pay_123` and `pay.123` read as `PAY-123`,
//! and `pay 12` as `PAY-12`, which a word still being typed asks as a prefix,
//! as `PAY-12` is asked. Only for a project the mirror's tickets are numbered
//! under ([`Projects`]): `top 10` stays two words unless `TOP` is a project.
//! The reading is **an addition**: the boxes ask the key beside the words as
//! typed and keep the words' answer below the key's
//! (`docs/decisions/2026-11-20-a-key-typed-with-a-space-is-the-key.md`).
//!
//! The number is never fuzzed: `pay 132` reads `PAY-132`, never `PAY-123`.

use std::sync::Arc;

/// The project keys the mirror's tickets are numbered under, lowered and
/// sorted: what a typed word must be for a number after it to read as a key.
///
/// Held by the picker beside the names (read off the ticket ids it already
/// holds, so it costs no read of its own) and handed to each query through
/// the vocabulary. Empty, it reads nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Projects(Arc<[String]>);

/// `[A-Za-z][A-Za-z0-9_]*`: a project key's shape (Jira's).
fn project_shaped(word: &str) -> bool {
    let mut chars = word.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn digits(word: &str) -> bool {
    !word.is_empty() && word.bytes().all(|b| b.is_ascii_digit())
}

/// `PAY-123` as `("PAY", "123")`: a project-shaped key, a `-`, and digits.
fn key_parts(key: &str) -> Option<(&str, &str)> {
    key.rsplit_once('-')
        .filter(|(project, number)| project_shaped(project) && digits(number))
}

impl Projects {
    /// The projects of the ticket ids in `ids` (`jira:PAY-231`): the key
    /// after the source's namespace, where it is shaped as a key. An id of
    /// another shape (`gitea:owner/repo#12`) names no project.
    #[must_use]
    pub fn of_ids<'a>(ids: impl IntoIterator<Item = &'a str>) -> Self {
        let mut projects: Vec<String> = ids
            .into_iter()
            .filter_map(|id| {
                let key = id.split_once(':').map_or(id, |(_, key)| key);
                key_parts(key).map(|(project, _)| project.to_ascii_lowercase())
            })
            .collect();
        projects.sort_unstable();
        projects.dedup();
        Self(projects.into())
    }

    /// How many projects are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether no project is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether `word` names a project, in any case.
    #[must_use]
    pub fn holds(&self, word: &str) -> bool {
        project_shaped(word)
            && self
                .0
                .binary_search_by(|held| {
                    held.as_str()
                        .bytes()
                        .cmp(word.bytes().map(|b| b.to_ascii_lowercase()))
                })
                .is_ok()
    }

    /// One token that spells a key with no separator, `_` or `.` between
    /// its project and its number (`pay123`, `pay_123`, `pay.123`), as the
    /// key. `pay-123` is a key already and is left as typed. Where two
    /// projects could begin it (`AB` and `AB1` for `ab1123`), the longer.
    fn joined(&self, token: &str) -> Option<String> {
        let start = token.len() - token.bytes().rev().take_while(u8::is_ascii_digit).count();
        if start == 0 || start == token.len() {
            return None;
        }
        let head = &token[..start];
        if let Some(project) = head.strip_suffix(['_', '.']) {
            return self
                .holds(project)
                .then(|| key_of(project, &token[start..]));
        }
        // No separator: the project may end in digits itself, so every
        // split that leaves one digit is tried, the longest project first.
        (start..token.len())
            .rev()
            .find(|&split| self.holds(&token[..split]))
            .map(|split| key_of(&token[..split], &token[split..]))
    }

    /// `text` with every key typed with a space or no separator written as
    /// the key (`pay 123 retry` → `PAY-123 retry`), or `None` where nothing
    /// reads as one. The other words are kept as typed, in their order.
    #[must_use]
    pub fn read(&self, text: &str) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let tokens = crate::query::tokenize(text);
        let mut out: Vec<String> = Vec::with_capacity(tokens.len());
        let mut read = false;
        let mut at = 0;
        while at < tokens.len() {
            let token = &tokens[at];
            if let Some(number) = tokens.get(at + 1).filter(|next| digits(next))
                && self.holds(token)
            {
                out.push(key_of(token, number));
                read = true;
                at += 2;
                continue;
            }
            match self.joined(token) {
                Some(key) => {
                    out.push(key);
                    read = true;
                }
                None => out.push(token.clone()),
            }
            at += 1;
        }
        read.then(|| out.join(" "))
    }
}

/// The word filter's alternatives for `text` (the Tickets view's and the
/// boards', [`crate::query::word_alternatives`]), and where `projects` read a
/// key in it, the alternatives of the text so read beside them: `pay123`
/// admits `PAY-123` as typing `PAY-123` does, and what the words as typed
/// admit stays admitted. Each alternative once.
pub(crate) fn alternatives(text: &str, projects: &Projects) -> Vec<crate::query::WordAlternative> {
    let mut alternatives = crate::query::word_alternatives(text);
    if let Some(keyed) = projects.read(text) {
        for alternative in crate::query::word_alternatives(&keyed) {
            if !alternatives.contains(&alternative) {
                alternatives.push(alternative);
            }
        }
    }
    alternatives
}

fn key_of(project: &str, number: &str) -> String {
    format!("{}-{number}", project.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projects(keys: &[&str]) -> Projects {
        Projects::of_ids(keys.iter().copied())
    }

    #[test]
    fn the_projects_are_read_off_ticket_keys_and_nothing_else() {
        let held = projects(&[
            "jira:PAY-231",
            "jira:PAY-1",
            "jira:PAYOUT-7",
            "jira:MY_PROJ-3",
            "jira-eu:K8S-12",
            "gitea:tidewater/payout#12",
            "wiki:9001",
            "jira:-12",
            "jira:1PAY-3",
            "jira:PAY-",
        ]);
        assert_eq!(&*held.0, ["k8s", "my_proj", "pay", "payout"]);
        assert!(held.holds("PAY") && held.holds("pay") && held.holds("Pay"));
        assert!(!held.holds("pa") && !held.holds("payo") && !held.holds(""));
    }

    #[test]
    fn every_spelling_reads_as_the_key() {
        let held = projects(&["jira:PAY-1"]);
        for typed in [
            "pay 123", "PAY 123", "Pay 123", "pay123", "PAY123", "pay_123", "pay.123",
        ] {
            assert_eq!(held.read(typed).as_deref(), Some("PAY-123"), "`{typed}`");
        }
        // A part of the number, as `PAY-12` is asked.
        assert_eq!(held.read("pay 12").as_deref(), Some("PAY-12"));
        assert_eq!(held.read("pay1").as_deref(), Some("PAY-1"));
    }

    #[test]
    fn the_other_words_stay_as_typed() {
        let held = projects(&["jira:PAY-1", "jira:OPS-4"]);
        assert_eq!(
            held.read("retry pay 123 sepa").as_deref(),
            Some("retry PAY-123 sepa")
        );
        assert_eq!(
            held.read("pay 1 ops2").as_deref(),
            Some("PAY-1 OPS-2"),
            "two keys"
        );
    }

    #[test]
    fn nothing_reads_where_no_project_is_named() {
        let held = projects(&["jira:PAY-1"]);
        for typed in [
            "top 10",
            "java 21",
            "top10",
            "pay",
            "pay ",
            "pay retry",
            "123",
            "pay-123",
            "PAY-123",
            "-pay 123",
            "\"pay 123\"",
            "pay 12a",
            "pay #123",
            "pay- 123",
            "pay_",
        ] {
            assert_eq!(held.read(typed), None, "`{typed}`");
        }
        assert_eq!(
            Projects::default().read("pay 123"),
            None,
            "no projects held"
        );
    }

    #[test]
    fn a_project_that_is_a_common_word_reads_too() {
        let held = projects(&["jira:TOP-1"]);
        assert_eq!(held.read("top 10").as_deref(), Some("TOP-10"));
        assert_eq!(held.read("java 21"), None);
    }

    #[test]
    fn a_project_that_begins_another_reads_as_itself() {
        let held = projects(&["jira:PAY-1", "jira:PAYOUT-1"]);
        assert_eq!(held.read("payout 12").as_deref(), Some("PAYOUT-12"));
        assert_eq!(held.read("payout12").as_deref(), Some("PAYOUT-12"));
        assert_eq!(held.read("pay 12").as_deref(), Some("PAY-12"));
        assert_eq!(held.read("pay12").as_deref(), Some("PAY-12"));
        // `payo` is no project and `PAY-o12` no key.
        assert_eq!(held.read("payo12"), None);
    }

    #[test]
    fn a_project_with_digits_or_an_underscore_reads() {
        let held = projects(&["jira:K8S-1", "jira:MY_PROJ-1", "jira:AB-1", "jira:AB1-1"]);
        assert_eq!(held.read("k8s 4").as_deref(), Some("K8S-4"));
        assert_eq!(held.read("k8s4").as_deref(), Some("K8S-4"));
        assert_eq!(held.read("my_proj_12").as_deref(), Some("MY_PROJ-12"));
        assert_eq!(held.read("my_proj12").as_deref(), Some("MY_PROJ-12"));
        assert_eq!(held.read("my_proj 12").as_deref(), Some("MY_PROJ-12"));
        assert_eq!(
            held.read("ab1123").as_deref(),
            Some("AB1-123"),
            "the longer project"
        );
        assert_eq!(held.read("ab 1123").as_deref(), Some("AB-1123"));
    }

    #[test]
    fn the_word_filter_admits_the_key_beside_the_words() {
        use crate::query::WordAlternative;
        let alt = |holds: &[&str]| WordAlternative {
            holds: holds.iter().map(|h| (*h).to_owned()).collect(),
            lacks: Vec::new(),
        };
        let held = projects(&["jira:PAY-1"]);
        assert_eq!(
            alternatives("pay 123", &held),
            [alt(&["pay", "123"]), alt(&["PAY-123"])]
        );
        assert_eq!(
            alternatives("pay123", &held),
            [alt(&["pay123"]), alt(&["PAY-123"])]
        );
        assert_eq!(
            alternatives("sepa or pay_12", &held),
            [alt(&["sepa"]), alt(&["pay_12"]), alt(&["PAY-12"])],
            "each alternative once"
        );
        assert_eq!(alternatives("top 10", &held), [alt(&["top", "10"])]);
        assert_eq!(
            alternatives("pay 123", &Projects::default()),
            [alt(&["pay", "123"])]
        );
    }

    #[test]
    fn the_number_is_never_fuzzed() {
        let held = projects(&["jira:PAY-123"]);
        assert_eq!(held.read("pay 132").as_deref(), Some("PAY-132"));
    }
}
