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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocsTree {
    /// Ordered by source, then key.
    pub spaces: Vec<DocSpace>,
    /// The page the reader opened most recently (`visit_entity`), while it is
    /// still a live page; `#/docs` opens it.
    pub last_read: Option<String>,
    /// *For you* (#860, R-10.2): the pages whose mention of the reader is in
    /// the inbox now, newest first.
    pub for_you: Vec<DocForYou>,
}

/// A page that needs the reader, and who asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocForYou {
    pub entity_id: String,
    pub title: String,
    pub space_key: Option<String>,
    /// The display name of who mentioned the reader: the newest comment that
    /// does, else the page's editor when the body does. `None` where the
    /// record does not say (a mention typed as a plain `@name`).
    pub by_name: Option<String>,
}

/// One space and its pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocSpace {
    pub source_id: String,
    pub key: String,
    /// The space's name, `None` where no page of it carries one knobas can
    /// read.
    pub name: Option<String>,
    /// Every live page of the space, in the source's sibling order where it
    /// gives one, else by title (#953).
    pub pages: Vec<DocNode>,
}

/// One page in a space's tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocNode {
    pub entity_id: String,
    pub title: String,
    /// The page above it, where its declared parent is a mirrored page of the
    /// same space; `None` for a page at the top of what the mirror holds.
    pub parent: Option<String>,
}

/// One page, as the reader draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
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
    ///
    /// In the source's account spelling.
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
    /// The comment writes to this page that settled as sent with an id the
    /// source named for the comment, oldest first (#2074). Docs drops a
    /// queued comment once [`DocPage::comments`] holds a comment of that id,
    /// whatever its words say by then; a mark whose write is not here falls
    /// back to the words.
    pub sent_comments: Vec<DocSentComment>,
    /// The identity `@me` resolves to, so the reader can say *You* for the
    /// editor and a comment's author.
    pub me: Vec<String>,
    /// The page's sections, numbered as the anchors are, each with its text
    /// or why knobas does not edit it (#862).
    pub sections: Vec<knobas_source::docs::DocSection>,
    /// The versions the record names, newest first (#861).
    pub history: Vec<knobas_source::docs::DocVersion>,
    /// The live pages of the same source whose body links to this one, by
    /// title (#861, *Pages that link here*).
    pub backlinks: Vec<DocBacklink>,
    /// The tickets the page references or is linked to in knobas, with their
    /// status now (#861, *Work it belongs to*).
    pub work: Vec<DocWork>,
    /// Which Docs writes this page's source takes: `comment`, `update_page`,
    /// `create_page`, each where its adapter declares it and (for the two
    /// page writes) writes the source's markup (#862).
    #[ts(as = "Vec<DocWrite>")]
    pub writes: Vec<String>,
}

/// A comment write knobas sent to a page, and the id the source gave the
/// comment (`knobas.write_queue.remote_id`, stamped by the settle from the
/// write's receipt): the pair a queued comment is matched to its comment by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocSentComment {
    /// The queued write's id, the one `submit_write` answered.
    pub write_id: i64,
    /// The source's id for the comment it made: [`DocComment::id`].
    pub comment_id: String,
}

/// A page that links to the one being read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocBacklink {
    pub entity_id: String,
    pub title: String,
    pub space_key: Option<String>,
}

/// A ticket a page belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocWork {
    pub entity_id: String,
    /// The entity id past its namespace (`PAY-231`): the job's address.
    pub key: String,
    pub title: String,
    pub status: Option<String>,
    /// Whether a knobas link draws it (else the body references it).
    pub linked: bool,
}

/// A space, named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocSpaceRef {
    pub key: String,
    pub name: Option<String>,
}

/// One step of a page's parent chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocAncestor {
    pub title: String,
    pub entity_id: Option<String>,
}

/// The live pages with their space and their declared direct parent's name,
/// each space's pages in the source's own sibling order (the declared
/// `position`, #953) and by title where the source gives none. Ordering the
/// whole space by it orders every parent's children, because the reader picks
/// a parent's children out of the list in list order. `$1` is the
/// declarations, `$2` the wiki role's fillers: the tree is the wiki's by
/// definition, so a space of any other source is not in it (#1296,
/// ADR-0029 as amended on 2026-10-19).
///
/// The pages are read from `sync.item` by their kind, live by
/// [`knobas_core::live_by_exclusion!`], not through `sync.live_item` (#981).
/// Through the view, on tables never analyzed, the statement was planned from
/// `sync.item_project` -- a row per item of every kind -- with a probe into
/// `knobas.entity` and `sync.item` per row and the kind filtered after the
/// probe: 211 ms on the probe's 22,000 items for 2,000 pages, against 22 ms
/// this way (31 and 21 ms analyzed).
const PAGES: &str = concat!(
    "select i.entity_id, i.title, p.source_id, p.key, p.name, \
       (select pn.name from ",
    knobas_core::declared_parent_names!("$1"),
    " pn order by pn.pos desc, pn.name_ord limit 1) as parent \
       from sync.item i \
       join sync.item_project p on p.entity_id = i.entity_id \
       join knobas.source_config s on s.id = p.source_id and s.kind = p.adapter_kind \
      where i.kind = 'page' and i.source_id = any($2::text[]) and ",
    knobas_core::live_by_exclusion!("i"),
    " order by p.source_id, p.key, ",
    knobas_core::declared_integer!("$1", "position"),
    " nulls last, lower(i.title), i.entity_id"
);

/// The tree's page statement, for the plan test (`tests/it/read_plans.rs`,
/// #981). It binds what [`tree`] binds.
#[doc(hidden)]
#[must_use]
pub fn planned_tree() -> &'static str {
    PAGES
}

/// The page read last, among the tree's: `$2` is the wiki role's fillers.
const LAST_READ: &str = "\
select a.entity_id from knobas.activity a
  join sync.live_item i on i.entity_id = a.entity_id
 where a.verb = $1 and a.actor = 'user' and i.kind = 'page'
   and i.source_id = any($2::text[])
 order by a.at desc, a.id desc
 limit 1";

/// The spaces with their trees, the page read last and *For you*, each
/// narrowed to `fillers`, the sources filling the wiki role (#1296): a page
/// of any other source is in no space, is never the page read last and is
/// never *For you*.
///
/// # Errors
///
/// `internal` for a query failure.
pub async fn tree(
    pool: &PgPool,
    declarations: &Declarations,
    project: impl Fn(&str) -> Option<knobas_source::docs::DocProjection>,
    me: &[String],
    fillers: &[String],
) -> Result<DocsTree, IpcError> {
    let rows = sqlx::query(PAGES)
        .bind(declarations.as_param())
        .bind(fillers)
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
        .bind(fillers)
        .fetch_optional(pool)
        .await?;
    let for_you = for_you(pool, declarations, project, me, fillers).await?;
    Ok(DocsTree {
        spaces,
        last_read,
        for_you,
    })
}

/// The rows *For you* reads its pages from. `$1` the ids, `$2` the wiki
/// role's fillers.
const FOR_YOU_ROWS: &str = "\
select i.entity_id, i.payload, s.kind as adapter_kind, p.key
  from sync.live_item i
  join knobas.source_config s on s.id = i.source_id
  left join sync.item_project p on p.entity_id = i.entity_id and p.adapter_kind = s.kind
 where i.entity_id = any($1) and i.source_id = any($2::text[])";

/// *For you*: the inbox's mentions of the reader that are pages, as the
/// inbox has them now (a mention marked done or snoozed there is not here),
/// each with who mentioned the reader. Only the pages of `fillers`, as the
/// tree holds only theirs: a mention whose page no filler holds is left out.
async fn for_you(
    pool: &PgPool,
    declarations: &Declarations,
    project: impl Fn(&str) -> Option<knobas_source::docs::DocProjection>,
    me: &[String],
    fillers: &[String],
) -> Result<Vec<DocForYou>, IpcError> {
    use knobas_core::inbox::{Category, Shelf, items_from, rule};
    let Some(mention) = rule(Category::Mention) else {
        return Ok(Vec::new());
    };
    let items: Vec<_> = items_from(pool, mention, me, Utc::now(), Shelf::Stream, declarations)
        .await
        .map_err(IpcError::internal)?
        .into_iter()
        .filter(|item| item.kind.as_deref() == Some(PAGE_KIND))
        .filter_map(|item| item.entity_id.map(|id| (id, item.title)))
        .collect();
    let ids: Vec<&String> = items.iter().map(|(id, _)| id).collect();
    let rows: HashMap<String, sqlx::postgres::PgRow> = sqlx::query(FOR_YOU_ROWS)
        .bind(&ids)
        .bind(fillers)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| (r.get::<String, _>("entity_id"), r))
        .collect();
    Ok(items
        .into_iter()
        .filter_map(|(entity_id, title)| {
            let row = rows.get(&entity_id)?;
            let payload: serde_json::Value = row.get("payload");
            let by_name = project(&row.get::<String, _>("adapter_kind"))
                .and_then(|project| project(PAGE_KIND, &payload))
                .and_then(|content| mentioner(&content, me));
            Some(DocForYou {
                space_key: row.get("key"),
                entity_id,
                title,
                by_name,
            })
        })
        .collect())
}

/// Who mentioned one of `me` in a page: the newest comment that does, else
/// the page's editor when the body does.
fn mentioner(content: &knobas_source::docs::DocContent, me: &[String]) -> Option<String> {
    let mentions = |blocks: &[DocBlock]| {
        let mut blocks = blocks.to_vec();
        knobas_source::docs::inlines_mut(&mut blocks)
            .into_iter()
            .any(|i| matches!(i, Inline::Mention { user: Some(user), .. } if me.contains(user)))
    };
    content
        .comments
        .iter()
        .rev()
        .find(|c| mentions(&c.blocks))
        .and_then(|c| c.author_name.clone().or_else(|| c.author.clone()))
        .or_else(|| {
            mentions(&content.blocks)
                .then(|| content.editor_name.clone())
                .flatten()
        })
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

/// The tickets a page is linked to in knobas (confirmed links, either
/// direction), with their declared status. `$1` the page, `$2` declarations.
const LINKED_TICKETS: &str = concat!(
    "select i.entity_id, substr(i.entity_id, strpos(i.entity_id, ':') + 1) as key, i.title, ",
    knobas_core::declared_string!("$2", "status_name"),
    " as status from knobas.confirmed_link l \
       join sync.live_item i on i.entity_id = case when l.from_id = $1 then l.to_id else l.from_id end \
      where (l.from_id = $1 or l.to_id = $1) and i.kind = 'ticket' \
      order by l.created_at, i.entity_id"
);

/// The comment writes to the page `$1` that settled with a stamped id. Read off the
/// `comment` op alone, so a comment id is never taken for an entity's.
const SENT_COMMENTS: &str = "select id as write_id, remote_id as comment_id
  from knobas.write_queue
 where entity_id = $1 and op = 'comment' and state = 'sent' and remote_id is not null
 order by id";

/// The live pages of one source whose `body_text` carries a link line to
/// this page (`knobas_source::docs::indexed::link_line`, written at sync).
/// The title's words narrow the scan through the full-text index first: a
/// link line holds the title, so every page that links here matches them.
/// `$1` source, `$2` the page, `$3` its title, `$4` its link line.
const BACKLINKS: &str = "\
select i.entity_id, i.title, p.key
  from sync.live_item i
  cross join (select phraseto_tsquery('english', $3) as q) t
  left join sync.item_project p on p.entity_id = i.entity_id
 where i.kind = 'page' and i.source_id = $1 and i.entity_id <> $2
   and (numnode(t.q) = 0 or i.fts @@ t.q)
   and strpos(i.body_text, $4) > 0
 order by lower(i.title), i.entity_id
 limit 50";

async fn backlinks(
    pool: &PgPool,
    source_id: &str,
    entity_id: &str,
    space_key: &str,
    title: &str,
) -> Result<Vec<DocBacklink>, IpcError> {
    let line = knobas_source::docs::indexed::link_line(space_key, title);
    Ok(sqlx::query(BACKLINKS)
        .bind(source_id)
        .bind(entity_id)
        .bind(title)
        .bind(&line)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| DocBacklink {
            entity_id: r.get("entity_id"),
            title: r.get("title"),
            space_key: r.get("key"),
        })
        .collect())
}

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
    writes: impl Fn(&str) -> Vec<String>,
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
        sections: Vec::new(),
        history: Vec::new(),
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

    let title: String = row.get("title");
    let backlinks = match &here {
        Some(key) => backlinks(pool, &source_id, &id, key, &title).await?,
        None => Vec::new(),
    };
    let mut work: Vec<DocWork> = Vec::new();
    for inline in every_inline(&mut content) {
        if let Inline::Issue {
            key,
            entity_id: Some(ticket),
            title: Some(summary),
            status,
        } = &*inline
            && !work.iter().any(|w| w.entity_id == *ticket)
        {
            work.push(DocWork {
                entity_id: ticket.clone(),
                key: key.clone(),
                title: summary.clone(),
                status: status.clone(),
                linked: false,
            });
        }
    }
    for r in sqlx::query(LINKED_TICKETS)
        .bind(&id)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?
    {
        let ticket: String = r.get("entity_id");
        match work.iter_mut().find(|w| w.entity_id == ticket) {
            Some(known) => known.linked = true,
            None => work.push(DocWork {
                key: r.get("key"),
                title: r.get("title"),
                status: r.get("status"),
                linked: true,
                entity_id: ticket,
            }),
        }
    }

    let sent_comments = if readable {
        sqlx::query(SENT_COMMENTS)
            .bind(&id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| DocSentComment {
                write_id: r.get("write_id"),
                comment_id: r.get("comment_id"),
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(DocPage {
        sent_comments,
        writes: if readable {
            writes(&adapter_kind)
        } else {
            Vec::new()
        },
        sections: content.sections,
        history: content.history,
        backlinks,
        work,
        entity_id: id,
        source_id,
        source_name: row.get("source_name"),
        title,
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
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
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
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
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
///
/// Source text, never markup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocSnippet {
    /// The section heading it sits under, `None` before the first one.
    pub section: Option<String>,
    /// The comment's author's display name when it is a comment's; `Some("")`
    /// for a comment whose record named nobody.
    pub comment_by: Option<String>,
    #[ts(inline)]
    pub segments: Vec<knobas_search::Segment>,
}

/// The rows a set of hits draws from. `$1` the ids, `$2` the wiki role's
/// fillers: a hit on any other source's page has no row, so it is not drawn.
const HIT_ROWS: &str = "\
select i.entity_id, i.body_text, i.author, i.payload, s.kind as adapter_kind, p.key, p.name
  from sync.live_item i
  join knobas.source_config s on s.id = i.source_id
  left join sync.item_project p on p.entity_id = i.entity_id and p.adapter_kind = s.kind
 where i.entity_id = any($1) and i.source_id = any($2::text[])";

/// The Docs search (#860): the pages `raw` admits, in `scope` when given,
/// each with its snippet, match count, space, path and editor. Only the pages
/// of `fillers`, the sources filling the wiki role, as the tree holds only
/// theirs (#1296): a hit on another source's page is left out and is not in
/// `total` either.
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
    fillers: &[String],
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
    let rows = sqlx::query(HIT_ROWS)
        .bind(&ids)
        .bind(fillers)
        .fetch_all(pool)
        .await?;
    let by_id: HashMap<String, sqlx::postgres::PgRow> = rows
        .into_iter()
        .map(|r| (r.get::<String, _>("entity_id"), r))
        .collect();
    let stems: Vec<Vec<char>> = found.needles.iter().map(|n| stem(n)).collect();
    let hits: Vec<DocHit> = found
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
    // A hit with no row is not drawn, so it is not counted either.
    let left_out = found.hits.len() - hits.len();
    Ok(Some(DocsHits {
        total: found
            .total
            .saturating_sub(left_out.try_into().unwrap_or(u32::MAX)),
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

/// The Docs writes a page can offer, in [`DocPage::writes`]' order.
pub const DOC_WRITES: [&str; 3] = ["comment", "update_page", "create_page"];

/// [`DOC_WRITES`] as the generated TypeScript's `DocWrite` union (#1326): a
/// Docs write a page's source takes, one entry of [`DocPage::writes`]. Never
/// built: the wire carries the op identifier as a string, and this enum only
/// names the union. `the_doc_write_union_is_the_list` pins it to the list.
#[derive(ts_rs::TS)]
#[ts(export_to = "entity.ts", rename_all = "snake_case")]
pub enum DocWrite {
    Comment,
    UpdatePage,
    CreatePage,
}

/// Which Docs writes an adapter kind takes: `comment` where it declares it,
/// `update_page` and `create_page` where it declares them **and** writes its
/// own markup (`has_markup`, the registry's `doc_writes`).
#[must_use]
pub fn docs_writes(
    templates: &[knobas_source::SourceDescriptor],
    has_markup: bool,
    kind: &str,
) -> Vec<String> {
    let Some(template) = templates.iter().find(|t| t.adapter_kind == kind) else {
        return Vec::new();
    };
    DOC_WRITES
        .into_iter()
        .filter(|op| template.write_ops.iter().any(|w| w == op))
        .filter(|op| *op == "comment" || has_markup)
        .map(str::to_owned)
        .collect()
}

// -- Export as Markdown (#861) ----------------------------------------------

/// What *Export as Markdown* wrote.
///
/// Issue #861.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct DocExport {
    /// The file's name, `page_file_name(title)`.
    pub file: String,
    /// The folder it is in, absolute.
    pub folder: String,
}

/// A page as Markdown (R-10.17), through the one converter, which reads the
/// frame's own blocks (`knobas_source::docs::markdown`, #952): its title as
/// the heading, then its blocks.
#[must_use]
pub fn page_markdown(title: &str, blocks: &[DocBlock]) -> String {
    knobas_source::docs::markdown::page(title, blocks)
}

/// Write one page as Markdown into `folder`, under the first free name of
/// `<slug>.md`, `<slug> (1).md`, … (#1454): a file already there is never
/// replaced, and the answer names the file written.
///
/// # Errors
///
/// As [`page`]; `internal` if the file cannot be written, which leaves no
/// file behind.
pub async fn export(
    pool: &PgPool,
    declarations: &Declarations,
    project: impl Fn(&str) -> Option<knobas_source::docs::DocProjection>,
    entity_id: &str,
    folder: &std::path::Path,
) -> Result<DocExport, IpcError> {
    let page = page(pool, declarations, project, |_| Vec::new(), &[], entity_id).await?;
    if !page.readable {
        return Err(IpcError::invalid(format!(
            "knobas cannot read the body of {}, so it has nothing to export",
            page.title
        )));
    }
    let name = knobas_source::docs::markdown::page_file_name(&page.title);
    tokio::fs::create_dir_all(folder)
        .await
        .map_err(IpcError::internal)?;
    let (path, mut out) = crate::downloads::create_free(folder, &name)
        .await
        .map_err(|(path, e)| IpcError::internal(format!("writing {}: {e}", path.display())))?;
    let text = page_markdown(&page.title, &page.blocks);
    if let Err(e) = write_whole(&mut out, text.as_bytes()).await {
        drop(out);
        let _ = tokio::fs::remove_file(&path).await;
        return Err(IpcError::internal(e));
    }
    Ok(DocExport {
        file: path
            .file_name()
            .map_or(name, |n| n.to_string_lossy().into_owned()),
        folder: folder.display().to_string(),
    })
}

async fn write_whole(out: &mut tokio::fs::File, bytes: &[u8]) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    out.write_all(bytes).await?;
    out.flush().await
}

// -- Edit section and New page (#862) -----------------------------------------

/// The page row a write is made from. `$1` the id.
const WRITE_ROW: &str = "\
select i.kind, i.source_id, i.payload, s.kind as adapter_kind
  from sync.live_item i
  join knobas.source_config s on s.id = i.source_id
 where i.entity_id = $1";

/// The `UpdatePage` a section edit queues (R-10.13): section `section` of the
/// mirrored page, its body replaced by `text` in the source's markup, against
/// the version the reader read (`version`). Refused when the mirror has moved
/// past that version since, when the section is one knobas does not edit, or
/// when the source writes no pages.
///
/// # Errors
///
/// `invalid` for each refusal above, `not_found` for a page the live mirror
/// does not hold, `internal` for a query failure.
pub async fn section_edit(
    pool: &PgPool,
    writes: impl Fn(&str) -> Option<knobas_source::docs::DocWrites>,
    entity_id: &str,
    section: u32,
    version: Option<u64>,
    text: &str,
) -> Result<serde_json::Value, IpcError> {
    use knobas_source::docs::{EditRefused, SectionRefusal};
    let row = sqlx::query(WRITE_ROW)
        .bind(entity_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("{entity_id} is not in the mirror")))?;
    let kind: String = row.get("kind");
    let adapter_kind: String = row.get("adapter_kind");
    let payload: serde_json::Value = row.get("payload");
    let Some(doc) = writes(&adapter_kind) else {
        return Err(IpcError::invalid(format!(
            "knobas does not edit pages of a {adapter_kind} source"
        )));
    };
    let edit = (doc.edit_section)(&kind, &payload, section, text).map_err(|refused| {
        IpcError::invalid(match refused {
            EditRefused::Section(SectionRefusal::Table) => "This section has a table; edit it in the source.",
            EditRefused::Section(SectionRefusal::Code) => "This section has code; edit it in the source.",
            EditRefused::Section(SectionRefusal::Macro) => "This section has a macro; edit it in the source.",
            EditRefused::Section(SectionRefusal::Formatting) => {
                "This section has formatting or links that a text edit would lose; edit it in the source."
            }
            EditRefused::Missing => "The page has no such section any more. Reopen it and try again.",
        })
    })?;
    if version.is_some_and(|v| i64::try_from(v).ok() != Some(edit.base_version)) {
        return Err(IpcError::invalid(
            "The page changed since you opened it. Reopen it and edit the section again.",
        ));
    }
    Ok(serde_json::json!({
        "UpdatePage": {
            "entity": entity_id,
            "base_version": edit.base_version,
            "body": edit.body,
        }
    }))
}

/// What *New page* asks for (R-10.16).
///
/// Issue #862.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct NewPage {
    /// The page it goes under, a mirrored page of the space.
    pub parent: String,
    pub title: String,
    /// The editor's plain text: `## ` a section, `- ` a list item.
    pub text: String,
}

/// The `CreatePage` *New page* queues: under a mirrored page, in that page's
/// space, its body the text in the source's markup.
///
/// # Errors
///
/// `invalid` for a blank title, a parent with no space, or a source that
/// writes no pages; `not_found` for a parent the live mirror does not hold.
pub async fn create_page(
    pool: &PgPool,
    writes: impl Fn(&str) -> Option<knobas_source::docs::DocWrites>,
    new: &NewPage,
) -> Result<(serde_json::Value, String), IpcError> {
    let title = new.title.trim();
    if title.is_empty() {
        return Err(IpcError::invalid("A new page needs a title."));
    }
    let row = sqlx::query(
        "select i.kind, i.source_id, s.kind as adapter_kind, p.key from sync.live_item i \
           join knobas.source_config s on s.id = i.source_id \
           left join sync.item_project p on p.entity_id = i.entity_id and p.adapter_kind = s.kind \
          where i.entity_id = $1",
    )
    .bind(&new.parent)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| IpcError::not_found(format!("{} is not in the mirror", new.parent)))?;
    let kind: String = row.get("kind");
    if kind != PAGE_KIND {
        return Err(IpcError::invalid(format!(
            "{} is a {kind}, not a page",
            new.parent
        )));
    }
    let adapter_kind: String = row.get("adapter_kind");
    let source_id: String = row.get("source_id");
    let Some(space) = row.get::<Option<String>, _>("key") else {
        return Err(IpcError::invalid(format!(
            "knobas does not know which space {} is in",
            new.parent
        )));
    };
    let Some(doc) = writes(&adapter_kind) else {
        return Err(IpcError::invalid(format!(
            "knobas does not create pages in a {adapter_kind} source"
        )));
    };
    Ok((
        serde_json::json!({
            "CreatePage": {
                "parent": new.parent,
                "space": space,
                "title": title,
                "body": (doc.body_from_text)(&new.text),
            }
        }),
        source_id,
    ))
}

/// The page a queued `CreatePage` made, once the write has settled and the
/// mirror holds the page: the source's id for it (`knobas.write_queue.
/// remote_id`, stamped by the settle) in the write's source. Matched by that
/// id, never by title, so a same-titled page someone else made is not it.
/// `None` while there is nothing to link yet.
///
/// The look-up *Link it to <job>* resolves through
/// (`sources::write_queue::link_on_arrival`, #1121).
///
/// # Errors
///
/// `internal` if a read fails.
pub async fn created_page(pool: &PgPool, write_id: i64) -> Result<Option<String>, IpcError> {
    let found: Option<(String, Option<String>)> =
        sqlx::query_as("select source_id, remote_id from knobas.write_queue where id = $1")
            .bind(write_id)
            .fetch_optional(pool)
            .await?;
    let Some((source_id, Some(remote))) = found else {
        return Ok(None);
    };
    let page = format!("{source_id}:{remote}");
    let mirrored: Option<String> = sqlx::query_scalar("select id from knobas.entity where id = $1")
        .bind(&page)
        .fetch_optional(pool)
        .await?;
    Ok(mirrored)
}

/// *Export as Markdown*'s bytes, pinned over every block and inline the frame
/// has (#952): captured before the two block vocabularies became one, so the
/// change of vocabulary changed no output.
#[cfg(test)]
mod markdown_golden {
    use super::*;
    use knobas_source::docs::{PanelKind, TableCell};

    fn text(words: &str) -> Vec<Inline> {
        vec![Inline::text(words)]
    }

    fn marked(words: &str, strong: bool, emphasis: bool, code: bool) -> Inline {
        Inline::Text {
            text: words.to_owned(),
            strong,
            emphasis,
            code,
        }
    }

    fn para(words: &str) -> DocBlock {
        DocBlock::Paragraph { text: text(words) }
    }

    fn cell(header: bool, words: &str) -> TableCell {
        TableCell {
            header,
            text: text(words),
        }
    }

    fn panel(kind: PanelKind, title: Option<&str>, blocks: Vec<DocBlock>) -> DocBlock {
        DocBlock::Panel {
            kind,
            title: title.map(str::to_owned),
            blocks,
        }
    }

    fn fixture() -> Vec<DocBlock> {
        vec![
            DocBlock::Heading {
                level: 2,
                text: text("Retry\npolicy"),
            },
            DocBlock::Heading {
                level: 9,
                text: text("Deep"),
            },
            para("# not a heading"),
            para("12. not numbered"),
            DocBlock::Paragraph {
                text: vec![
                    marked("bold", true, false, false),
                    Inline::text(" "),
                    marked("soft", false, true, false),
                    Inline::text(" "),
                    marked("both_", true, true, false),
                    Inline::text(" "),
                    marked("a`b", false, false, true),
                    Inline::text(" "),
                    marked("`x", true, false, true),
                    Inline::Break,
                    Inline::Link {
                        text: "the *docs*".to_owned(),
                        href: "https://example.test/a b".to_owned(),
                    },
                    Inline::text(" "),
                    Inline::PageLink {
                        text: "ignored".to_owned(),
                        title: "Payout design: v2".to_owned(),
                        space: Some("ENG".to_owned()),
                        entity_id: Some("x".to_owned()),
                    },
                    Inline::text(" "),
                    Inline::Issue {
                        key: "PAY-231".to_owned(),
                        entity_id: None,
                        title: Some("t".to_owned()),
                        status: None,
                    },
                    Inline::text(" "),
                    Inline::Mention {
                        user: Some("priya".to_owned()),
                        name: Some("Priya Nair".to_owned()),
                        me: true,
                    },
                    Inline::text(" "),
                    Inline::Mention {
                        user: Some("mara_k".to_owned()),
                        name: None,
                        me: false,
                    },
                    Inline::text(" "),
                    Inline::Mention {
                        user: None,
                        name: None,
                        me: false,
                    },
                ],
            },
            DocBlock::List {
                ordered: false,
                items: vec![
                    vec![para("one")],
                    vec![
                        para("two"),
                        DocBlock::List {
                            ordered: true,
                            items: vec![vec![para("a")], vec![para("b")]],
                        },
                        panel(PanelKind::Tip, None, vec![para("inside")]),
                    ],
                ],
            },
            DocBlock::Code {
                code: "fn main() {}\n".to_owned(),
                language: Some(" rust ".to_owned()),
            },
            DocBlock::Code {
                code: "```\nnested\n```".to_owned(),
                language: None,
            },
            DocBlock::Table {
                rows: vec![
                    vec![cell(true, "Service"), cell(true, "Owner")],
                    vec![cell(false, "payouts|api"), cell(false, "Mara\nK")],
                    vec![cell(false, "ledger")],
                ],
            },
            DocBlock::Table {
                rows: vec![
                    vec![cell(true, "a"), cell(false, "b")],
                    vec![cell(false, "1"), cell(false, "2"), cell(false, "3")],
                ],
            },
            DocBlock::Table { rows: vec![] },
            DocBlock::Table {
                rows: vec![vec![], vec![cell(false, "x")]],
            },
            panel(
                PanelKind::Warning,
                Some(" Before  you deploy "),
                vec![para("Drain the queue."), para("Then deploy.")],
            ),
            panel(PanelKind::Info, Some("Only a title"), vec![]),
            panel(PanelKind::Note, None, vec![]),
            panel(PanelKind::Note, Some(" "), vec![para("Body only")]),
            DocBlock::Placeholder {
                name: "jira--chart".to_owned(),
            },
        ]
    }

    #[test]
    fn every_block_and_inline_exports_as_before() {
        let md = page_markdown("Retry *budget*: [v2]", &fixture());
        let want = concat!(
            "# Retry \\*budget\\*: \\[v2\\]\n\n",
            "## Retry policy\n\n",
            "###### Deep\n\n",
            "\\# not a heading\n\n",
            "12\\. not numbered\n\n",
            "**bold** *soft* ***both\\_*** ``a`b`` `` `x `` ",
            "[the \\*docs\\*](<https://example.test/a b>) ",
            "[Payout design: v2](payout-design-v2.md) PAY-231 @Priya @mara\\_k @someone\n\n",
            "- one\n",
            "- two\n\n",
            "  1. a\n",
            "  2. b\n\n",
            "  > **Tip**\n",
            "  >\n",
            "  > inside\n\n",
            "```rust\nfn main() {}\n```\n\n",
            "````\n```\nnested\n```\n````\n\n",
            "| Service | Owner |\n",
            "| --- | --- |\n",
            "| payouts\\|api | Mara<br>K |\n",
            "| ledger |  |\n\n",
            "| a | b |  |\n",
            "| --- | --- | --- |\n",
            "| 1 | 2 | 3 |\n\n",
            "|  |\n",
            "| --- |\n",
            "| x |\n\n",
            "> **Warning** Before you deploy\n",
            ">\n",
            "> Drain the queue.\n",
            ">\n",
            "> Then deploy.\n\n",
            "> **Info** Only a title\n\n",
            "> **Note**\n\n",
            "> **Note**\n",
            ">\n",
            "> Body only\n\n",
            "<!-- macro: jira- -chart -->\n",
        );
        assert_eq!(md, want);
    }

    #[test]
    fn an_empty_page_is_its_title() {
        assert_eq!(page_markdown("Empty", &[]), "# Empty\n");
        assert_eq!(page_markdown("", &[]), "# \n");
    }
}

#[cfg(test)]
mod writes_tests {
    use super::*;

    /// The `DocWrite` union the generated TypeScript declares (#1326) is
    /// exactly [`DOC_WRITES`]: a write added to the list alone would reach
    /// the shell as a word its types do not have.
    #[test]
    fn the_doc_write_union_is_the_list() {
        let union = DOC_WRITES
            .iter()
            .map(|write| format!("\"{write}\""))
            .collect::<Vec<_>>()
            .join(" | ");
        assert_eq!(
            <DocWrite as ts_rs::TS>::inline(&ts_rs::Config::new()),
            union
        );
    }
}
