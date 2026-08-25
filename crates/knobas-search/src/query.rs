//! The launcher's query grammar (spec §4), as one pure function.
//!
//! Everything the `⌘K` box understands is parsed **here, in the backend**
//! (ruling P2): the leading prefixes (`>` `?` `t ` `note:` `list:` `asset:`),
//! the token markers (`/source` `@person` `#ticket`) and the inline
//! `key:value` filters. One grammar, one implementation -- the frontend
//! renders the chips this returns instead of parsing the same string a second
//! time, and M4's saved searches parse through the same door.
//!
//! No sqlx in this module and no `async`: a grammar is a string function, and
//! keeping it one is what lets the whole table below be a unit test.
//!
//! # What the grammar does *not* do
//!
//! It never guesses. A `/alias` that resolves to nothing, an `updated:` it
//! cannot read, an `env:` that has no home until M4 -- each is reported in
//! [`ParsedQuery::unknown_tokens`] so the launcher can grey it out with a
//! reason. The alternative, demoting an unrecognised filter to a search word,
//! quietly changes the result set in a way nobody on screen can see.

use crate::types::{ParsedQuery, Prefix, SearchFilters};
use crate::vocab::Vocabulary;

/// How many unknown tokens are worth reporting.
///
/// They are display material, and an adversarial paste must not make the
/// response unbounded.
const MAX_UNKNOWN_TOKENS: usize = 8;

/// Leading prefixes, **longest marker first**.
///
/// The order is the invariant, not a tidiness: [`leading_prefix`] takes the
/// first row that matches, so a marker that is a prefix of a later one would
/// shadow it and leave the tail of the longer marker in the search text. No
/// pair in today's table is such a prefix, which is exactly why the ordering
/// has to be *checked* rather than assumed -- adding `asset` beside `asset:`
/// is a one-line change that would otherwise break `asset:` silently.
/// `the_leading_table_is_ordered_longest_first` is that check.
///
/// `t ` is not in here because it is a prefix only as a *whole word* -- see
/// [`leading_prefix`]. `#`, `@` and `/` are not in here either: they are token
/// markers that work anywhere in the query, so they live in [`apply_token`].
const LEADING: &[(&str, Prefix)] = &[
    ("assets:", Prefix::Asset),
    ("asset:", Prefix::Asset),
    ("note:", Prefix::Note),
    ("list:", Prefix::List),
    (">", Prefix::Action),
    ("?", Prefix::Help),
];

/// The result of parsing one raw box string.
///
/// [`Self::query`] is the part that goes back over the bridge (interfaces
/// §2.4). The other fields are facts the SQL layer needs and the frozen
/// contract has no home for.
#[derive(Debug, Clone)]
pub struct Parsed {
    /// The public echo: text, prefix, filters, unknown tokens.
    pub query: ParsedQuery,
    /// People named with `@name` / `author:name`.
    ///
    /// Always empty today: `SearchFilters` has only `mine`, so a named person
    /// has nowhere to land and is reported as an unknown token instead (open
    /// question **E-Q1**). The field exists because the merge and the query
    /// builder both already handle a list of authors -- granting E-Q1 is one
    /// additive field, not a new code path.
    pub authors: Vec<String>,
    /// Whether the last word is still being typed, and should therefore match
    /// as a prefix (`sep` finds `sepa`).
    pub prefix_last_term: bool,
    /// The smart list named by `list:<id>`, lowercased.
    pub list_id: Option<String>,
    /// The identity `@me` stands for, carried over from the vocabulary so
    /// [`merge`] can resolve `mine` -- including when `mine` came from a chip
    /// and the text never mentioned it.
    pub identity: Vec<String>,
}

/// The filters a query actually runs with, after the typed grammar and the
/// clicked chips have been reconciled by [`merge`].
///
/// Not part of the IPC contract: `SearchFilters` is what crosses the bridge,
/// this is what reaches the SQL builder. The difference is [`Self::authors`],
/// which is `mine` already resolved to usernames.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectiveFilters {
    pub sources: Vec<String>,
    pub kinds: Vec<String>,
    pub updated_within_days: Option<u32>,
    pub mine: bool,
    /// The usernames `mine` and any `@person` resolved to.
    ///
    /// Empty **with `mine` set** is a real state, not "no filter": it means
    /// no source was configured with a username, so knobas does not know who
    /// the user is. The builder emits an author predicate that matches nothing
    /// rather than dropping the filter -- see `sql::search_sql`.
    pub authors: Vec<String>,
}

/// Parse one raw launcher string against what this installation is configured
/// with.
#[must_use]
pub fn parse(raw: &str, vocab: &Vocabulary) -> Parsed {
    let mut parsed = Parsed::empty(vocab);
    let head = raw.trim_start();

    let rest = match leading_prefix(head) {
        Some((marker, prefix)) => {
            parsed.query.prefix = Some(prefix);
            &head[marker.len()..]
        }
        None => head,
    };

    // `note:` pins the corpus to notes and `asset:` to the estate. Both express
    // themselves as an ordinary kind filter, so nothing downstream needs a
    // mode: a corpus that does not exist yet simply has no rows of that kind
    // (interfaces §2.4 -- notes are M2, assets M4).
    match parsed.query.prefix {
        Some(Prefix::Note) => parsed.query.filters.kinds.push("note".to_owned()),
        Some(Prefix::Asset) => parsed.query.filters.kinds.push("asset".to_owned()),
        Some(Prefix::List) => {
            let id = rest.trim().to_lowercase();
            // An empty id is someone who has typed `list:` and nothing else,
            // not a request for the list called "".
            parsed.list_id = (!id.is_empty()).then_some(id);
            return parsed;
        }
        _ => {}
    }

    let mut terms: Vec<String> = Vec::new();
    for token in tokenize(rest) {
        apply_token(&token, vocab, &mut parsed, &mut terms);
    }
    parsed.query.text = terms.join(" ");

    // "Still being typed" means: the user has not pressed space, there is a
    // word to extend, and that word is not a closed phrase (`"sepa retry"` is
    // finished by its closing quote, and `'sepa' <-> 'retri':*` would be a
    // prefix match on a word the user already ended).
    parsed.prefix_last_term = !raw.ends_with(char::is_whitespace)
        && !parsed.query.text.is_empty()
        && terms.last().is_some_and(|term| !term.starts_with('"'));

    parsed
}

/// Reconcile the filters the user *typed* with the chips they *clicked*.
///
/// Dimension by dimension: the text wins wherever it spoke, and the chip holds
/// wherever it was silent. Typing `/ji` while a Gitea chip is up means the user
/// changed their mind about the source, not that they want both -- but it says
/// nothing about the kind chip, which stays.
///
/// `mine` is the one dimension that ORs instead: a chip and an `@me` mean the
/// same thing, and neither spelling can express "not mine", so there is no
/// disagreement to resolve.
#[must_use]
pub fn merge(parsed: &Parsed, chips: &SearchFilters) -> EffectiveFilters {
    let typed = &parsed.query.filters;
    let mine = typed.mine || chips.mine;

    let mut authors: Vec<String> = Vec::new();
    if mine {
        for name in &parsed.identity {
            push_unique(&mut authors, name);
        }
    }
    for name in &parsed.authors {
        push_unique(&mut authors, name);
    }

    EffectiveFilters {
        sources: pick(&typed.sources, &chips.sources),
        kinds: pick(&typed.kinds, &chips.kinds),
        updated_within_days: typed.updated_within_days.or(chips.updated_within_days),
        mine,
        authors,
    }
}

impl Parsed {
    fn empty(vocab: &Vocabulary) -> Self {
        Self {
            query: ParsedQuery {
                text: String::new(),
                prefix: None,
                filters: SearchFilters::default(),
                unknown_tokens: Vec::new(),
            },
            authors: Vec::new(),
            prefix_last_term: false,
            list_id: None,
            identity: vocab.identity.clone(),
        }
    }

    /// Record a token nobody could make sense of, up to the display cap.
    fn unknown(&mut self, token: &str) {
        if self.query.unknown_tokens.len() < MAX_UNKNOWN_TOKENS {
            self.query.unknown_tokens.push(token.to_owned());
        }
    }

    /// Set the prefix only if nothing has claimed it: the *first* marker in the
    /// box is the one that describes what the user is doing.
    fn claim_prefix(&mut self, prefix: Prefix) {
        if self.query.prefix.is_none() {
            self.query.prefix = Some(prefix);
        }
    }
}

fn pick(typed: &[String], chips: &[String]) -> Vec<String> {
    if typed.is_empty() {
        chips.to_vec()
    } else {
        typed.to_vec()
    }
}

/// Which leading prefix, if any, this query opens with, and the marker to
/// strip.
fn leading_prefix(head: &str) -> Option<(&'static str, Prefix)> {
    for (marker, prefix) in LEADING {
        if head.starts_with(marker) {
            return Some((marker, *prefix));
        }
    }
    // `t ` is the time prefix, and only as a whole word: `team` is a search
    // term. Checked against the *next character* rather than against `"t "` so
    // that a tab does not fall through.
    let mut chars = head.chars();
    if chars.next() == Some('t') && chars.next().is_none_or(char::is_whitespace) {
        return Some(("t", Prefix::Time));
    }
    None
}

/// Split on whitespace, but keep a double-quoted run together **with its
/// quotes** -- `websearch_to_tsquery` reads those quotes as a phrase, so
/// stripping them here would silently turn a phrase search into an AND.
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in input.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// One token of the query, applied to the parse in progress.
///
/// The arms are ordered: marker prefixes first, then `key:value`, then "it is
/// a search word". The last arm is deliberately the default -- pasting
/// `jira:PAY-231` must *search*, not be read as a filter on a key called
/// `jira`.
fn apply_token(token: &str, vocab: &Vocabulary, parsed: &mut Parsed, terms: &mut Vec<String>) {
    if let Some(alias) = token.strip_prefix('/') {
        parsed.claim_prefix(Prefix::Source);
        resolve_sources(parsed, vocab, alias, token);
        return;
    }
    if let Some(person) = token.strip_prefix('@') {
        parsed.claim_prefix(Prefix::Person);
        apply_person(parsed, person, token);
        return;
    }
    if let Some(rest) = token.strip_prefix('#') {
        parsed.claim_prefix(Prefix::Ticket);
        push_unique(&mut parsed.query.filters.kinds, "ticket");
        if !rest.is_empty() {
            terms.push(rest.to_owned());
        }
        return;
    }
    if let Some((key, value)) = split_key_value(token)
        && apply_key_value(&key, value, token, vocab, parsed)
    {
        return;
    }
    terms.push(token.to_owned());
}

/// `key:value`, if the token is one at all.
///
/// A token that merely *starts* with a colon is not one. Neither is a quoted
/// phrase, and that falls out rather than being special-cased: the quote is
/// kept on the token, so `"note: the colon"` yields the key `"note` -- with
/// the quote -- which matches no filter key and lands in the search text where
/// it belongs.
fn split_key_value(token: &str) -> Option<(String, &str)> {
    let (key, value) = token.split_once(':')?;
    if key.is_empty() {
        return None;
    }
    Some((key.to_lowercase(), value))
}

/// Apply one `key:value` token; `false` means "not a filter, treat it as text".
fn apply_key_value(
    key: &str,
    value: &str,
    token: &str,
    vocab: &Vocabulary,
    parsed: &mut Parsed,
) -> bool {
    match key {
        "source" | "src" | "in" => {
            parsed.claim_prefix(Prefix::Source);
            resolve_sources(parsed, vocab, value, token);
        }
        // `kind:` is unambiguous, so it is taken at face value: a kind nothing
        // emits simply matches nothing, which is a truthful empty result.
        "kind" => {
            if value.is_empty() {
                parsed.unknown(token);
            } else {
                push_unique(&mut parsed.query.filters.kinds, value);
            }
        }
        // `type:` is not unambiguous -- §4 gives it to the *estate* chips
        // (`type:hypervisor`), which are M4. So it is a kind filter only when
        // it names a kind an adapter actually declared, and an unknown token
        // otherwise. Reading `type:hypervisor` as a kind filter would return
        // an empty list that looks like an answer.
        "type" => {
            if vocab.kinds.is_declared(value) {
                push_unique(&mut parsed.query.filters.kinds, value);
            } else {
                parsed.unknown(token);
            }
        }
        "updated" => match duration_days(value) {
            Some(days) => parsed.query.filters.updated_within_days = Some(days),
            None => parsed.unknown(token),
        },
        "is" => {
            if value.eq_ignore_ascii_case("mine") {
                parsed.query.filters.mine = true;
            } else {
                parsed.unknown(token);
            }
        }
        "author" | "owner" | "by" => {
            parsed.claim_prefix(Prefix::Person);
            apply_person(parsed, value, token);
        }
        // The estate's own keys. They have no home before M4, and demoting
        // them to search words would quietly widen the result set.
        "env" | "health" => parsed.unknown(token),
        _ => return false,
    }
    true
}

/// `@me` is the identity filter; anybody else has nowhere to land (**E-Q1**).
fn apply_person(parsed: &mut Parsed, person: &str, token: &str) {
    if person.eq_ignore_ascii_case("me") {
        parsed.query.filters.mine = true;
    } else if !person.is_empty() {
        parsed.unknown(token);
    }
    // A bare `@` is someone who has just typed the marker. The prefix is
    // already claimed; reporting it as unknown would grey out a token that is
    // one keystroke from being valid.
}

fn resolve_sources(parsed: &mut Parsed, vocab: &Vocabulary, alias: &str, token: &str) {
    let resolved = vocab.resolve_source(alias);
    if resolved.is_empty() {
        // A bare `/` is a marker mid-typing, not a mistake.
        if !alias.trim().is_empty() {
            parsed.unknown(token);
        }
        return;
    }
    for id in resolved {
        push_unique(&mut parsed.query.filters.sources, &id);
    }
}

/// `updated:` values, as §4's chips spell them.
///
/// `checked_mul` rather than `*`: `updated:9999999999w` is one paste away, and
/// a wrapped day count is a filter that silently means something else.
fn duration_days(value: &str) -> Option<u32> {
    let value = value.to_lowercase();
    match value.as_str() {
        "today" => return Some(1),
        "yesterday" => return Some(2),
        "week" => return Some(7),
        _ => {}
    }
    for (suffix, per_unit) in [("d", 1_u32), ("w", 7), ("m", 30)] {
        if let Some(count) = value.strip_suffix(suffix)
            && !count.is_empty()
            && count.chars().all(|c| c.is_ascii_digit())
        {
            return count
                .parse::<u32>()
                .ok()
                .and_then(|n| n.checked_mul(per_unit));
        }
    }
    None
}

/// Append unless already present, preserving first-seen order.
///
/// Order matters beyond tidiness: it is the order the generated SQL binds its
/// arrays in, so a stable one keeps the query text -- and therefore the plan
/// cache and every test expectation -- deterministic.
fn push_unique(into: &mut Vec<String>, value: &str) {
    if !into.iter().any(|existing| existing == value) {
        into.push(value.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vocabulary {
        Vocabulary::fixture()
    }

    #[test]
    fn leading_prefixes_are_exclusive_and_consume_their_marker() {
        let v = fixture();
        for (raw, prefix, text) in [
            ("> start work", Some(Prefix::Action), "start work"),
            ("?", Some(Prefix::Help), ""),
            ("t ", Some(Prefix::Time), ""),
            ("t", Some(Prefix::Time), ""),
            ("t worklog", Some(Prefix::Time), "worklog"),
            ("note: sepa", Some(Prefix::Note), "sepa"),
            ("list:failing", Some(Prefix::List), ""),
            ("asset: pve-02", Some(Prefix::Asset), "pve-02"),
            ("assets: pve-02", Some(Prefix::Asset), "pve-02"),
            ("#PAY-231", Some(Prefix::Ticket), "PAY-231"),
            ("sepa retry", None, "sepa retry"),
        ] {
            let p = parse(raw, &v);
            assert_eq!(p.query.prefix, prefix, "{raw:?}");
            assert_eq!(p.query.text, text, "{raw:?}");
        }
        // `t` is a prefix only as a whole word -- "team" is a search term.
        assert_eq!(parse("team", &v).query.prefix, None);
        assert_eq!(parse("team", &v).query.text, "team");
        // `note:` pins the corpus to notes, `asset:` to the estate: both
        // express themselves as a kind filter, so the rest of the pipeline
        // needs no mode.
        assert_eq!(parse("note: x", &v).query.filters.kinds, ["note"]);
        assert_eq!(parse("asset: x", &v).query.filters.kinds, ["asset"]);
        assert_eq!(parse("#PAY-231", &v).query.filters.kinds, ["ticket"]);
        assert_eq!(
            parse("list:failing", &v).list_id.as_deref(),
            Some("failing")
        );
        assert_eq!(
            parse("list:FAILING", &v).list_id.as_deref(),
            Some("failing")
        );
        // Someone who has typed the marker and nothing else has not named a
        // list called "".
        assert_eq!(parse("list:", &v).list_id, None);
        assert_eq!(parse("list:", &v).query.prefix, Some(Prefix::List));
    }

    #[test]
    fn the_leading_table_is_ordered_longest_first() {
        // `leading_prefix` takes the first row that matches, so a shorter
        // marker sitting above a longer one it prefixes would shadow it.
        for pair in LEADING.windows(2) {
            assert!(
                pair[0].0.len() >= pair[1].0.len(),
                "{:?} sits above the longer {:?}",
                pair[0].0,
                pair[1].0
            );
        }
    }

    #[test]
    fn source_aliases_resolve_to_every_instance_of_that_adapter() {
        let v = fixture();
        // One alias, two instances: /ji means "the Jiras", not "the first
        // Jira".
        assert_eq!(
            parse("/ji sepa", &v).query.filters.sources,
            ["jira", "jira-eu"]
        );
        assert_eq!(parse("/ji sepa", &v).query.text, "sepa");
        assert_eq!(parse("/gitea", &v).query.filters.sources, ["gitea"]);
        assert_eq!(parse("/jira-eu", &v).query.filters.sources, ["jira-eu"]); // exact id wins
        assert_eq!(parse("/tc", &v).query.filters.sources, ["teamcity"]);
        assert_eq!(parse("source:gt x", &v).query.filters.sources, ["gitea"]);
        assert_eq!(parse("src:gt x", &v).query.filters.sources, ["gitea"]);
        assert_eq!(parse("in:gt x", &v).query.filters.sources, ["gitea"]);
        assert_eq!(parse("/ji /gt", &v).query.filters.sources.len(), 3);
        // The same source twice is one filter, not two binds.
        assert_eq!(parse("/jira /jira", &v).query.filters.sources, ["jira"]);
        // Unresolvable: reported, never silently dropped and never guessed.
        let p = parse("/nope x", &v);
        assert!(p.query.filters.sources.is_empty());
        assert_eq!(p.query.unknown_tokens, ["/nope"]);
        assert_eq!(p.query.text, "x");
        // A bare marker is mid-typing, not a mistake.
        let bare = parse("/", &v);
        assert_eq!(bare.query.prefix, Some(Prefix::Source));
        assert!(bare.query.unknown_tokens.is_empty());
    }

    #[test]
    fn inline_key_values_fill_the_filters_they_have_a_home_for() {
        let v = fixture();
        assert_eq!(parse("kind:pr sepa", &v).query.filters.kinds, ["pr"]);
        assert_eq!(parse("type:build", &v).query.filters.kinds, ["build"]);
        assert_eq!(
            parse("updated:today", &v).query.filters.updated_within_days,
            Some(1)
        );
        assert_eq!(
            parse("updated:yesterday", &v)
                .query
                .filters
                .updated_within_days,
            Some(2)
        );
        assert_eq!(
            parse("updated:week", &v).query.filters.updated_within_days,
            Some(7)
        );
        assert_eq!(
            parse("updated:7d", &v).query.filters.updated_within_days,
            Some(7)
        );
        assert_eq!(
            parse("updated:2w", &v).query.filters.updated_within_days,
            Some(14)
        );
        assert_eq!(
            parse("updated:2m", &v).query.filters.updated_within_days,
            Some(60)
        );
        assert!(parse("is:mine", &v).query.filters.mine);
        assert!(parse("@me", &v).query.filters.mine);
        assert_eq!(parse("@me", &v).query.prefix, Some(Prefix::Person));
        // The *first* marker describes what the user is doing; a later one
        // still filters but does not rename the mode.
        assert_eq!(parse("/ji @me", &v).query.prefix, Some(Prefix::Source));
        assert_eq!(parse("> /ji #x", &v).query.prefix, Some(Prefix::Action));

        // An unreadable duration is reported, not rounded to something.
        let bad = parse("updated:soonish", &v);
        assert_eq!(bad.query.filters.updated_within_days, None);
        assert_eq!(bad.query.unknown_tokens, ["updated:soonish"]);
        // And an arithmetic overflow is a bad duration, not a wrapped one.
        assert_eq!(
            parse("updated:4294967295w", &v)
                .query
                .filters
                .updated_within_days,
            None
        );

        // Keys that are understood but have no M1 home are reported as unknown
        // so the launcher can grey them out with a reason -- they are not
        // silently demoted to search words, which would quietly change the
        // result set.
        let p = parse("env:prod health:down type:hypervisor x", &v);
        assert_eq!(
            p.query.unknown_tokens,
            ["env:prod", "health:down", "type:hypervisor"]
        );
        assert!(p.query.filters.kinds.is_empty());
        assert_eq!(p.query.text, "x");
        // Anything else with a colon is just text: pasting an entity id must
        // search.
        let p = parse("jira:PAY-231", &v);
        assert!(p.query.unknown_tokens.is_empty());
        assert!(p.query.filters.sources.is_empty());
        assert_eq!(p.query.text, "jira:PAY-231");
    }

    /// A named person has no field in `SearchFilters` (**E-Q1**), so the
    /// honest answer is to say so rather than to filter by nothing.
    #[test]
    fn a_named_person_is_reported_rather_than_silently_ignored() {
        let v = fixture();
        for raw in ["@jonas", "author:jonas", "owner:jonas", "by:jonas"] {
            let p = parse(raw, &v);
            assert_eq!(p.query.prefix, Some(Prefix::Person), "{raw:?}");
            assert_eq!(p.query.unknown_tokens, [raw], "{raw:?}");
            assert!(p.authors.is_empty(), "{raw:?}");
            assert!(!p.query.filters.mine, "{raw:?}");
            assert!(p.query.text.is_empty(), "{raw:?}");
        }
    }

    #[test]
    fn websearch_operators_and_phrases_survive_into_the_text() {
        let v = fixture();
        let p = parse(r#"/ji "sepa retry" -timeout or backoff"#, &v);
        assert_eq!(p.query.text, r#""sepa retry" -timeout or backoff"#);
        assert_eq!(p.query.filters.sources, ["jira", "jira-eu"]);
        // A colon inside a phrase is prose, not a filter key.
        assert_eq!(parse(r#""note: the colon""#, &v).query.prefix, None);
        assert_eq!(
            parse(r#""note: the colon""#, &v).query.text,
            r#""note: the colon""#
        );
    }

    #[test]
    fn the_trailing_word_is_a_prefix_match_only_while_it_is_being_typed() {
        let v = fixture();
        assert!(parse("sep", &v).prefix_last_term); // still typing
        assert!(!parse("sep ", &v).prefix_last_term); // word finished
        assert!(!parse(r#""sepa retry""#, &v).prefix_last_term); // a closed phrase
        assert!(!parse("/ji", &v).prefix_last_term); // no search word at all
        assert!(!parse("", &v).prefix_last_term);
        assert!(parse("/ji sep", &v).prefix_last_term);
    }

    #[test]
    fn text_filters_win_over_chips_dimension_by_dimension() {
        let v = fixture();
        let chips = SearchFilters {
            sources: vec!["gitea".into()],
            kinds: vec!["pr".into()],
            updated_within_days: Some(30),
            mine: false,
        };
        let eff = merge(&parse("/ji sepa", &v), &chips);
        assert_eq!(eff.sources, ["jira", "jira-eu"]); // the text spoke: it decides
        assert_eq!(eff.kinds, ["pr"]); // the text was silent: the chip holds
        assert_eq!(eff.updated_within_days, Some(30));
        assert!(!eff.mine);
        assert!(eff.authors.is_empty());

        // The text speaking on a dimension the chip also speaks on replaces it
        // rather than unioning: changing your mind is the common case.
        let eff = merge(&parse("kind:build updated:1d", &v), &chips);
        assert_eq!(eff.kinds, ["build"]);
        assert_eq!(eff.updated_within_days, Some(1));

        // `mine` is the one dimension that ORs: a chip and an `@me` mean the
        // same thing, and neither can express "not mine".
        assert!(merge(&parse("@me", &v), &SearchFilters::default()).mine);
        assert_eq!(merge(&parse("@me", &v), &chips).authors, ["mara.lindqvist"]);
        // Including when only the chip said so -- which is why the identity
        // travels with the parse rather than only being read at `@me`.
        let mine_chip = SearchFilters {
            mine: true,
            ..SearchFilters::default()
        };
        let eff = merge(&parse("sepa", &v), &mine_chip);
        assert!(eff.mine);
        assert_eq!(eff.authors, ["mara.lindqvist"]);
    }

    /// Not knowing who the user is must not read as "no filter".
    #[test]
    fn mine_without_an_identity_resolves_to_no_authors_but_stays_set() {
        let mut v = Vocabulary::fixture();
        v.identity.clear();
        let eff = merge(&parse("@me", &v), &SearchFilters::default());
        assert!(eff.mine);
        assert!(eff.authors.is_empty());
    }

    #[test]
    fn unknown_tokens_are_capped_so_a_paste_cannot_grow_the_response() {
        let v = fixture();
        let raw = (0..40)
            .map(|n| format!("env:v{n}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            parse(&raw, &v).query.unknown_tokens.len(),
            MAX_UNKNOWN_TOKENS
        );
    }

    #[test]
    fn tokenizing_keeps_a_phrase_whole_and_its_quotes_on() {
        assert_eq!(tokenize(r#"a "b c" d"#), ["a", r#""b c""#, "d"]);
        // An unclosed quote swallows the rest rather than losing it.
        assert_eq!(tokenize(r#"a "b c"#), ["a", r#""b c"#]);
        assert!(tokenize("   ").is_empty());
    }
}
