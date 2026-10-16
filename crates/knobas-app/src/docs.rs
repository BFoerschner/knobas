//! The Docs reader's two reads (#859, ADR-0022): the spaces with their page
//! trees, and one page drawn as blocks.
//!
//! The shell draws the Docs frame and a docs source fills it. What knobas
//! reads here is declared, never guessed (ADR-0007):
//!
//! - **a space** is the page's project: `sync.item_project`, which the database
//!   resolves from the declared `project_key` and `project_name` (#759);
//! - **the tree** is the declared parent chain (ADR-0018): a page's direct
//!   parent is the innermost name of its chain, which a docs source keeps
//!   unique inside a space (a Confluence title is);
//! - **the body, comments, version and labels** are the adapter's projection
//!   of the mirrored record (`knobas_source::docs::DocProjection`), read from
//!   the registry by the source's adapter kind. No network and no credential:
//!   the reader opens from the mirror.
//!
//! What knobas adds to the projection is what only the mirror knows: which
//! mirrored page a page link names, which mirrored ticket a ticket chip names
//! and its status now, and which mentions are of the reader.

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use knobas_core::payload::Declarations;
use knobas_source::docs::{DocBlock, DocComment, Inline, PAGE_KIND};
use serde::Serialize;
use sqlx::{PgPool, Row};

use crate::IpcError;

/// Every space the mirror holds pages of, and the page read last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocsTree {
    /// Ordered by source, then key.
    pub spaces: Vec<DocSpace>,
    /// The page the reader opened most recently (`visit_entity`), while it is
    /// still a live page; `#/docs` opens it.
    pub last_read: Option<String>,
}

/// One space and its pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocSpace {
    pub source_id: String,
    pub key: String,
    /// The space's name, `None` where no page of it carries one knobas can
    /// read.
    pub name: Option<String>,
    /// Every live page of the space, ordered by title.
    pub pages: Vec<DocNode>,
}

/// One page in a space's tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocNode {
    pub entity_id: String,
    pub title: String,
    /// The page above it, where its declared parent is a mirrored page of the
    /// same space; `None` for a page at the top of what the mirror holds.
    pub parent: Option<String>,
}

/// One page, as the reader draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocPage {
    pub entity_id: String,
    pub source_id: String,
    /// The source's configured name, falling back to its id: what *Open in …*
    /// and the placeholder name (R-10.18 keeps those two source-specific).
    pub source_name: String,
    pub title: String,
    pub space: Option<DocSpaceRef>,
    /// The parent chain, outermost first: each ancestor's title, and the
    /// mirrored page it is where the mirror holds it.
    pub ancestors: Vec<DocAncestor>,
    /// Who made this version (contract §4.1's `author`).
    pub editor: Option<String>,
    /// The editor's display name, where the source's record carries one.
    pub editor_name: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
    /// The page in the source (*Open in Confluence*).
    pub web_url: Option<String>,
    /// Whether the source's adapter projects its pages. `false` draws the
    /// header and says the body cannot be read; every field below is empty.
    pub readable: bool,
    pub version: Option<u64>,
    pub labels: Vec<String>,
    pub blocks: Vec<DocBlock>,
    pub comments: Vec<DocComment>,
    /// The identity `@me` resolves to, so the reader can say *You* for the
    /// editor and a comment's author.
    pub me: Vec<String>,
}

/// A space, named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocSpaceRef {
    pub key: String,
    pub name: Option<String>,
}

/// One step of a page's parent chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocAncestor {
    pub title: String,
    pub entity_id: Option<String>,
}

/// The live pages with their space and their declared direct parent's name.
/// `$1` is the declarations.
const PAGES: &str = concat!(
    "select i.entity_id, i.title, p.source_id, p.key, p.name, \
       (select pn.name from ",
    knobas_core::declared_parent_names!("$1"),
    " pn order by pn.pos desc, pn.name_ord limit 1) as parent \
       from sync.live_item i \
       join sync.item_project p on p.entity_id = i.entity_id \
       join knobas.source_config s on s.id = p.source_id and s.kind = p.adapter_kind \
      where i.kind = 'page' \
      order by p.source_id, p.key, lower(i.title), i.entity_id"
);

const LAST_READ: &str = "\
select a.entity_id from knobas.activity a
  join sync.live_item i on i.entity_id = a.entity_id
 where a.verb = $1 and a.actor = 'user' and i.kind = 'page'
 order by a.at desc, a.id desc
 limit 1";

/// The spaces with their trees, and the page read last.
///
/// # Errors
///
/// `internal` for a query failure.
pub async fn tree(pool: &PgPool, declarations: &Declarations) -> Result<DocsTree, IpcError> {
    let rows = sqlx::query(PAGES)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?;
    let mut spaces: Vec<DocSpace> = Vec::new();
    let mut parents: Vec<Vec<Option<String>>> = Vec::new();
    for row in rows {
        let source_id: String = row.get("source_id");
        let key: String = row.get("key");
        let name: Option<String> = row.get("name");
        let same = spaces
            .last()
            .is_some_and(|s| s.source_id == source_id && s.key == key);
        if !same {
            spaces.push(DocSpace {
                source_id,
                key,
                name: None,
                pages: Vec::new(),
            });
            parents.push(Vec::new());
        }
        let space = spaces.last_mut().expect("pushed above");
        if space.name.is_none() {
            space.name = name;
        }
        space.pages.push(DocNode {
            entity_id: row.get("entity_id"),
            title: row.get("title"),
            parent: None,
        });
        parents
            .last_mut()
            .expect("pushed above")
            .push(row.get("parent"));
    }
    for (space, parents) in spaces.iter_mut().zip(parents) {
        let by_title: HashMap<String, String> = space
            .pages
            .iter()
            .map(|p| (p.title.clone(), p.entity_id.clone()))
            .collect();
        for (page, parent) in space.pages.iter_mut().zip(parents) {
            page.parent = parent
                .and_then(|title| by_title.get(&title).cloned())
                .filter(|id| *id != page.entity_id);
        }
        break_loops(&mut space.pages);
    }
    let last_read = sqlx::query_scalar(LAST_READ)
        .bind(knobas_core::activity::VISITED)
        .fetch_optional(pool)
        .await?;
    Ok(DocsTree { spaces, last_read })
}

/// Cut every parent loop: two pages that name each other (titles are matched,
/// and a title can be reused after a move) would otherwise have no top and
/// would recurse in every walk of the tree. Each page on a loop is put at the
/// top, so it is still drawn. A page that only hangs under a loop keeps its
/// parent: once the loop is cut, that parent sits at the top.
fn break_loops(pages: &mut [DocNode]) {
    let parent_of: HashMap<String, Option<String>> = pages
        .iter()
        .map(|p| (p.entity_id.clone(), p.parent.clone()))
        .collect();
    for page in pages.iter_mut() {
        let mut seen = std::collections::HashSet::new();
        let mut at = page.parent.clone();
        while let Some(id) = at {
            if id == page.entity_id {
                page.parent = None;
                break;
            }
            if !seen.insert(id.clone()) {
                break;
            }
            at = parent_of.get(&id).cloned().flatten();
        }
    }
}

/// Every inline of a page, body and comments, for the fill-in pass.
fn every_inline(content: &mut knobas_source::docs::DocContent) -> Vec<&mut Inline> {
    let mut inlines = knobas_source::docs::inlines_mut(&mut content.blocks);
    for c in &mut content.comments {
        inlines.extend(knobas_source::docs::inlines_mut(&mut c.blocks));
    }
    inlines
}

/// One page and what the reader needs around it. `$1` the id, `$2` the
/// declarations.
const PAGE: &str = concat!(
    "select i.entity_id, i.source_id, i.kind, i.title, i.author, i.item_updated_at, \
            i.web_url, i.payload, s.kind as adapter_kind, \
            coalesce(nullif(btrim(s.display_name), ''), s.id) as source_name, p.key, p.name, \
            coalesce((select jsonb_agg(f.name order by f.pos) from \
              (select distinct on (pn.pos) pn.pos, pn.name from ",
    knobas_core::declared_parent_names!("$2"),
    " pn order by pn.pos, pn.name_ord) f), '[]'::jsonb) as ancestors \
       from sync.live_item i \
       join knobas.source_config s on s.id = i.source_id \
       left join sync.item_project p on p.entity_id = i.entity_id and p.adapter_kind = s.kind \
      where i.entity_id = $1"
);

/// The live pages of one space of one source, by title. `$1` source, `$2`
/// space key, `$3` titles.
const PAGES_BY_TITLE: &str = "\
select i.title, i.entity_id from sync.live_item i
  join sync.item_project p on p.entity_id = i.entity_id
  join knobas.source_config s on s.id = p.source_id and s.kind = p.adapter_kind
 where i.kind = 'page' and p.source_id = $1 and p.key = $2 and i.title = any($3)
 order by i.entity_id";

/// The mirrored tickets with these keys (the entity id past its namespace),
/// with their declared status. `$1` keys, `$2` declarations.
const TICKETS: &str = concat!(
    "select substr(i.entity_id, strpos(i.entity_id, ':') + 1) as key, i.entity_id, i.title, ",
    knobas_core::declared_string!("$2", "status_name"),
    " as status from sync.live_item i \
      where i.kind = 'ticket' and substr(i.entity_id, strpos(i.entity_id, ':') + 1) = any($1) \
      order by i.entity_id"
);

/// One page, drawn.
///
/// # Errors
///
/// `invalid` for something that is not an entity id, `not_found` for one the
/// live mirror does not hold or that is not a page, `internal` for a query
/// failure.
pub async fn page(
    pool: &PgPool,
    declarations: &Declarations,
    project: impl Fn(&str) -> Option<knobas_source::docs::DocProjection>,
    me: &[String],
    entity_id: &str,
) -> Result<DocPage, IpcError> {
    let entity = knobas_core::entity::EntityRef::parse(entity_id).map_err(IpcError::invalid)?;
    let id = entity.to_string();
    let row = sqlx::query(PAGE)
        .bind(&id)
        .bind(declarations.as_param())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("{id} is not in the mirror")))?;
    let kind: String = row.get("kind");
    if kind != PAGE_KIND {
        return Err(IpcError::not_found(format!("{id} is a {kind}, not a page")));
    }
    let source_id: String = row.get("source_id");
    let adapter_kind: String = row.get("adapter_kind");
    let payload: serde_json::Value = row.get("payload");
    let space = row.get::<Option<String>, _>("key").map(|key| DocSpaceRef {
        key,
        name: row.get("name"),
    });
    let chain: Vec<String> =
        serde_json::from_value(row.get::<serde_json::Value, _>("ancestors")).unwrap_or_default();

    let content = project(&adapter_kind).and_then(|projection| projection(&kind, &payload));
    let readable = content.is_some();
    let mut content = content.unwrap_or(knobas_source::docs::DocContent {
        version: None,
        editor_name: None,
        labels: Vec::new(),
        blocks: Vec::new(),
        comments: Vec::new(),
    });

    // Titles to resolve, per space: the chain in this page's space, and every
    // page link in its own (or this page's).
    let here = space.as_ref().map(|s| s.key.clone());
    let mut wanted: HashMap<String, BTreeSet<String>> = HashMap::new();
    if let Some(key) = &here {
        wanted
            .entry(key.clone())
            .or_default()
            .extend(chain.iter().cloned());
    }
    let mut keys: BTreeSet<String> = BTreeSet::new();
    {
        let inlines = every_inline(&mut content);
        for inline in &inlines {
            match &**inline {
                Inline::PageLink { title, space, .. } => {
                    if let Some(key) = space.clone().or_else(|| here.clone()) {
                        wanted.entry(key).or_default().insert(title.clone());
                    }
                }
                Inline::Issue { key, .. } => {
                    keys.insert(key.clone());
                }
                _ => {}
            }
        }
    }
    let mut pages: HashMap<(String, String), String> = HashMap::new();
    for (key, titles) in wanted {
        let titles: Vec<String> = titles.into_iter().collect();
        for r in sqlx::query(PAGES_BY_TITLE)
            .bind(&source_id)
            .bind(&key)
            .bind(&titles)
            .fetch_all(pool)
            .await?
        {
            pages
                .entry((key.clone(), r.get("title")))
                .or_insert_with(|| r.get("entity_id"));
        }
    }
    let mut tickets: HashMap<String, (String, String, Option<String>)> = HashMap::new();
    if !keys.is_empty() {
        let keys: Vec<String> = keys.into_iter().collect();
        for r in sqlx::query(TICKETS)
            .bind(&keys)
            .bind(declarations.as_param())
            .fetch_all(pool)
            .await?
        {
            tickets
                .entry(r.get("key"))
                .or_insert_with(|| (r.get("entity_id"), r.get("title"), r.get("status")));
        }
    }

    for inline in every_inline(&mut content) {
        match inline {
            Inline::PageLink {
                title,
                space,
                entity_id,
                ..
            } => {
                if let Some(key) = space.clone().or_else(|| here.clone()) {
                    *entity_id = pages.get(&(key, title.clone())).cloned();
                }
            }
            Inline::Issue {
                key,
                entity_id,
                title,
                status,
            } => {
                if let Some((id, summary, now)) = tickets.get(key) {
                    *entity_id = Some(id.clone());
                    *title = Some(summary.clone());
                    status.clone_from(now);
                }
            }
            Inline::Mention { user, me: mine, .. } => {
                *mine = user.as_ref().is_some_and(|u| me.contains(u));
            }
            _ => {}
        }
    }

    let ancestors = chain
        .into_iter()
        .map(|title| DocAncestor {
            entity_id: here
                .as_ref()
                .and_then(|key| pages.get(&(key.clone(), title.clone())).cloned()),
            title,
        })
        .collect();

    Ok(DocPage {
        entity_id: id,
        source_id,
        source_name: row.get("source_name"),
        title: row.get("title"),
        space,
        ancestors,
        editor: row.get("author"),
        editor_name: content.editor_name,
        updated_at: row.get("item_updated_at"),
        web_url: row.get("web_url"),
        readable,
        version: content.version,
        labels: content.labels,
        blocks: content.blocks,
        comments: content.comments,
        me: me.to_vec(),
    })
}

// -- the Docs search (#860) -------------------------------------------------

/// How many hits the Docs search draws. The header counts every match.
pub const SEARCH_LIMIT: u32 = 50;

/// The space the Docs search is kept to: the picker's space.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DocsScope {
    pub source_id: String,
    pub key: String,
}

/// The Docs search's answer (#860, R-10.3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocsHits {
    /// Every page the query admits, before the limit.
    pub total: u32,
    pub took_ms: u32,
    /// The words and phrases the reader marks, lowercased: each word, each
    /// quoted phrase whole, never one left out.
    pub needles: Vec<String>,
    /// What the grammar could not read.
    pub unknown_tokens: Vec<String>,
    /// The best pages, best first.
    pub hits: Vec<DocHit>,
}

/// One page the Docs search found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocHit {
    pub entity_id: String,
    pub title: String,
    pub space: Option<DocSpaceRef>,
    /// The parent chain as one line (`Engineering › Payments`), `None` at the
    /// top of a space.
    pub path: Option<String>,
    /// The best sentence, cut around its first match.
    pub snippet: DocSnippet,
    /// How often the needles occur in the title and the indexed text.
    pub matches: u32,
    pub editor: Option<String>,
    pub editor_name: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// A hit's snippet: where it is, and its text with the matches marked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocSnippet {
    /// The section heading it sits under, `None` before the first one.
    pub section: Option<String>,
    /// The comment's author's display name when it is a comment's; `Some("")`
    /// for a comment whose record named nobody.
    pub comment_by: Option<String>,
    pub segments: Vec<knobas_search::Segment>,
}

/// The rows a set of hits draws from. `$1` the ids.
const HIT_ROWS: &str = "\
select i.entity_id, i.body_text, i.author, i.payload, s.kind as adapter_kind, p.key, p.name
  from sync.live_item i
  join knobas.source_config s on s.id = i.source_id
  left join sync.item_project p on p.entity_id = i.entity_id and p.adapter_kind = s.kind
 where i.entity_id = any($1)";

/// The Docs search (#860): the pages `raw` admits, in `scope` when given,
/// each with its snippet, match count, space, path and editor.
///
/// `None` for a query with neither words nor a filter.
///
/// # Errors
///
/// `invalid` for a query past the launcher's length, `internal` for a query
/// failure.
pub async fn search(
    pool: &PgPool,
    searcher: &knobas_search::Searcher,
    project: impl Fn(&str) -> Option<knobas_source::docs::DocProjection>,
    raw: &str,
    scope: Option<&DocsScope>,
) -> Result<Option<DocsHits>, IpcError> {
    let found = searcher
        .page_search(
            raw,
            scope.map(|s| (s.source_id.as_str(), s.key.as_str())),
            SEARCH_LIMIT,
        )
        .await?;
    let Some(found) = found else {
        return Ok(None);
    };
    let ids: Vec<String> = found.hits.iter().map(|h| h.row.entity_id.clone()).collect();
    let rows = sqlx::query(HIT_ROWS).bind(&ids).fetch_all(pool).await?;
    let by_id: HashMap<String, sqlx::postgres::PgRow> = rows
        .into_iter()
        .map(|r| (r.get::<String, _>("entity_id"), r))
        .collect();
    let stems: Vec<Vec<char>> = found.needles.iter().map(|n| stem(n)).collect();
    let hits = found
        .hits
        .iter()
        .filter_map(|hit| {
            let row = by_id.get(&hit.row.entity_id)?;
            let body: String = row.get("body_text");
            let title = &hit.row.title;
            let adapter_kind: String = row.get("adapter_kind");
            let payload: serde_json::Value = row.get("payload");
            let editor_name = project(&adapter_kind)
                .and_then(|projection| projection(PAGE_KIND, &payload))
                .and_then(|c| c.editor_name);
            Some(DocHit {
                entity_id: hit.row.entity_id.clone(),
                title: title.clone(),
                space: row.get::<Option<String>, _>("key").map(|key| DocSpaceRef {
                    key,
                    name: row.get("name"),
                }),
                path: hit.row.path.clone().filter(|p| !p.trim().is_empty()),
                snippet: snippet(title, &body, &stems),
                matches: matches(title, &body, &stems),
                editor: row.get("author"),
                editor_name,
                updated_at: hit.row.updated_at,
            })
        })
        .collect();
    Ok(Some(DocsHits {
        total: found.total,
        took_ms: found.took_ms,
        needles: found.needles,
        unknown_tokens: found.unknown_tokens,
        hits,
    }))
}

/// A needle as the reader matches it: lowercased, and a word longer than four
/// letters without one `ing`, `es`, `ed` or `s` at its end, so "reviews"
/// finds "review" and "retrying" finds "retry" (prototype-6's stemming; the
/// shell marks with the same rule).
fn stem(needle: &str) -> Vec<char> {
    let lower: Vec<char> = needle.chars().map(lower).collect();
    if lower.len() > 4 {
        for suffix in ["ing", "es", "ed", "s"] {
            let suffix: Vec<char> = suffix.chars().collect();
            if lower.ends_with(&suffix) {
                return lower[..lower.len() - suffix.len()].to_vec();
            }
        }
    }
    lower
}

/// One character lowercased to one character, so offsets survive.
fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Where each needle starts in `text` (already lowercased), in order.
fn starts(text: &[char], needle: &[char]) -> Vec<usize> {
    if needle.is_empty() || needle.len() > text.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= text.len() {
        if text[i..i + needle.len()] == *needle {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

/// How often the needles occur in the title and the indexed words.
fn matches(title: &str, body: &str, stems: &[Vec<char>]) -> u32 {
    let read = knobas_source::docs::indexed::read(body);
    // A source that opens `body_text` with the title (Confluence does) has
    // counted it already.
    let mut all = if body.starts_with(title) {
        String::new()
    } else {
        title.to_owned()
    };
    for run in &read.runs {
        all.push('\n');
        if let knobas_source::docs::indexed::Part::Section(heading) = &run.part {
            all.push_str(heading);
            all.push('\n');
        }
        all.push_str(&run.text);
    }
    let lowered: Vec<char> = all.chars().map(lower).collect();
    let count: usize = stems.iter().map(|s| starts(&lowered, s).len()).sum();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The best sentence for a hit (R-10.3): the one holding the most needles,
/// from a section or a comment, cut around its first match and prefixed with
/// where it is. With no needle anywhere (a hit by its title alone), the first
/// sentence of the page.
fn snippet(title: &str, body: &str, stems: &[Vec<char>]) -> DocSnippet {
    use knobas_source::docs::indexed::Part;
    let read = knobas_source::docs::indexed::read(body);
    let mut best: Option<(usize, String, &Part)> = None;
    let mut first: Option<(String, &Part)> = None;
    for run in &read.runs {
        let text = match &run.part {
            // The lead opens with the page's title, which is not a sentence
            // of the page.
            Part::Lead => run
                .text
                .strip_prefix(title)
                .unwrap_or(&run.text)
                .trim()
                .to_owned(),
            _ => run.text.clone(),
        };
        for sentence in sentences(&text) {
            if first.is_none() {
                first = Some((sentence.clone(), &run.part));
            }
            let lowered: Vec<char> = sentence.chars().map(lower).collect();
            let k = stems
                .iter()
                .filter(|s| !starts(&lowered, s).is_empty())
                .count();
            if k > 0 && best.as_ref().is_none_or(|(b, _, _)| k > *b) {
                best = Some((k, sentence, &run.part));
            }
        }
    }
    let (sentence, part) = match best {
        Some((_, sentence, part)) => (sentence, Some(part)),
        None => match first {
            Some((sentence, part)) => (sentence, Some(part)),
            None => (String::new(), None),
        },
    };
    let (section, comment_by) = match part {
        Some(Part::Section(heading)) => (Some(heading.clone()), None),
        Some(Part::Comment(by)) => (None, Some(by.clone())),
        _ => (None, None),
    };
    DocSnippet {
        section,
        comment_by,
        segments: marked(&cut(&sentence, stems), stems),
    }
}

/// A run's sentences: split after `.`, `!` or `?` and at line ends.
fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut current = String::new();
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            current.push(c);
            if matches!(c, '.' | '!' | '?') && chars.peek().is_some_and(|n| n.is_whitespace()) {
                out.push(std::mem::take(&mut current).trim().to_owned());
            }
        }
        out.push(current.trim().to_owned());
    }
    out.retain(|s| !s.is_empty());
    out
}

/// At most 150 characters of `sentence`, starting a little before its first
/// match when that is far in, with `…` where it was cut (prototype-6).
fn cut(sentence: &str, stems: &[Vec<char>]) -> String {
    const SPAN: usize = 150;
    let chars: Vec<char> = sentence.chars().collect();
    let lowered: Vec<char> = chars.iter().copied().map(lower).collect();
    let at = stems
        .iter()
        .filter_map(|s| starts(&lowered, s).first().copied())
        .min();
    let start = match at {
        Some(at) if at > 60 => chars[..at - 50]
            .iter()
            .rposition(|c| *c == ' ')
            .map_or(0, |space| space + 1),
        _ => 0,
    };
    let end = (start + SPAN).min(chars.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(&chars[start..end]);
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// `text` as segments, each needle's occurrence and the rest of its word
/// marked as a hit.
fn marked(text: &str, stems: &[Vec<char>]) -> Vec<knobas_search::Segment> {
    let chars: Vec<char> = text.chars().collect();
    let lowered: Vec<char> = chars.iter().copied().map(lower).collect();
    let mut hit = vec![false; chars.len()];
    for s in stems {
        for at in starts(&lowered, s) {
            let mut end = at + s.len();
            while end < chars.len() && chars[end].is_alphanumeric() {
                end += 1;
            }
            hit[at..end].iter_mut().for_each(|h| *h = true);
        }
    }
    let mut out: Vec<knobas_search::Segment> = Vec::new();
    for (c, h) in chars.into_iter().zip(hit) {
        match out.last_mut() {
            Some(last) if last.hit == h => last.text.push(c),
            _ => out.push(knobas_search::Segment {
                text: c.to_string(),
                hit: h,
            }),
        }
    }
    out
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use knobas_source::docs::indexed::{comment_line, link_line, section_line};

    fn plain(segments: &[knobas_search::Segment]) -> String {
        segments
            .iter()
            .map(|s| {
                if s.hit {
                    format!("[{}]", s.text)
                } else {
                    s.text.clone()
                }
            })
            .collect()
    }

    fn stems(needles: &[&str]) -> Vec<Vec<char>> {
        needles.iter().map(|n| stem(n)).collect()
    }

    fn body() -> String {
        [
            "SEPA payout retry design".to_owned(),
            "Why we built it.".to_owned(),
            section_line("Backoff policy"),
            "Base 30 s, factor 2. Retries stop after five attempts and the batch is parked."
                .to_owned(),
            comment_line(Some("Priya Nair")),
            "Should the retry cap be six attempts?".to_owned(),
            link_line("ENG", "Retry budget"),
        ]
        .join("\n")
    }

    #[test]
    fn stemming_is_prototype_sixes() {
        assert_eq!(stem("Reviews"), "review".chars().collect::<Vec<_>>());
        assert_eq!(stem("retrying"), "retry".chars().collect::<Vec<_>>());
        assert_eq!(stem("cases"), "cas".chars().collect::<Vec<_>>());
        assert_eq!(stem("runs"), "runs".chars().collect::<Vec<_>>());
    }

    #[test]
    fn the_snippet_is_the_sentence_with_the_most_needles_and_says_its_section() {
        let s = snippet(
            "SEPA payout retry design",
            &body(),
            &stems(&["retries", "attempts"]),
        );
        assert_eq!(s.section.as_deref(), Some("Backoff policy"));
        assert_eq!(s.comment_by, None);
        assert_eq!(
            plain(&s.segments),
            "[Retries] stop after five [attempts] and the batch is parked."
        );
    }

    #[test]
    fn a_comment_is_a_snippet_by_its_author() {
        let s = snippet("SEPA payout retry design", &body(), &stems(&["cap", "six"]));
        assert_eq!(s.comment_by.as_deref(), Some("Priya Nair"));
        assert_eq!(s.section, None);
        assert_eq!(
            plain(&s.segments),
            "Should the retry [cap] be [six] attempts?"
        );
    }

    #[test]
    fn a_hit_by_title_alone_shows_the_first_sentence_not_the_title() {
        let s = snippet("SEPA payout retry design", &body(), &stems(&["sepa"]));
        assert_eq!(s.section, None);
        assert_eq!(plain(&s.segments), "Why we built it.");
    }

    #[test]
    fn a_far_match_is_cut_around_with_ellipses() {
        let long = format!(
            "{} retry {}",
            "word ".repeat(30).trim(),
            "tail ".repeat(40).trim()
        );
        let s = cut(&long, &stems(&["retry"]));
        assert!(s.starts_with('…'), "{s}");
        assert!(s.ends_with('…'), "{s}");
        assert!(s.contains("retry"), "{s}");
        assert!(s.chars().count() <= 152, "{s}");
    }

    #[test]
    fn matches_count_title_sections_and_comments_but_not_link_lines() {
        // title 1 and comment 1 ("Retries" stems apart); the link line's "Retry
        // budget" is not text.
        assert_eq!(
            matches("SEPA payout retry design", &body(), &stems(&["retry"])),
            2
        );
    }
}
