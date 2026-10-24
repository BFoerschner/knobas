//! The launcher's query grammar (spec §4), as one pure function.
//!
//! Everything the `⌘K` box understands is parsed **here, in the backend**
//! (ruling P2): the leading prefixes (`>` `?` `t ` `note:` `list:` `asset:`),
//! the token markers (`/source` `@person` `#ticket`) and the inline
//! `key:value` filters. One grammar, one implementation -- the frontend
//! renders the chips this returns instead of parsing the same string a second
//! time, and saved searches parse through the same door.
//!
//! No sqlx in this module and no `async`: a grammar is a string function, and
//! keeping it one is what lets the whole table below be a unit test.
//!
//! # What the grammar does *not* do
//!
//! It never guesses. A `/alias` that resolves to nothing, an `updated:` it
//! cannot read, an `env:` or `health:` no ticket has given a home yet -- each
//! is reported, not honoured, in [`ParsedQuery::unknown_tokens`] so the
//! launcher can grey it out with a reason. So is a `-` on a key or marker with
//! no negation (`-kind:pr`, `-@jonas`): only the ticket keys negate. The alternative, demoting an unrecognised filter to a search word,
//! quietly changes the result set in a way nobody on screen can see.

use knobas_core::payload::Declarations;

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

/// The ticket keys (#811, R-3.14): one per field the Jira sync keeps in a
/// ticket's payload, each read through the adapter's declaration
/// (ADR-0007), never by a payload key named here. `type:` is not one: it
/// means the entity kind, so the issue type has its own key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FieldKey {
    /// `project:` -- the project's key or name.
    Project,
    /// `status:` -- the status name.
    Status,
    /// `issuetype:` -- the source's own type (`Bug`, `Story`).
    IssueType,
    /// `priority:` -- the priority name.
    Priority,
    /// `assignee:` -- the assignee's username; `@me` is the user.
    Assignee,
    /// `label:` -- one of the item's labels.
    Label,
    /// `sprint:current` -- in a sprint that is active. `current` is the one
    /// value.
    Sprint,
    /// `assigned:<window>` -- assigned to the user and changed within the
    /// window (`today`, `week`, `7d`, as `updated:` reads them).
    Assigned,
    /// `has:<signal>` (#815) -- the ticket carries that signal
    /// (`failing-build`, `blocker`, `open-pr`, `review-for-me`), as
    /// `knobas_core::signals` reads it for the cards.
    Has,
}

impl FieldKey {
    /// Every key, in the order the box offers them.
    pub const ALL: [FieldKey; 9] = [
        FieldKey::Project,
        FieldKey::Status,
        FieldKey::IssueType,
        FieldKey::Priority,
        FieldKey::Assignee,
        FieldKey::Label,
        FieldKey::Sprint,
        FieldKey::Assigned,
        FieldKey::Has,
    ];

    /// The key as the grammar spells it, without the colon.
    #[must_use]
    pub fn spelling(self) -> &'static str {
        match self {
            FieldKey::Project => "project",
            FieldKey::Status => "status",
            FieldKey::IssueType => "issuetype",
            FieldKey::Priority => "priority",
            FieldKey::Assignee => "assignee",
            FieldKey::Label => "label",
            FieldKey::Sprint => "sprint",
            FieldKey::Assigned => "assigned",
            FieldKey::Has => "has",
        }
    }

    fn from_spelling(key: &str) -> Option<Self> {
        // `space:` is the Docs search's word for a page's project (#860, spec
        // #808 *Docs*: "`space:` is a grammar alias"); a space is what a
        // Confluence page's declared project is (ADR-0010).
        if key == "space" {
            return Some(FieldKey::Project);
        }
        Self::ALL.into_iter().find(|k| k.spelling() == key)
    }
}

/// The value `assignee:` takes for the user, whoever the sources say that is.
pub(crate) const ME: &str = "@me";

/// One ticket-key condition: the field, whether it is negated, and the values
/// any of which it matches, lowercased.
///
/// In [`Parsed::fields`] the values are as typed (lowercased), `@me`
/// included; in [`EffectiveFilters::fields`] `@me` has been resolved to the
/// identity's usernames, and `assigned:`'s values are those usernames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldFilter {
    pub key: FieldKey,
    /// `-status:Done`: the row must not match.
    pub negated: bool,
    pub values: Vec<String>,
    /// `assigned:`'s window in days; `None` for every other key.
    pub within_days: Option<u32>,
    /// `has:`'s user (#815): in [`EffectiveFilters::fields`] the identity's
    /// usernames, lowercased, which `has:review-for-me` reads; empty for
    /// every other key and before [`merge`].
    pub me: Vec<String>,
}

/// The result of parsing one raw box string.
///
/// [`Self::query`] is the part that goes back over the bridge (interfaces
/// §2.4). The other fields are facts the SQL layer needs and the frozen
/// contract has no home for.
#[derive(Debug, Clone)]
pub struct Parsed {
    /// The public echo: text, prefix, filters, unknown tokens.
    pub query: ParsedQuery,
    /// Whether the last word is still being typed, and should therefore match
    /// as a prefix (`sep` finds `sepa`).
    pub prefix_last_term: bool,
    /// The smart list named by `list:<id>`, lowercased.
    pub list_id: Option<String>,
    /// The identity `@me` stands for, carried over from the vocabulary so
    /// [`merge`] can resolve `mine` -- including when `mine` came from a chip
    /// and the text never mentioned it.
    pub identity: Vec<String>,
    /// The `under:` values, each with the token it was typed as, in the order
    /// typed -- `under:` matches ignoring case (ADR-0018).
    ///
    /// Here and not on [`ParsedQuery`]'s filters: `SearchFilters` is the
    /// frozen wire shape the chips travel in, and `under:` has no chip -- it
    /// lives in the text, which is also how a saved list stores it. The token
    /// is kept because whether a value names any parent is a question for the
    /// mirror, asked after the parse, and the answer is reported in the
    /// spelling the reader typed.
    pub under: Vec<(String, String)>,
    /// The `reach:` values (#817), as typed, in the order typed: each a
    /// ticket key or id whose job-reach read the row must be in. Typed only,
    /// for `under:`'s reason.
    pub reach: Vec<String>,
    /// The `name:` globs (#868), as typed, in the order typed. Typed only,
    /// for `under:`'s reason.
    pub names: Vec<String>,
    /// What every source declares, carried over from the vocabulary for the
    /// same reason [`Self::identity`] is: [`merge`] hands it to the statement,
    /// which reads the parent each child declares.
    pub declarations: Declarations,
    /// The ticket-key conditions (#811), one per key and sign, in the order
    /// first typed. Here and not on [`ParsedQuery`]'s filters for `under:`'s
    /// reason: no chip carries them.
    pub fields: Vec<FieldFilter>,
}

/// The filters a query actually runs with, after the typed grammar and the
/// clicked chips have been reconciled by [`merge`].
///
/// Not part of the IPC contract: `SearchFilters` is what crosses the bridge,
/// this is what reaches the SQL builder. The difference is
/// [`Self::identity_authors`], which is `mine` already resolved to usernames
/// and which the bridge deliberately never carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectiveFilters {
    pub sources: Vec<String>,
    pub kinds: Vec<String>,
    pub updated_within_days: Option<u32>,
    pub mine: bool,
    /// The people the query **named** -- `@jonas`, `author:jonas`, or an
    /// author chip (ruling **E-Q1**).
    ///
    /// A filter in its own right, which is why [`Self::is_empty`] consults it:
    /// `@jonas` with no text is a query somebody asked, not a browse.
    pub named_authors: Vec<String>,
    /// The `under:` values, as typed (the statement lower-cases them, #1652), OR-ed: the item sits beneath a parent
    /// answering to one of them, or is that parent (ADR-0018).
    pub under: Vec<String>,
    /// The `reach:` values (#817), as typed, OR-ed: the row is something
    /// one of these tickets reaches, by `knobas_core::reach_walk!` -- the
    /// same walk the job-reach read makes.
    pub reach: Vec<String>,
    /// The `name:` globs (#868), as typed, OR-ed: the row's key or title
    /// matches one of them whole, `*` any run of characters and `?` one
    /// (`knobas_core::name_glob_matches!`).
    pub names: Vec<String>,
    /// The declarations every row's path line reads the parent through
    /// (#596). Carried whether or not [`Self::under`] is set, since every hit
    /// draws its path. The `under:` predicate read them too until #627; it
    /// now reads `sync.item_parent`, which the database keeps from the stored
    /// copy of the same declarations (migration `0028`).
    pub declarations: Declarations,
    /// The usernames **`mine`** resolved to, and nothing else.
    ///
    /// Empty **with `mine` set** is a real state, not "no filter": it means
    /// no source was configured with a username, so knobas does not know who
    /// the user is. The builder emits an author predicate that matches nothing
    /// rather than dropping the filter -- see `sql::search_sql`.
    pub identity_authors: Vec<String>,
    /// The ticket-key conditions (#811), `@me` resolved: every one must hold.
    pub fields: Vec<FieldFilter>,
}

impl EffectiveFilters {
    /// Everything the author predicate binds: the named people, plus whoever
    /// `mine` stands for.
    ///
    /// One list because the predicate is one `= any(...)`: `@me @jonas` is
    /// "mine or jonas's", the same OR every other multi-valued dimension uses.
    #[must_use]
    pub fn authors(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for name in self.identity_authors.iter().chain(&self.named_authors) {
            push_unique(&mut out, name);
        }
        out
    }

    /// Whether this query put anything into the author predicate.
    ///
    /// **Reads `mine` and the *named* people, never [`Self::identity_authors`]**,
    /// and that is the load-bearing part: the resolved list is empty in exactly
    /// the state the predicate has to survive -- an installation where no source
    /// was configured with a username -- and a query that asks "mine" there is
    /// still an author query.
    ///
    /// One spelling, two readers, and they must not drift: `sql::search_sql`
    /// binds the predicate on it, and `coverage::author_coverage` decides
    /// whether there is anything to report on it (issue #141). Two inline
    /// copies would let coverage go on explaining a filter the statement had
    /// stopped applying.
    #[must_use]
    pub fn filters_by_author(&self) -> bool {
        self.mine || !self.named_authors.is_empty()
    }

    /// Whether this query scoped by a parent. One spelling for the statement
    /// and for coverage, for [`Self::filters_by_author`]'s reason.
    #[must_use]
    pub fn filters_by_under(&self) -> bool {
        !self.under.is_empty()
    }

    /// Whether this query scoped by what a ticket reaches (#817).
    #[must_use]
    pub fn filters_by_reach(&self) -> bool {
        !self.reach.is_empty()
    }

    /// Whether anything is actually being filtered on.
    ///
    /// The engine's reason for asking: a query with neither text nor a filter
    /// is a *browse of the whole corpus*, which the builder will happily
    /// generate and which counts every row in the mirror. That answer belongs
    /// to the launcher board, not to a SQL statement.
    ///
    /// [`Self::identity_authors`] is deliberately not consulted, and cannot
    /// be: it is `mine` already resolved, so it is empty exactly when knobas
    /// does not know who the user is -- and reading that as "no filter" would
    /// turn `@me` on an unconfigured install into an unfiltered scan.
    /// [`Self::named_authors`] is the opposite case and *is* consulted: it is
    /// empty exactly when nobody was named.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
            && self.kinds.is_empty()
            && self.updated_within_days.is_none()
            && !self.mine
            && self.named_authors.is_empty()
            && self.under.is_empty()
            && self.reach.is_empty()
            && self.names.is_empty()
            && self.fields.is_empty()
    }

    /// What the launcher draws its chips from.
    ///
    /// The *merged* filters, not the typed ones: a chip the user clicked is
    /// part of what the query ran with, and echoing only the typed half would
    /// have the UI redraw a narrower query than the one it got results for.
    /// [`Self::identity_authors`] stays behind -- it is `mine` resolved to
    /// usernames, and echoing it would put names the user never typed into the
    /// chip row, where the `@me` chip already says the same thing.
    #[must_use]
    pub fn echo(&self) -> SearchFilters {
        SearchFilters {
            sources: self.sources.clone(),
            kinds: self.kinds.clone(),
            updated_within_days: self.updated_within_days,
            mine: self.mine,
            authors: self.named_authors.clone(),
        }
    }
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
    // prefix match on a word the user already ended). A negated phrase
    // (`-"retry batch"`) is just as closed: its tsquery text is
    // `!( 'retri' <-> 'batch' )`, and `:*` after the parenthesis is a syntax
    // error (#1648). A bare `-word` is not closed: its text ends in a quoted
    // lexeme, so it stays a prefix. This reads only the last *typed* term, so
    // it is the cheap first check: the statement guards the cast again on the
    // tsquery text itself, which catches a negated phrase followed by a word
    // Postgres drops (`-"retry batch" the`, #1971).
    parsed.prefix_last_term = !raw.ends_with(char::is_whitespace)
        && !parsed.query.text.is_empty()
        && terms
            .last()
            .is_some_and(|term| !term.starts_with('"') && !term.starts_with("-\""));

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

    EffectiveFilters {
        sources: pick(&typed.sources, &chips.sources),
        kinds: pick(&typed.kinds, &chips.kinds),
        updated_within_days: typed.updated_within_days.or(chips.updated_within_days),
        mine,
        named_authors: pick(&typed.authors, &chips.authors),
        // Typed only: there is no `under:` chip to reconcile with.
        under: parsed
            .under
            .iter()
            .map(|(_, value)| value.clone())
            .collect(),
        // Typed only, like `under:`.
        reach: parsed.reach.clone(),
        // Typed only, like `under:`.
        names: parsed.names.clone(),
        // Always: every row's path line reads the parent its kind declares
        // (#596), not only an `under:` predicate.
        declarations: parsed.declarations.clone(),
        // `mine` is what puts the identity in play, and only `mine`: a query
        // that named somebody else must not also quietly match the user.
        identity_authors: if mine {
            parsed.identity.clone()
        } else {
            Vec::new()
        },
        // Typed only, like `under:`. `@me` becomes whoever the identity
        // names, as typed like every name value here (the statement folds
        // them, #1843); with no identity it
        // becomes nobody, which matches nothing -- not "no filter".
        fields: parsed
            .fields
            .iter()
            .map(|filter| {
                let mut values = Vec::new();
                // `assigned:` is always the user's: its values are the identity.
                let me = [ME.to_owned()];
                let typed = if filter.key == FieldKey::Assigned {
                    &me[..]
                } else {
                    &filter.values[..]
                };
                for value in typed {
                    if value == ME {
                        for name in &parsed.identity {
                            push_unique_value(&mut values, name);
                        }
                    } else {
                        push_unique_value(&mut values, value);
                    }
                }
                FieldFilter {
                    values,
                    me: if filter.key == FieldKey::Has {
                        parsed
                            .identity
                            .iter()
                            .map(|name| name.to_lowercase())
                            .collect()
                    } else {
                        Vec::new()
                    },
                    ..filter.clone()
                }
            })
            .collect(),
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
            prefix_last_term: false,
            list_id: None,
            identity: vocab.identity.clone(),
            under: Vec::new(),
            reach: Vec::new(),
            names: Vec::new(),
            declarations: vocab.declarations.clone(),
            fields: Vec::new(),
        }
    }

    /// Add one ticket-key condition, joining the values of one already typed
    /// with the same key and sign: repeated values mean any of them, and two
    /// `assigned:` windows mean the wider.
    fn field(&mut self, key: FieldKey, negated: bool, value: Option<String>, days: Option<u32>) {
        let index = match self
            .fields
            .iter()
            .position(|f| f.key == key && f.negated == negated)
        {
            Some(index) => index,
            None => {
                self.fields.push(FieldFilter {
                    key,
                    negated,
                    values: Vec::new(),
                    within_days: None,
                    me: Vec::new(),
                });
                self.fields.len() - 1
            }
        };
        let filter = &mut self.fields[index];
        if let Some(value) = value {
            push_unique_value(&mut filter.values, &value);
        }
        if let Some(days) = days {
            filter.within_days = Some(filter.within_days.map_or(days, |d| d.max(days)));
        }
    }

    /// Record a token nobody could make sense of, up to the display cap.
    ///
    /// Crate-visible because one kind of token is judged after the parse:
    /// whether an `under:` value names any parent is the mirror's to say.
    pub(crate) fn unknown(&mut self, token: &str) {
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
        apply_person(parsed, person);
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
    if let Some((key, value)) = split_key_value(token.strip_prefix('-').unwrap_or(token))
        && let Some(field) = FieldKey::from_spelling(&key)
    {
        apply_field(field, token.starts_with('-'), value, token, parsed);
        return;
    }
    if let Some((key, value)) = split_key_value(token)
        && apply_key_value(&key, value, token, vocab, parsed)
    {
        return;
    }
    if negates_what_cannot_be_negated(token) {
        parsed.unknown(token);
        return;
    }
    terms.push(token.to_owned());
}

/// The keys [`apply_key_value`] reads, which have no negation (#1649). Kept
/// beside it and pinned to it by
/// `the_refused_keys_are_the_keys_apply_key_value_reads`.
const NEGATION_UNSUPPORTED_KEYS: [&str; 15] = [
    "source", "src", "in", "kind", "type", "updated", "is", "author", "owner", "by", "under",
    "reach", "name", "env", "health",
];

/// Whether a `-` token is the negation of something the grammar reads and has
/// no negation for: a marker (`-@x`, `-/x`, `-#x`) or a `key:value` whose key
/// [`apply_key_value`] knows. Websearch would take it for a search word and
/// negate the English word (`-kind:pr` is `!kind & pr`), which is the
/// demotion the module's rule rules out. `-word` and `-unknownkey:v` stay
/// websearch's own.
fn negates_what_cannot_be_negated(token: &str) -> bool {
    let Some(rest) = token.strip_prefix('-') else {
        return false;
    };
    if rest.starts_with(['@', '/', '#']) {
        return rest.len() > 1;
    }
    split_key_value(rest).is_some_and(|(key, _)| NEGATION_UNSUPPORTED_KEYS.contains(&key.as_str()))
}

/// The words and phrases a query's text asks for, as a reader marks them
/// (#860): each word, and each quoted phrase whole, lowercased and in the
/// order typed, once each. What it leaves out is left out (`-word`,
/// `-"a phrase"`), and so is `or`, which `websearch_to_tsquery` reads as the
/// operator. `text` is [`ParsedQuery::text`](crate::types::ParsedQuery), the
/// grammar's keys already taken out. These are the marks, not a filter: the
/// Tickets view filters by [`word_alternatives`].
#[must_use]
pub fn needles(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for token in tokenize(text) {
        if token.starts_with('-') || token.eq_ignore_ascii_case("or") {
            continue;
        }
        push_needle(&mut out, &token);
    }
    out
}

/// One alternative of the Tickets view's word filter (#1889): a row it
/// admits holds every word of `holds` and none of `lacks`. Each is a needle
/// as [`needles`] spells it, lowercased, once each.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct WordAlternative {
    pub holds: Vec<String>,
    pub lacks: Vec<String>,
}

/// The Tickets view's word filter (#1889): the alternatives a query's text
/// asks for, a row admitted when it meets one of them. `or` (any case)
/// separates them, and each binds its words tighter than `or`, as
/// `websearch_to_tsquery` reads the launcher's text: `a b or c -d` is
/// `(a & b) | (c & !d)`. A `-word` or `-"a phrase"` is left out of the
/// alternative it stands in. An `or` with nothing on one side adds no
/// alternative, and none at all is no word filter. `text` is
/// [`ParsedQuery::text`](crate::types::ParsedQuery), the keys taken out.
#[must_use]
pub fn word_alternatives(text: &str) -> Vec<WordAlternative> {
    let mut out: Vec<WordAlternative> = Vec::new();
    let mut current = WordAlternative::default();
    for token in tokenize(text) {
        if token.eq_ignore_ascii_case("or") {
            out.push(std::mem::take(&mut current));
        } else if let Some(left_out) = token.strip_prefix('-') {
            push_needle(&mut current.lacks, left_out);
        } else {
            push_needle(&mut current.holds, &token);
        }
    }
    out.push(current);
    out.retain(|alternative| !(alternative.holds.is_empty() && alternative.lacks.is_empty()));
    out
}

/// `token` as a needle -- its quotes and edge punctuation off, lowercased --
/// onto `out`, unless it is empty or there already.
fn push_needle(out: &mut Vec<String>, token: &str) {
    let needle = token
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    if !needle.is_empty() && !out.contains(&needle) {
        out.push(needle);
    }
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

/// Whether two typed `under:`, `reach:` or `name:` values are one value,
/// for the once-each rule only: the match itself is the statement's `lower()`.
fn same_value(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
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
        // Two spellings, one filter, and only one of them is ambiguous.
        //
        // `kind:` is taken at face value: a kind nothing emits simply matches
        // nothing, which is a truthful empty result rather than a guess.
        //
        // `type:` is ambiguous -- §4 gives it to the *estate* chips
        // (`type:hypervisor`), which are M4 -- so it is a kind filter only
        // when the catalog says the value names a kind. But the catalog can
        // only say that when it knows anything at all: an **empty** catalog is
        // not evidence that `build` is not a kind, it is knobas not yet having
        // been told what kinds exist (**E-Q2**; `KindCatalog::default()` is
        // what the product ships with until stream F's registry is wired). A
        // disambiguator with no information must not act as a rejecter, so
        // `type:` falls back to `kind:` while the catalog is empty -- which is
        // also what makes §4's own `type:build` work today.
        //
        // The consequence, recorded rather than hidden: while the catalog is
        // empty, `type:hypervisor` is a kind filter matching nothing instead
        // of a reported token. It starts being reported the moment any adapter
        // declares its kinds, which is well before an asset exists to filter.
        "kind" | "type" => {
            if value.is_empty() {
                parsed.unknown(token);
            } else if key == "kind" || vocab.kinds.is_empty() || vocab.kinds.is_known(value) {
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
            apply_person(parsed, value);
        }
        // A parent (ADR-0018). Kept as typed: it matches ignoring case, and
        // the statement lower-cases both sides (#1652), because Rust's
        // `to_lowercase` and the server's `lower()` disagree on a few code
        // points (`İ`, a final sigma). Repeated values OR. Whether a value
        // names any parent at all is not
        // the grammar's to know -- the parent lives in the children's records
        // -- so that is judged after the parse (`crate::under`). A bare
        // `under:` is somebody mid-typing, as a bare `author:` is.
        //
        // A name with a space in it -- a page title -- arrives quoted, because
        // the tokenizer keeps a quoted run together *with* its quotes, and the
        // name is what is inside them (#601: the box completes such a name
        // that way). Unlike the search text, no phrase is lost: a parent's
        // name is matched whole, never as words.
        "under" => {
            let value = unquote(value.trim()).trim().to_owned();
            if !value.is_empty()
                && !parsed
                    .under
                    .iter()
                    .any(|(_, seen)| same_value(seen, &value))
            {
                parsed.under.push((token.to_owned(), value));
            }
        }
        // What a ticket reaches (#817, R-2.9): the job-reach read as a
        // filter. The value is a ticket key (`PAY-231`) or id
        // (`jira:PAY-231`), matched ignoring case; repeated values OR. A key
        // nothing answers to reaches nothing, which is a true empty result.
        // A bare `reach:` is somebody mid-typing.
        "reach" => {
            let value = unquote(value.trim()).trim().to_owned();
            if !value.is_empty() && !parsed.reach.iter().any(|seen| same_value(seen, &value)) {
                parsed.reach.push(value);
            }
        }
        // A name glob (#868, spec #808 *Link sets*: "a name glob is added to
        // the grammar"): the row's key or title, whole, `*` any run and `?`
        // one character, ignoring case; repeated globs OR. Quoted when it
        // holds a space. A bare `name:` is somebody mid-typing.
        "name" => {
            let value = unquote(value.trim()).trim().to_owned();
            if !value.is_empty() && !parsed.names.iter().any(|seen| same_value(seen, &value)) {
                parsed.names.push(value);
            }
        }
        // The estate's own keys. No ticket has given them a home yet, and
        // demoting them to search words would quietly widen the result set.
        "env" | "health" => parsed.unknown(token),
        _ => return false,
    }
    true
}

/// Apply one ticket-key token (#811), `-` already read as `negated`.
///
/// The value is unquoted (`status:"In Review"`) and lowercased, since every
/// one of these matches ignoring case. An empty value is somebody mid-typing.
/// `sprint:` reads `current` and nothing else, and `assigned:` a window as
/// `updated:` reads one; any other value of theirs is reported.
fn apply_field(key: FieldKey, negated: bool, value: &str, token: &str, parsed: &mut Parsed) {
    let typed = unquote(value.trim()).trim().to_owned();
    if typed.is_empty() {
        return;
    }
    // The parse reads `sprint:`, `has:`, `assigned:` and `@me` folded; the
    // value a name key matches with travels as typed (#1843).
    let folded = typed.to_lowercase();
    match key {
        FieldKey::Sprint if folded != "current" => parsed.unknown(token),
        FieldKey::Has if knobas_core::signals::SignalKind::from_spelling(&folded).is_none() => {
            parsed.unknown(token);
        }
        FieldKey::Sprint | FieldKey::Has => parsed.field(key, negated, Some(folded), None),
        FieldKey::Assigned => match duration_days(&folded) {
            Some(days) => parsed.field(key, negated, None, Some(days)),
            None => parsed.unknown(token),
        },
        FieldKey::Assignee if folded == "me" || folded == ME => {
            parsed.field(key, negated, Some(ME.to_owned()), None);
        }
        _ => parsed.field(key, negated, Some(typed), None),
    }
}

/// The text inside one pair of double quotes around `value`, or `value` as it
/// is when it is not quoted on both ends.
fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
}

/// `@me` is the identity filter; anybody else is an author filter (**E-Q1**).
///
/// The name is kept **verbatim**, not lowercased: `sync.item.author` holds the
/// username as its source spells it, the predicate is an equality, and folding
/// the case here would only make the filter miss on any source that
/// distinguishes it.
///
/// The one person this cannot serve is somebody actually called `me`, and that
/// is the right trade: `@me` is the filter §4 puts on a chip.
fn apply_person(parsed: &mut Parsed, person: &str) {
    if person.eq_ignore_ascii_case("me") {
        parsed.query.filters.mine = true;
    } else if !person.is_empty() {
        push_unique(&mut parsed.query.filters.authors, person);
    }
    // A bare `@` -- or an `author:` with nothing after it -- is someone who
    // has just typed the marker. The prefix is already claimed; reporting it
    // as unknown would grey out a token that is one keystroke from being
    // valid.
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

/// [`push_unique`] for a typed name value: the same value in another case is
/// the same value, and the statement folds it (#1843).
fn push_unique_value(into: &mut Vec<String>, value: &str) {
    if !into.iter().any(|existing| same_value(existing, value)) {
        into.push(value.to_owned());
    }
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

/// What a link-set pattern matches (#868, R-7.2): a stored query read down
/// to the four keys a pattern may use. Each list ORs its values and the lists
/// AND; an empty list does not narrow. Every value is stored as typed; the
/// matcher folds it with `lower()` (#1843). Serialized as the `pattern` column of
/// `knobas.link_set_member` (migration `0040`), which
/// `knobas_core::link_set_pattern_hits!` matches.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Pattern {
    /// `source:` (or `/alias`): the item's source id.
    pub sources: Vec<String>,
    /// `project:` (or `space:`): the item's declared project, by key or name
    /// (ADR-0010).
    pub projects: Vec<String>,
    /// `under:`: a parent the item declares, at any depth, or the item itself
    /// as that parent (ADR-0018).
    pub under: Vec<String>,
    /// `name:`: a glob over the item's key or title.
    pub names: Vec<String>,
}

/// Read a parsed query as a link-set pattern, or say why it is not one.
///
/// A pattern is a filter that picks up new items as they appear, so it is
/// `source:`, `project:`, `under:` and `name:` and nothing else: no search
/// words (a ranking, not a filter), no `type:` (the member's kind says it), no
/// person, window, ticket key, `reach:` or negation, and no token the grammar
/// could not read. It must narrow by at least one of the four.
///
/// # Errors
///
/// The reason, in words a dialog can show, for anything else.
pub fn pattern_of(parsed: &Parsed) -> Result<Pattern, String> {
    let query = &parsed.query;
    let filters = &query.filters;
    if let Some(token) = query.unknown_tokens.first() {
        return Err(format!("`{token}` is not something a pattern can read"));
    }
    if !query.text.trim().is_empty() {
        return Err(format!(
            "a pattern filters and does not search: `{}` is not one of its keys",
            query.text.trim()
        ));
    }
    // `source:` and `/alias` claim `Prefix::Source` when they come first,
    // and they are a pattern's own key; every other prefix is foreign.
    let foreign = query.prefix.is_some_and(|prefix| prefix != Prefix::Source)
        || parsed.list_id.is_some()
        || !filters.kinds.is_empty()
        || filters.updated_within_days.is_some()
        || filters.mine
        || !filters.authors.is_empty()
        || !parsed.reach.is_empty()
        || parsed
            .fields
            .iter()
            .any(|field| field.key != FieldKey::Project || field.negated);
    if foreign {
        return Err(
            "a pattern reads source:, project:, under: and name: and nothing else".to_owned(),
        );
    }
    let pattern = Pattern {
        sources: filters.sources.clone(),
        projects: parsed
            .fields
            .iter()
            .flat_map(|field| field.values.iter().cloned())
            .collect(),
        under: parsed
            .under
            .iter()
            .map(|(_, value)| value.clone())
            .collect(),
        // A pattern is stored as typed: `knobas_core`'s matcher folds the
        // stored side with the server's `lower()` (#1843).
        names: parsed.names.to_vec(),
    };
    if pattern == Pattern::default() {
        return Err("a pattern needs a project, a parent or a name to match".to_owned());
    }
    Ok(pattern)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::KindCatalog;

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

    /// `type:` is disambiguated by the catalog -- but only once the catalog
    /// knows something.
    ///
    /// Every other `type:` assertion in this file runs against
    /// `Vocabulary::fixture()`, which declares five kinds. The **shipped**
    /// catalog is `KindCatalog::default()`, which declares none, and gating on
    /// an empty catalog would make every `type:X` an unknown token in the
    /// product -- including §4's own `type:build`. This pins both states, so
    /// the fixture cannot flatter the behaviour again.
    #[test]
    fn type_falls_back_to_kind_while_the_catalog_is_empty() {
        let populated = Vocabulary::fixture();
        let empty = Vocabulary {
            kinds: KindCatalog::default(),
            ..Vocabulary::fixture()
        };
        assert!(empty.kinds.is_empty() && !populated.kinds.is_empty());

        // What the product does today.
        assert_eq!(parse("type:build", &empty).query.filters.kinds, ["build"]);
        assert!(parse("type:build", &empty).query.unknown_tokens.is_empty());
        // ... and the price of it, stated out loud: with nothing declared, an
        // estate key is a kind filter that matches nothing.
        assert_eq!(
            parse("type:hypervisor", &empty).query.filters.kinds,
            ["hypervisor"]
        );

        // What it does once an adapter has declared its kinds.
        assert_eq!(
            parse("type:build", &populated).query.filters.kinds,
            ["build"]
        );
        assert!(
            parse("type:hypervisor", &populated)
                .query
                .filters
                .kinds
                .is_empty()
        );
        assert_eq!(
            parse("type:hypervisor", &populated).query.unknown_tokens,
            ["type:hypervisor"]
        );

        for vocab in [&empty, &populated] {
            // `kind:` is the unambiguous spelling and never consults the
            // catalog, in either state.
            assert_eq!(
                parse("kind:hypervisor", vocab).query.filters.kinds,
                ["hypervisor"]
            );
            // And neither spelling may filter on the empty string.
            for raw in ["kind:", "type:"] {
                let parsed = parse(raw, vocab);
                assert!(parsed.query.filters.kinds.is_empty(), "{raw:?}");
                assert_eq!(parsed.query.unknown_tokens, [raw], "{raw:?}");
            }
        }
    }

    /// A named person is a filter, in all four spellings (**E-Q1**, granted).
    ///
    /// The four spellings are one filter, and none of them is `mine`: `@me`
    /// resolves to whoever the sources are configured as, a name resolves to
    /// itself, and reading a name as `mine` would answer with the wrong
    /// person's work.
    #[test]
    fn a_named_person_is_an_author_filter_in_every_spelling() {
        let v = fixture();
        for raw in ["@jonas", "author:jonas", "owner:jonas", "by:jonas"] {
            let p = parse(raw, &v);
            assert_eq!(p.query.prefix, Some(Prefix::Person), "{raw:?}");
            assert!(p.query.unknown_tokens.is_empty(), "{raw:?}");
            assert_eq!(p.query.filters.authors, ["jonas"], "{raw:?}");
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

    /// #1648: a negated phrase typed last is closed (its tsquery text ends in
    /// `)`, which `:*` cannot follow); a bare negated word is not.
    #[test]
    fn a_negated_phrase_typed_last_is_not_a_prefix_but_a_negated_word_is() {
        let v = fixture();
        for raw in [
            r#"-"a b""#,
            r#"a -"b c""#,
            r#"sepa -"retry batch""#,
            r#""a b"c"#,
        ] {
            assert!(!parse(raw, &v).prefix_last_term, "{raw}");
        }
        for raw in ["-word", "sepa -word", "a or", "\u{fc}berweisung"] {
            let p = parse(raw, &v);
            assert!(p.prefix_last_term, "{raw}: {p:?}");
        }
    }

    #[test]
    fn text_filters_win_over_chips_dimension_by_dimension() {
        let v = fixture();
        let chips = SearchFilters {
            sources: vec!["gitea".into()],
            kinds: vec!["pr".into()],
            updated_within_days: Some(30),
            mine: false,
            authors: vec!["jonas".into()],
        };
        let eff = merge(&parse("/ji sepa", &v), &chips);
        assert_eq!(eff.sources, ["jira", "jira-eu"]); // the text spoke: it decides
        assert_eq!(eff.kinds, ["pr"]); // the text was silent: the chip holds
        assert_eq!(eff.updated_within_days, Some(30));
        assert!(!eff.mine);
        // Authors are a dimension like any other: the chip holds where the
        // text was silent, and `mine` stayed off, so nobody's identity joined.
        assert_eq!(eff.named_authors, ["jonas"]);
        assert_eq!(eff.authors(), ["jonas"]);

        // ... and the text replaces it where it spoke.
        let eff = merge(&parse("@mara", &v), &chips);
        assert_eq!(eff.named_authors, ["mara"]);

        // The text speaking on a dimension the chip also speaks on replaces it
        // rather than unioning: changing your mind is the common case.
        let eff = merge(&parse("kind:build updated:1d", &v), &chips);
        assert_eq!(eff.kinds, ["build"]);
        assert_eq!(eff.updated_within_days, Some(1));

        // `mine` is the one dimension that ORs: a chip and an `@me` mean the
        // same thing, and neither can express "not mine".
        assert!(merge(&parse("@me", &v), &SearchFilters::default()).mine);
        assert_eq!(
            merge(&parse("@me", &v), &SearchFilters::default()).authors(),
            ["mara.lindqvist"]
        );
        // Including when only the chip said so -- which is why the identity
        // travels with the parse rather than only being read at `@me`.
        let mine_chip = SearchFilters {
            mine: true,
            ..SearchFilters::default()
        };
        let eff = merge(&parse("sepa", &v), &mine_chip);
        assert!(eff.mine);
        assert_eq!(eff.authors(), ["mara.lindqvist"]);
    }

    /// `@me` and a name are two dimensions that both reach the same predicate.
    ///
    /// The predicate is one `= any(...)`, so "mine or jonas's" is the only
    /// reading -- but the two halves have to stay distinguishable up to that
    /// point, because only one of them is something the user typed and only
    /// one of them may reach the chip row.
    #[test]
    fn me_and_a_name_are_both_bound_but_only_the_name_is_echoed() {
        let v = fixture();
        let eff = merge(&parse("@me @jonas", &v), &SearchFilters::default());
        assert!(eff.mine);
        assert_eq!(eff.named_authors, ["jonas"]);
        assert_eq!(eff.identity_authors, ["mara.lindqvist"]);
        assert_eq!(eff.authors(), ["mara.lindqvist", "jonas"]);
        assert_eq!(eff.echo().authors, ["jonas"]);
        assert!(eff.echo().mine);

        // A name on its own leaves the identity out entirely: answering with
        // the user's own work as well would be the wrong person's results.
        let eff = merge(&parse("@jonas", &v), &SearchFilters::default());
        assert!(!eff.mine);
        assert!(eff.identity_authors.is_empty());
        assert_eq!(eff.authors(), ["jonas"]);

        // Two names are two binds; the same name twice is one.
        assert_eq!(
            merge(&parse("@jonas author:mara", &v), &SearchFilters::default()).authors(),
            ["jonas", "mara"]
        );
        assert_eq!(
            merge(&parse("@jonas by:jonas", &v), &SearchFilters::default()).authors(),
            ["jonas"]
        );
        // And `@me` twice over -- once typed, once resolved -- is still one.
        let mut same = Vocabulary::fixture();
        same.identity = vec!["jonas".to_owned()];
        assert_eq!(
            merge(&parse("@me @jonas", &same), &SearchFilters::default()).authors(),
            ["jonas"]
        );
    }

    /// Not knowing who the user is must not read as "no filter".
    #[test]
    fn mine_without_an_identity_resolves_to_no_authors_but_stays_set() {
        let mut v = Vocabulary::fixture();
        v.identity.clear();
        let eff = merge(&parse("@me", &v), &SearchFilters::default());
        assert!(eff.mine);
        assert!(eff.authors().is_empty());
        assert!(
            !eff.is_empty(),
            "`mine` is a filter even with nobody behind it"
        );
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

    /// The chips the launcher redraws are the *merged* filters, so every
    /// dimension the query actually ran with has to survive the echo. A
    /// dimension dropped here is a chip row that disagrees with the results
    /// under it, which is a lie nobody on screen can see.
    #[test]
    fn the_echo_carries_every_dimension_the_query_ran_with() {
        let filters = EffectiveFilters {
            sources: vec!["jira".to_owned()],
            kinds: vec!["ticket".to_owned()],
            updated_within_days: Some(7),
            mine: true,
            named_authors: vec!["jonas".to_owned()],
            // `mine` resolved. The `@me` chip already says this, and echoing
            // the usernames behind it would put names the user never typed
            // into the chip row.
            identity_authors: vec!["mara.lindqvist".to_owned()],
            // No chip carries a parent: `under:` lives in the text.
            ..EffectiveFilters::default()
        };
        let echo = filters.echo();
        assert_eq!(echo.sources, ["jira"]);
        assert_eq!(echo.kinds, ["ticket"]);
        assert_eq!(echo.updated_within_days, Some(7));
        assert!(echo.mine);
        assert_eq!(echo.authors, ["jonas"]);
        assert!(!echo.is_empty());
        assert_eq!(EffectiveFilters::default().echo(), SearchFilters::default());
    }

    /// Every dimension counts as a filter on its own -- the engine asks this to
    /// decide whether a text-free query is a browse the user asked for or the
    /// unbounded scan that belongs to the board instead.
    #[test]
    fn each_dimension_alone_makes_a_query_filtered() {
        assert!(EffectiveFilters::default().is_empty());
        let cases = [
            EffectiveFilters {
                sources: vec!["jira".to_owned()],
                ..EffectiveFilters::default()
            },
            EffectiveFilters {
                kinds: vec!["ticket".to_owned()],
                ..EffectiveFilters::default()
            },
            EffectiveFilters {
                updated_within_days: Some(1),
                ..EffectiveFilters::default()
            },
            // `mine` with nothing resolved is still a filter: knobas not
            // knowing who the user is must not widen the query to everything.
            EffectiveFilters {
                mine: true,
                ..EffectiveFilters::default()
            },
            // And a named person is a filter on its own, or `@jonas` with no
            // search word would be answered with the board instead of jonas's
            // work (`Searcher::search` refuses a query that filters nothing).
            EffectiveFilters {
                named_authors: vec!["jonas".to_owned()],
                ..EffectiveFilters::default()
            },
            // `under:payout-service` alone is "everything in that repo".
            EffectiveFilters {
                under: vec!["payout-service".to_owned()],
                ..EffectiveFilters::default()
            },
            // `reach:PAY-231` alone is "everything that job shows" (#817).
            EffectiveFilters {
                reach: vec!["pay-231".to_owned()],
                ..EffectiveFilters::default()
            },
        ];
        for filters in cases {
            assert!(!filters.is_empty(), "{filters:?}");
        }
    }

    /// `under:` is a filter, not a search word: kept as typed (the statement folds case), repeated values
    /// collected once each in the order typed, the token kept for the report,
    /// and a bare `under:` -- somebody mid-typing -- neither a filter nor an
    /// unknown token. No chip carries it, so the echo does not either.
    #[test]
    fn under_is_a_typed_filter_that_ignores_case() {
        let v = fixture();
        let p = parse(
            "under:Tidewater/Payout-Service jitter UNDER:ledger under:ledger",
            &v,
        );
        assert_eq!(p.query.text, "jitter");
        assert_eq!(
            p.under,
            [
                (
                    "under:Tidewater/Payout-Service".to_owned(),
                    "Tidewater/Payout-Service".to_owned()
                ),
                ("UNDER:ledger".to_owned(), "ledger".to_owned()),
            ]
        );
        assert!(p.query.unknown_tokens.is_empty());
        let merged = merge(&p, &SearchFilters::default());
        assert_eq!(merged.under, ["Tidewater/Payout-Service", "ledger"]);
        assert!(merged.filters_by_under());
        assert_eq!(merged.echo(), SearchFilters::default());

        // Not folded here: Rust's lower-casing of `İ` is not the server's, so
        // the typed spelling reaches the statement (#1652).
        let p = parse(r#"under:"İstanbul Rollout" reach:İ-1 name:ΌΔΟΣ*"#, &v);
        assert_eq!(p.under[0].1, "İstanbul Rollout");
        assert_eq!(p.reach, ["İ-1"]);
        assert_eq!(p.names, ["ΌΔΟΣ*"]);

        let typing = parse("under:", &v);
        assert!(typing.under.is_empty());
        assert!(typing.query.unknown_tokens.is_empty());
        assert!(typing.query.text.is_empty());
    }

    /// `reach:` (#817) is a typed filter like `under:`: lowercased, each
    /// value once, a bare one neither a filter nor an unknown token, and no
    /// chip to echo.
    #[test]
    fn reach_is_a_typed_filter_that_ignores_case() {
        let v = fixture();
        let p = parse("reach:PAY-231 retry REACH:jira:OPS-77 reach:pay-231", &v);
        assert_eq!(p.query.text, "retry");
        assert_eq!(p.reach, ["PAY-231", "jira:OPS-77"]);
        assert!(p.query.unknown_tokens.is_empty());
        let merged = merge(&p, &SearchFilters::default());
        assert_eq!(merged.reach, ["PAY-231", "jira:OPS-77"]);
        assert!(merged.filters_by_reach());
        assert_eq!(merged.echo(), SearchFilters::default());

        let typing = parse("reach:", &v);
        assert!(typing.reach.is_empty());
        assert!(typing.query.unknown_tokens.is_empty());
    }

    /// `name:` (#868) is a typed filter like `reach:`: a glob, lowercased,
    /// each once, quoted when it holds a space; a bare one is mid-typing.
    #[test]
    fn name_is_a_typed_glob_that_ignores_case() {
        let v = fixture();
        let p = parse(
            r#"name:Payout_* retry NAME:"Dash board?" name:payout_*"#,
            &v,
        );
        assert_eq!(p.query.text, "retry");
        assert_eq!(p.names, ["Payout_*", "Dash board?"]);
        assert!(p.query.unknown_tokens.is_empty());
        let merged = merge(&p, &SearchFilters::default());
        assert_eq!(merged.names, ["Payout_*", "Dash board?"]);
        assert!(!merged.is_empty());
        assert_eq!(merged.echo(), SearchFilters::default());

        let typing = parse("name:", &v);
        assert!(typing.names.is_empty());
        assert!(typing.query.unknown_tokens.is_empty());
    }

    /// A link-set pattern (#868, R-7.2) reads `source:`, `project:`,
    /// `under:` and `name:` and nothing else: each list ORs, the lists AND.
    #[test]
    fn a_pattern_reads_four_keys_and_refuses_the_rest() {
        let v = fixture();
        let read = |raw: &str| pattern_of(&parse(raw, &v));
        assert_eq!(
            read(r#"project:"Payouts" name:Payout_* name:PspClient_*"#),
            Ok(Pattern {
                sources: vec![],
                projects: vec!["Payouts".to_owned()],
                under: vec![],
                names: vec!["Payout_*".to_owned(), "PspClient_*".to_owned()],
            })
        );
        assert_eq!(
            read(r#"project:OPS under:"Runbooks""#).map(|p| (p.projects, p.under)),
            Ok((vec!["OPS".to_owned()], vec!["Runbooks".to_owned()]))
        );
        // `source:` and `/alias` claim the source prefix; they are still a
        // pattern's key.
        for raw in ["source:teamcity name:Payout_*", "/teamcity name:Payout_*"] {
            assert_eq!(
                read(raw).map(|p| (p.sources, p.names)),
                Ok((vec!["teamcity".to_owned()], vec!["Payout_*".to_owned()])),
                "{raw:?}"
            );
        }
        for refused in [
            "",
            "payout",
            "name:x status:done",
            "name:x -project:ops",
            "name:x @me",
            "name:x updated:7d",
            "name:x reach:PAY-1",
            "name:x type:ticket",
            "name:x bogus:1",
        ] {
            assert!(read(refused).is_err(), "{refused:?} read as a pattern");
        }
    }

    fn field(key: FieldKey, negated: bool, values: &[&str]) -> FieldFilter {
        FieldFilter {
            key,
            negated,
            values: values.iter().map(|v| (*v).to_owned()).collect(),
            within_days: None,
            me: Vec::new(),
        }
    }

    /// Each of the eight new keys (#811) is a filter on its own field: its
    /// value as typed (the statement ignores case), a quoted value read inside
    /// its quotes, never a search word and never a kind.
    #[test]
    fn each_ticket_key_is_a_filter_on_its_own_field() {
        let v = fixture();
        for (raw, key, value) in [
            ("project:PAY", FieldKey::Project, "PAY"),
            ("status:Done", FieldKey::Status, "Done"),
            (r#"status:"In Review""#, FieldKey::Status, "In Review"),
            ("issuetype:Bug", FieldKey::IssueType, "Bug"),
            ("priority:High", FieldKey::Priority, "High"),
            ("assignee:jonas.becker", FieldKey::Assignee, "jonas.becker"),
            ("label:sepa", FieldKey::Label, "sepa"),
            ("sprint:current", FieldKey::Sprint, "current"),
            ("SPRINT:Current", FieldKey::Sprint, "current"),
        ] {
            let p = parse(raw, &v);
            assert_eq!(p.fields, [field(key, false, &[value])], "{raw:?}");
            assert!(p.query.text.is_empty(), "{raw:?}");
            assert!(p.query.unknown_tokens.is_empty(), "{raw:?}");
            assert!(p.query.filters.kinds.is_empty(), "{raw:?}");
        }
        // `type:` still means the entity kind, and `issuetype:` never does.
        assert_eq!(parse("type:ticket", &v).query.filters.kinds, ["ticket"]);
        assert!(parse("type:ticket", &v).fields.is_empty());
    }

    /// `space:` is the Docs search's alias of `project:` (#860): a page's
    /// space is its declared project.
    #[test]
    fn space_is_an_alias_of_project() {
        let v = fixture();
        assert_eq!(
            parse("space:ENG", &v).fields,
            parse("project:ENG", &v).fields
        );
        assert_eq!(
            parse("-space:\"Ops Team\"", &v).fields,
            [field(FieldKey::Project, true, &["Ops Team"])]
        );
        assert!(parse("space:ENG retry", &v).query.unknown_tokens.is_empty());
        assert_eq!(parse("space:ENG retry", &v).query.text, "retry");
    }

    /// What a reader marks: the words and phrases asked for, never one left
    /// out, never `or`.
    #[test]
    fn needles_are_what_the_text_asks_for() {
        assert_eq!(
            needles(r#"Retry "backoff policy" -cron -"dead letter" OR sepa, retry"#),
            ["retry", "backoff policy", "sepa"]
        );
        assert!(needles("").is_empty());
        assert!(needles("-only").is_empty());
    }

    /// The Tickets page's `needles.ts` is a second copy of this grammar. Both
    /// read `tests/fixtures/needle-parity.json` (typed text, its needles and
    /// alternatives); `needles.test.ts` is the other half. A grammar change
    /// here that moves a row fails the other side until the fixture and the
    /// copy agree.
    #[test]
    fn needles_and_alternatives_match_the_shared_parity_fixture() {
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../tests/fixtures/needle-parity.json"))
                .expect("fixture is JSON");
        assert!(rows.len() > 10);
        let v = fixture();
        let words = |value: &serde_json::Value| -> Vec<String> {
            value
                .as_array()
                .expect("array")
                .iter()
                .map(|w| w.as_str().expect("string").to_owned())
                .collect()
        };
        for row in rows {
            let typed = row["typed"].as_str().expect("typed");
            let text = parse(typed, &v).query.text;
            assert_eq!(
                needles(&text),
                words(&row["needles"]),
                "needles of {typed:?}"
            );
            let expected: Vec<(Vec<String>, Vec<String>)> = row["alternatives"]
                .as_array()
                .expect("alternatives")
                .iter()
                .map(|a| (words(&a["holds"]), words(&a["lacks"])))
                .collect();
            let got: Vec<(Vec<String>, Vec<String>)> = word_alternatives(&text)
                .into_iter()
                .map(|a| (a.holds, a.lacks))
                .collect();
            assert_eq!(got, expected, "alternatives of {typed:?}");
        }
    }

    /// The view's word filter (#1889): `or` splits the alternatives, and a
    /// `-word` or `-"phrase"` is left out of the one it stands in.
    #[test]
    fn word_alternatives_split_on_or_and_record_what_is_left_out() {
        let alt = |holds: &[&str], lacks: &[&str]| WordAlternative {
            holds: holds.iter().map(|w| (*w).to_owned()).collect(),
            lacks: lacks.iter().map(|w| (*w).to_owned()).collect(),
        };
        assert_eq!(
            word_alternatives(r#"Retry "backoff policy" -cron -"dead letter" OR sepa, retry -"#),
            [
                alt(&["retry", "backoff policy"], &["cron", "dead letter"]),
                alt(&["sepa", "retry"], &[]),
            ]
        );
        assert_eq!(word_alternatives("-only"), [alt(&[], &["only"])]);
        assert_eq!(
            word_alternatives("or a or or -b or"),
            [alt(&["a"], &[]), alt(&[], &["b"])]
        );
        assert!(word_alternatives("").is_empty());
        assert!(word_alternatives("or - \"\"").is_empty());
    }

    /// A minus on a key or marker the grammar knows and cannot negate is
    /// reported, never demoted to a search word: `-kind:pr sepa` as text is
    /// `!kind & pr & sepa`, a result set narrowed on English words nobody can
    /// see (the module's "never demotes" rule; #1649).
    #[test]
    fn a_minus_on_a_key_that_cannot_be_negated_is_an_unknown_token() {
        let v = fixture();
        let p = parse("-kind:pr sepa", &v).query;
        assert_eq!(p.text, "sepa");
        assert_eq!(p.unknown_tokens, ["-kind:pr"]);
        assert!(p.filters.kinds.is_empty());

        // Every key `apply_key_value` reads, not only the one named.
        for key in NEGATION_UNSUPPORTED_KEYS {
            let token = format!("-{key}:x");
            let p = parse(&format!("{token} sepa"), &v).query;
            assert_eq!(p.text, "sepa", "{token}");
            assert_eq!(p.unknown_tokens, [token.as_str()], "{token}");
        }
        for token in ["-@jonas", "-/gitea", "-#PAY-231"] {
            let p = parse(&format!("{token} sepa"), &v).query;
            assert_eq!(p.text, "sepa", "{token}");
            assert_eq!(p.unknown_tokens, [token], "{token}");
            assert_eq!(p.prefix, None, "{token}");
        }
        // The un-negated spelling is still read.
        assert_eq!(parse("kind:pr", &v).query.filters.kinds, ["pr"]);
    }

    /// The list of keys the minus is refused on is every key
    /// `apply_key_value` reads: a key added there and not here would be
    /// demoted again.
    #[test]
    fn the_refused_keys_are_the_keys_apply_key_value_reads() {
        let v = fixture();
        for key in NEGATION_UNSUPPORTED_KEYS {
            let mut parsed = Parsed::empty(&v);
            assert!(
                apply_key_value(key, "x", &format!("{key}:x"), &v, &mut parsed),
                "{key}"
            );
        }
        let mut parsed = Parsed::empty(&v);
        assert!(!apply_key_value("jira", "x", "jira:x", &v, &mut parsed));
    }

    /// Repeated values of one key mean any of them; the same value twice is
    /// one; a negated key is a filter of its own, and so is its repetition.
    #[test]
    fn repeated_values_mean_any_and_a_minus_negates() {
        let v = fixture();
        let p = parse(
            "status:Done status:\"In Review\" STATUS:done -status:Blocked -status:\"On Hold\" sepa",
            &v,
        );
        assert_eq!(
            p.fields,
            [
                field(FieldKey::Status, false, &["Done", "In Review"]),
                field(FieldKey::Status, true, &["Blocked", "On Hold"]),
            ]
        );
        assert_eq!(p.query.text, "sepa");
        // A minus on a word is still websearch's own negation, and a minus on
        // a key the grammar does not know stays a word too.
        assert_eq!(parse("-timeout", &v).query.text, "-timeout");
        assert_eq!(parse("-jira:PAY-231", &v).query.text, "-jira:PAY-231");
        assert_eq!(
            parse("-label:tech-debt -assignee:@me", &v).fields,
            [
                field(FieldKey::Label, true, &["tech-debt"]),
                field(FieldKey::Assignee, true, &["@me"]),
            ]
        );
    }

    /// `assignee:@me` (and `assignee:me`) is the user, resolved by `merge`
    /// to every username the identity holds, lowercased; anybody else is
    /// matched by name. With no identity it resolves to nobody, which
    /// matches nothing, as `@me` does.
    #[test]
    fn assignee_me_is_the_identity_and_assigned_is_mine_within_a_window() {
        let v = fixture();
        let p = parse("assignee:@me assignee:me assignee:Jonas", &v);
        assert_eq!(
            p.fields,
            [field(FieldKey::Assignee, false, &["@me", "Jonas"])]
        );
        let eff = merge(&p, &SearchFilters::default());
        assert_eq!(
            eff.fields,
            [field(
                FieldKey::Assignee,
                false,
                &["mara.lindqvist", "Jonas"]
            )]
        );
        // Not `mine`: the author filter stays off.
        assert!(!eff.mine);
        assert!(!eff.is_empty());

        for (raw, days) in [
            ("assigned:today", 1),
            ("assigned:week", 7),
            ("assigned:30d", 30),
        ] {
            let p = parse(raw, &v);
            assert_eq!(
                p.fields,
                [FieldFilter {
                    within_days: Some(days),
                    ..field(FieldKey::Assigned, false, &[])
                }],
                "{raw:?}"
            );
            let eff = merge(&p, &SearchFilters::default());
            assert_eq!(eff.fields[0].values, ["mara.lindqvist"], "{raw:?}");
        }
        // Two windows: either of them, so the wider.
        assert_eq!(
            parse("assigned:today assigned:week", &v).fields[0].within_days,
            Some(7)
        );

        let mut nobody = Vocabulary::fixture();
        nobody.identity.clear();
        let eff = merge(&parse("assignee:@me", &nobody), &SearchFilters::default());
        assert_eq!(eff.fields, [field(FieldKey::Assignee, false, &[])]);
        assert!(
            !eff.is_empty(),
            "knobas not knowing who you are is still a filter"
        );
    }

    /// Every ticket key negates with a leading `-`, as a filter of its own
    /// beside the unsigned one; a minus never turns a key into a word.
    #[test]
    fn every_ticket_key_negates() {
        let v = fixture();
        for (raw, key, value) in [
            ("project:PAY", FieldKey::Project, Some("PAY")),
            ("status:Done", FieldKey::Status, Some("Done")),
            ("issuetype:Bug", FieldKey::IssueType, Some("Bug")),
            ("priority:High", FieldKey::Priority, Some("High")),
            ("assignee:jonas", FieldKey::Assignee, Some("jonas")),
            ("label:sepa", FieldKey::Label, Some("sepa")),
            ("sprint:current", FieldKey::Sprint, Some("current")),
            ("assigned:week", FieldKey::Assigned, None),
        ] {
            let p = parse(&format!("{raw} -{raw}"), &v);
            assert_eq!(p.fields.len(), 2, "{raw:?}");
            for (filter, negated) in p.fields.iter().zip([false, true]) {
                assert_eq!(filter.key, key, "{raw:?}");
                assert_eq!(filter.negated, negated, "{raw:?}");
                assert_eq!(
                    filter.values,
                    value.map(str::to_owned).into_iter().collect::<Vec<_>>(),
                    "{raw:?}"
                );
            }
            assert!(p.query.text.is_empty(), "{raw:?}");
            assert!(p.query.unknown_tokens.is_empty(), "{raw:?}");
        }
    }

    /// A value the key cannot read is reported, never guessed at; a key with
    /// nothing after it is somebody mid-typing, neither a filter nor a
    /// mistake.
    #[test]
    fn an_unreadable_ticket_value_is_reported_and_a_bare_key_is_typing() {
        let v = fixture();
        let p = parse("sprint:next assigned:soonish x", &v);
        assert!(p.fields.is_empty());
        assert_eq!(p.query.unknown_tokens, ["sprint:next", "assigned:soonish"]);
        assert_eq!(p.query.text, "x");
        for raw in ["status:", "-label:", "project:\"\"", "assigned:"] {
            let p = parse(raw, &v);
            assert!(p.fields.is_empty(), "{raw:?}");
            assert!(p.query.unknown_tokens.is_empty(), "{raw:?}");
            assert!(p.query.text.is_empty(), "{raw:?}");
        }
    }

    /// No chip carries these keys: they live in the text, as `under:` does,
    /// so the echo is unchanged by them.
    #[test]
    fn ticket_keys_are_typed_filters_the_echo_does_not_carry() {
        let v = fixture();
        let eff = merge(&parse("status:Done", &v), &SearchFilters::default());
        assert_eq!(eff.echo(), SearchFilters::default());
        assert!(!eff.is_empty());
    }
}
