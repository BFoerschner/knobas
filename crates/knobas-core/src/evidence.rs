//! Proposal evidence (#836): why the detector proposed a link, in more than
//! one line.
//!
//! The stored `reason` stays what it was: one clause, written by the rule
//! when it proposed (see [`crate::suggest`]). This reads, for **one**
//! proposal, what that clause rests on:
//!
//! * **excerpts** -- the text where the evidence was found, with the matching
//!   text marked: the key in a branch name, commit message, build or page; the
//!   host in a monitor's text; the words two documents share;
//! * **support** -- items found the same way and already linked to the same
//!   ticket, a key named more than once, the shared words;
//! * **against** -- other keys the same text names, proposals found the same
//!   way at these ends that the reader dismissed, an end its source withdrew,
//!   and for shared wording, that wording alone often matches unrelated items.
//!
//! A read beside the tray and not a field on it: the tray's ranking and its
//! read (`suggest::proposals`) do not change, and the excerpts cost a text
//! scan per proposal that only the open proposal needs. Its own module
//! rather than a child of `suggest`, so the responsiveness probe's
//! after-pass snapshot, keyed on `suggest.rs`, survives it (as `tray.rs`
//! does).
//!
//! The words it writes are English sentences, as the stored reason is: the
//! shell draws them and adds none of its own.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::CoreError;

/// Everything said for and against one proposal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProposalEvidence {
    pub link_id: Uuid,
    /// The reason the rule stored, unchanged.
    pub reason: Option<String>,
    /// The text the evidence was found in, with the match marked. Empty for a
    /// rule whose evidence is not in any end's text (a relation the source
    /// records).
    pub excerpts: Vec<Excerpt>,
    /// *Also found*: what supports the proposal. May be empty.
    pub support: Vec<String>,
    /// *Speaks against it*. Never empty: [`NOTHING_AGAINST`] when nothing does.
    pub against: Vec<String>,
}

/// A stretch of one end's text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Excerpt {
    /// The end the text is from.
    pub entity_id: String,
    /// The text in order; the marked parts are the match. Parts rather than
    /// offsets, so no reader has to agree with Rust on what a character is.
    pub parts: Vec<Part>,
}

/// One run of an excerpt's text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Part {
    pub text: String,
    pub marked: bool,
}

/// What *Speaks against it* says when nothing does (prototype-6's words).
pub const NOTHING_AGAINST: &str = "Nothing found against it.";

/// What *Speaks against it* says of every shared-wording proposal.
pub const WORDING_ALONE: &str = "Wording alone often matches unrelated items.";

/// The rules that find a ticket's key in the other end's text. Each proposes
/// `item -> ticket`.
const KEY_RULES: [&str; 4] = [
    "branch_name_key",
    "commit_message_key",
    "build_parameter_key",
    "page_text_key",
];

/// How many excerpts one proposal quotes from one end.
const EXCERPTS_PER_END: usize = 2;

/// How many characters an excerpt keeps on each side of a match.
const CONTEXT_CHARS: usize = 60;

/// How many items a sentence names before it counts the rest.
const NAMED: usize = 3;

/// How much of an end's text the shared-wording excerpt is cut from.
const HEADLINE_CHARS: i32 = 20_000;

/// The markers `ts_headline` writes around a shared word: two characters
/// from the private use area, which no source's text is expected to hold.
const START: char = '\u{E000}';
const STOP: char = '\u{E001}';

#[derive(sqlx::FromRow)]
struct Row {
    from_id: String,
    to_id: String,
    rule: Option<String>,
    reason: Option<String>,
    from_kind: String,
    from_title: String,
    from_gone: bool,
    to_kind: String,
    to_title: String,
    to_gone: bool,
    from_text: Option<Vec<String>>,
    to_text: Option<Vec<String>>,
}

/// The proposal, both ends, and each end's mirrored title and body (none for
/// an end that is not a mirror item: an asset, a note).
const PROPOSAL: &str = "select l.from_id, l.to_id, l.rule, l.reason,
        f.kind as from_kind, f.title as from_title, f.deleted_at is not null as from_gone,
        t.kind as to_kind, t.title as to_title, t.deleted_at is not null as to_gone,
        (select array[i.title, i.body_text] from sync.item i where i.entity_id = l.from_id)
            as from_text,
        (select array[i.title, i.body_text] from sync.item i where i.entity_id = l.to_id)
            as to_text
   from knobas.link l
   join knobas.entity f on f.id = l.from_id
   join knobas.entity t on t.id = l.to_id
  where l.id = $1";

/// Links the same rule found to the same ticket that the reader accepted,
/// newest first: `$1` the ticket, `$2` the rule, `$3` this proposal.
const SAME_WAY_LINKED: &str = "select e.id, e.kind, e.title
   from knobas.link l
   join knobas.entity e on e.id = l.from_id
  where l.to_id = $1 and l.rule = $2 and l.id <> $3
    and l.confirmed_at is not null and l.deleted_at is null
  order by l.confirmed_at desc, l.id
  limit 4";

/// Proposals the same rule made at either end that the reader dismissed:
/// unconfirmed rows with a tombstone.
const SAME_WAY_DISMISSED: &str = "select count(*)
   from knobas.link l
  where l.rule = $1 and l.id <> $2
    and l.confirmed_at is null and l.deleted_at is not null
    and (l.from_id = any($3) or l.to_id = any($3))";

/// Live proposals and links the same rule made to the same end from another
/// item: `$1` that end, `$2` the rule, `$3` this proposal.
const SAME_WAY_ELSEWHERE: &str = "select count(*)
   from knobas.link l
  where l.to_id = $1 and l.rule = $2 and l.id <> $3 and l.deleted_at is null";

/// The words two items share, as the similarity rule counts them: lexemes of
/// both `fts`, the common ones (#776) left out.
const SHARED_LEXEMES: &str = "with common as (
       select array(select c.lexeme from sync.similar_text_lexeme c where c.common) as words)
     select array(
              select x from unnest(tsvector_to_array(ts_delete(strip(a.fts), common.words))) x
              intersect
              select y from unnest(tsvector_to_array(ts_delete(strip(b.fts), common.words))) y
              order by 1)
       from sync.item a, sync.item b, common
      where a.entity_id = $1 and b.entity_id = $2";

/// One end's text around the shared words, the words marked. `$2` is the
/// shared lexemes as a `tsquery`, `$3` the options.
const HEADLINE: &str = "select ts_headline('english',
              left(case when i.body_text = '' then i.title
                        else i.title || E'\\n' || i.body_text end, $4),
              $2::tsquery, $3),
          left(case when i.body_text = '' then i.title
                    else i.title || E'\\n' || i.body_text end, $4)
     from sync.item i
    where i.entity_id = $1";

/// What was said for and against the proposal `id`.
///
/// Read for a link in any state: an accepted proposal keeps its rule and
/// reason as provenance, so its evidence still reads.
///
/// # Errors
///
/// [`CoreError::LinkNotFound`] if no link carries `id`; [`CoreError::Db`] if
/// a query fails.
pub async fn proposal_evidence(pool: &PgPool, id: Uuid) -> Result<ProposalEvidence, CoreError> {
    let row = sqlx::query_as::<_, Row>(PROPOSAL)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(CoreError::LinkNotFound(id))?;
    let rule = row.rule.as_deref().unwrap_or("");
    let mut found = ProposalEvidence {
        link_id: id,
        reason: row.reason.clone(),
        excerpts: Vec::new(),
        support: Vec::new(),
        against: Vec::new(),
    };

    if KEY_RULES.contains(&rule) {
        by_key(pool, id, &row, rule, &mut found).await?;
    } else if rule == "similar_text" {
        by_wording(pool, &row, &mut found).await?;
    } else if rule == "monitor_url_host" {
        by_host(pool, id, &row, rule, &mut found).await?;
    }

    if !rule.is_empty() {
        let (dismissed,): (i64,) = sqlx::query_as(SAME_WAY_DISMISSED)
            .bind(rule)
            .bind(id)
            .bind([&row.from_id, &row.to_id])
            .fetch_one(pool)
            .await?;
        if dismissed > 0 {
            found.against.push(if dismissed == 1 {
                "1 other proposal found the same way at these ends was dismissed.".to_owned()
            } else {
                format!(
                    "{dismissed} other proposals found the same way at these ends were dismissed."
                )
            });
        }
    }
    for (gone, kind, end_id, title) in [
        (row.from_gone, &row.from_kind, &row.from_id, &row.from_title),
        (row.to_gone, &row.to_kind, &row.to_id, &row.to_title),
    ] {
        if gone {
            found.against.push(format!(
                "{} was withdrawn by its source.",
                end_name(end_id, kind, title)
            ));
        }
    }
    if found.against.is_empty() {
        found.against.push(NOTHING_AGAINST.to_owned());
    }
    Ok(found)
}

/// An exact key: the key quoted where the item names it, the items found the
/// same way and already linked, and the other keys the text names.
async fn by_key(
    pool: &PgPool,
    id: Uuid,
    row: &Row,
    rule: &str,
    found: &mut ProposalEvidence,
) -> Result<(), CoreError> {
    let key = key_of(&row.to_id);
    let fields = row.from_text.clone().unwrap_or_default();
    let mut times = 0;
    for field in &fields {
        times += occurrences(field, key).len();
        for parts in excerpts(field, &occurrences(field, key)) {
            if found.excerpts.len() < EXCERPTS_PER_END {
                found.excerpts.push(Excerpt {
                    entity_id: row.from_id.clone(),
                    parts,
                });
            }
        }
    }

    let linked: Vec<(String, String, String)> = sqlx::query_as(SAME_WAY_LINKED)
        .bind(&row.to_id)
        .bind(rule)
        .bind(id)
        .fetch_all(pool)
        .await?;
    if !linked.is_empty() {
        let names: Vec<String> = linked
            .iter()
            .map(|(id, kind, title)| end_name(id, kind, title))
            .collect();
        found.support.push(format!(
            "Found the same way and already linked to {key}: {}.",
            series(&names, ", ")
        ));
    }
    if times > 1 {
        found.support.push(format!("It names {key} {times} times."));
    }

    let mut others: Vec<&str> = Vec::new();
    for field in &fields {
        for other in issue_keys(field) {
            if other != key && !others.contains(&other) {
                others.push(other);
            }
        }
    }
    if !others.is_empty() {
        let others: Vec<String> = others.iter().map(|k| (*k).to_owned()).collect();
        found
            .against
            .push(format!("It also names {}.", series(&others, " and ")));
    }
    Ok(())
}

/// Shared wording: each end quoted around the words they share, the words
/// marked, and the words themselves.
async fn by_wording(
    pool: &PgPool,
    row: &Row,
    found: &mut ProposalEvidence,
) -> Result<(), CoreError> {
    found.against.push(WORDING_ALONE.to_owned());
    let lexemes: Option<(Vec<String>,)> = sqlx::query_as(SHARED_LEXEMES)
        .bind(&row.from_id)
        .bind(&row.to_id)
        .fetch_optional(pool)
        .await?;
    let lexemes = lexemes.map(|(words,)| words).unwrap_or_default();
    if lexemes.is_empty() {
        return Ok(());
    }
    let query = lexemes
        .iter()
        .map(|lexeme| format!("'{}'", lexeme.replace('\\', "\\\\").replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(" | ");
    let options = format!(
        "StartSel=\"{START}\", StopSel=\"{STOP}\", MaxWords=30, MinWords=12, ShortWord=2, MaxFragments=1"
    );
    let mut words: Vec<String> = Vec::new();
    for end in [&row.from_id, &row.to_id] {
        let headline: Option<(String, String)> = sqlx::query_as(HEADLINE)
            .bind(end)
            .bind(&query)
            .bind(&options)
            .bind(HEADLINE_CHARS)
            .fetch_optional(pool)
            .await?;
        let Some((headline, whole)) = headline else {
            continue;
        };
        let parts = headline_parts(&headline, &whole);
        for part in parts.iter().filter(|p| p.marked) {
            let word = part.text.to_lowercase();
            if !words.contains(&word) {
                words.push(word);
            }
        }
        if parts.iter().any(|p| p.marked) {
            found.excerpts.push(Excerpt {
                entity_id: end.clone(),
                parts,
            });
        }
    }
    if words.is_empty() {
        words = lexemes;
    }
    words.truncate(8);
    found
        .support
        .push(format!("Shared words: {}.", words.join(", ")));
    Ok(())
}

/// A monitor's host: the host quoted where the monitor's text names it, and
/// the other assets the same host was proposed for.
async fn by_host(
    pool: &PgPool,
    id: Uuid,
    row: &Row,
    rule: &str,
    found: &mut ProposalEvidence,
) -> Result<(), CoreError> {
    if let Some(host) = row.reason.as_deref().and_then(watched_host) {
        for field in row.to_text.clone().unwrap_or_default() {
            for parts in excerpts(&field, &occurrences(&field, host)) {
                if found.excerpts.len() < EXCERPTS_PER_END {
                    found.excerpts.push(Excerpt {
                        entity_id: row.to_id.clone(),
                        parts,
                    });
                }
            }
        }
    }
    let (elsewhere,): (i64,) = sqlx::query_as(SAME_WAY_ELSEWHERE)
        .bind(&row.to_id)
        .bind(rule)
        .bind(id)
        .fetch_one(pool)
        .await?;
    if elsewhere > 0 {
        found.against.push(if elsewhere == 1 {
            "The same host is proposed for 1 other asset.".to_owned()
        } else {
            format!("The same host is proposed for {elsewhere} other assets.")
        });
    }
    Ok(())
}

/// The key of an entity id: what follows the namespace (`jira:PAY-231` is
/// `PAY-231`).
fn key_of(entity_id: &str) -> &str {
    entity_id.split_once(':').map_or(entity_id, |(_, key)| key)
}

/// An end as a sentence names it, as the Proposed links card does: a ticket
/// by key and title, a commit by sha and quoted message, anything else by
/// title.
fn end_name(entity_id: &str, kind: &str, title: &str) -> String {
    match kind {
        "ticket" => format!("{} {title}", key_of(entity_id)),
        "commit" => format!("{} “{title}”", key_of(entity_id)),
        _ => title.to_owned(),
    }
}

/// The host the monitor rule's reason names: "this monitor watches <host>, …".
fn watched_host(reason: &str) -> Option<&str> {
    let rest = reason.strip_prefix("this monitor watches ")?;
    rest.split_once(',').map(|(host, _)| host)
}

/// A word character, as PostgreSQL's `\m` and `\M` see one.
fn wordy(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The byte ranges where `needle` stands in `text` as a whole word: no word
/// character just before or after it, so `PAY-231` is not found in `PAY-2311`
/// or `xPAY-231`, as the key rules' `\m...\M` reads it.
fn occurrences(text: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    text.match_indices(needle)
        .filter(|(at, _)| {
            let before = text[..*at].chars().next_back();
            let after = text[at + needle.len()..].chars().next();
            !before.is_some_and(wordy) && !after.is_some_and(wordy)
        })
        .map(|(at, _)| (at, at + needle.len()))
        .collect()
}

/// Every issue-key-shaped word in `text` (`[A-Z][A-Z0-9]+-[0-9]+` between
/// word boundaries), in order.
fn issue_keys(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut keys = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let starts_word = at == 0 || !text[..at].chars().next_back().is_some_and(wordy);
        if starts_word && bytes[at].is_ascii_uppercase() {
            let mut end = at + 1;
            while end < bytes.len()
                && (bytes[end].is_ascii_uppercase() || bytes[end].is_ascii_digit())
            {
                end += 1;
            }
            if end > at + 1 && end < bytes.len() && bytes[end] == b'-' {
                let digits = end + 1;
                let mut stop = digits;
                while stop < bytes.len() && bytes[stop].is_ascii_digit() {
                    stop += 1;
                }
                if stop > digits && !text[stop..].chars().next().is_some_and(wordy) {
                    keys.push(&text[at..stop]);
                    at = stop;
                    continue;
                }
            }
        }
        at += text[at..].chars().next().map_or(1, char::len_utf8);
    }
    keys
}

/// The excerpts around `matches` in `text`: [`CONTEXT_CHARS`] on each side,
/// windows that touch merged into one, `…` where the text goes on.
fn excerpts(text: &str, matches: &[(usize, usize)]) -> Vec<Vec<Part>> {
    let mut windows: Vec<(usize, usize, Vec<(usize, usize)>)> = Vec::new();
    for &(start, end) in matches {
        let from = back(text, start, CONTEXT_CHARS);
        let to = forward(text, end, CONTEXT_CHARS);
        match windows.last_mut() {
            Some(last) if from <= last.1 => {
                last.1 = to;
                last.2.push((start, end));
            }
            _ => windows.push((from, to, vec![(start, end)])),
        }
    }
    windows
        .into_iter()
        .map(|(from, to, marks)| {
            let mut parts = Vec::new();
            let mut at = from;
            let lead = if from > 0 { "…" } else { "" };
            for (start, end) in marks {
                parts.push(plain(format!(
                    "{}{}",
                    if at == from { lead } else { "" },
                    &text[at..start]
                )));
                parts.push(Part {
                    text: text[start..end].to_owned(),
                    marked: true,
                });
                at = end;
            }
            parts.push(plain(format!(
                "{}{}",
                &text[at..to],
                if to < text.len() { "…" } else { "" }
            )));
            parts.retain(|p| !p.text.is_empty());
            parts
        })
        .collect()
}

fn plain(text: String) -> Part {
    Part {
        text,
        marked: false,
    }
}

/// The byte offset `n` characters before `at`.
fn back(text: &str, at: usize, n: usize) -> usize {
    text[..at]
        .char_indices()
        .rev()
        .nth(n.saturating_sub(1))
        .map_or(0, |(i, _)| i)
}

/// The byte offset `n` characters after `at`.
fn forward(text: &str, at: usize, n: usize) -> usize {
    text[at..]
        .char_indices()
        .nth(n)
        .map_or(text.len(), |(i, _)| at + i)
}

/// `ts_headline`'s answer split at its markers, `…` where `whole` goes on.
fn headline_parts(headline: &str, whole: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut marked = false;
    let mut run = String::new();
    for c in headline.chars() {
        if c == START || c == STOP {
            if !run.is_empty() {
                parts.push(Part {
                    text: std::mem::take(&mut run),
                    marked,
                });
            }
            marked = c == START;
        } else {
            run.push(c);
        }
    }
    if !run.is_empty() {
        parts.push(Part { text: run, marked });
    }
    let shown: String = parts.iter().map(|p| p.text.as_str()).collect();
    let shown = shown.trim();
    if !whole.trim_start().starts_with(shown) {
        parts.insert(0, plain("…".to_owned()));
    }
    if !whole.trim_end().ends_with(shown) {
        parts.push(plain("…".to_owned()));
    }
    parts
}

/// Names in a sentence: "a", "a and b", "a, b and c", "a, b, c and 2 more".
/// `last` joins the final pair (", " or " and ").
fn series(names: &[String], last: &str) -> String {
    let shown = names.len().min(NAMED);
    let rest = names.len() - shown;
    let names = &names[..shown];
    match (names, rest) {
        ([], _) => String::new(),
        ([one], 0) => one.clone(),
        (_, 0) => {
            let (tail, head) = names.split_last().expect("two or more");
            format!("{}{last}{tail}", head.join(", "))
        }
        _ => format!("{} and {rest} more", names.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(parts: &[Part]) -> Vec<&str> {
        parts
            .iter()
            .filter(|p| p.marked)
            .map(|p| p.text.as_str())
            .collect()
    }

    fn text(parts: &[Part]) -> String {
        parts.iter().map(|p| p.text.as_str()).collect()
    }

    #[test]
    fn a_key_is_found_only_as_a_whole_word() {
        let text = "PAY-231 and xPAY-231, PAY-2311, PAY-231x, (PAY-231).";
        let found: Vec<&str> = occurrences(text, "PAY-231")
            .into_iter()
            .map(|(a, b)| &text[a..b])
            .collect();
        assert_eq!(found.len(), 2);
        assert_eq!(
            occurrences(text, "PAY-231")[1].0,
            text.find("(PAY-231)").unwrap() + 1
        );
    }

    #[test]
    fn issue_keys_are_read_in_order_between_word_boundaries() {
        assert_eq!(
            issue_keys("Refs PAY-231, OPS-77 and xPAY-1, A-1, PAY-2x, UTF-8. ÄPAY-3"),
            ["PAY-231", "OPS-77", "UTF-8"]
        );
    }

    #[test]
    fn an_excerpt_keeps_context_on_both_sides_and_says_where_the_text_goes_on() {
        let long = format!("{}PAY-231{}", "a ".repeat(50), " b".repeat(50));
        let got = excerpts(&long, &occurrences(&long, "PAY-231"));
        assert_eq!(got.len(), 1);
        assert_eq!(marks(&got[0]), ["PAY-231"]);
        let shown = text(&got[0]);
        assert!(shown.starts_with('…') && shown.ends_with('…'), "{shown}");
        assert_eq!(shown.chars().count(), 2 + 7 + 2 * CONTEXT_CHARS);
    }

    #[test]
    fn a_short_text_is_quoted_whole_without_ellipses() {
        let got = excerpts(
            "PAY-231 backoff jitter",
            &occurrences("PAY-231 backoff jitter", "PAY-231"),
        );
        assert_eq!(
            got,
            [vec![
                Part {
                    text: "PAY-231".into(),
                    marked: true
                },
                plain(" backoff jitter".into()),
            ]]
        );
    }

    #[test]
    fn matches_close_together_share_one_excerpt() {
        let both = "PAY-231 then PAY-231 again";
        let got = excerpts(both, &occurrences(both, "PAY-231"));
        assert_eq!(got.len(), 1);
        assert_eq!(marks(&got[0]), ["PAY-231", "PAY-231"]);
        assert_eq!(text(&got[0]), both);
    }

    #[test]
    fn multibyte_text_is_cut_on_character_boundaries() {
        let text = format!("{}PAY-231{}", "ü ".repeat(80), " é".repeat(80));
        let got = excerpts(&text, &occurrences(&text, "PAY-231"));
        assert_eq!(marks(&got[0]), ["PAY-231"]);
    }

    #[test]
    fn a_headline_is_split_at_its_markers() {
        let headline = format!("the {START}payout{STOP} {START}ledger{STOP} drifts");
        let parts = headline_parts(&headline, "Intro.\nthe payout ledger drifts");
        assert_eq!(marks(&parts), ["payout", "ledger"]);
        assert_eq!(text(&parts), "…the payout ledger drifts");
    }

    #[test]
    fn names_read_as_a_sentence() {
        let n = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(series(&n(&["a"]), " and "), "a");
        assert_eq!(series(&n(&["a", "b"]), " and "), "a and b");
        assert_eq!(series(&n(&["a", "b", "c"]), " and "), "a, b and c");
        assert_eq!(series(&n(&["a", "b"]), ", "), "a, b");
        assert_eq!(
            series(&n(&["a", "b", "c", "d", "e"]), " and "),
            "a, b, c and 2 more"
        );
    }

    #[test]
    fn the_monitor_reason_names_its_host() {
        assert_eq!(
            watched_host("this monitor watches gitea, the host of the route compose"),
            Some("gitea")
        );
        assert_eq!(watched_host("the branch name contains PAY-1"), None);
    }
}
