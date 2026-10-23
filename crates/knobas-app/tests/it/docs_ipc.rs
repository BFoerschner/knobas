//! The Docs reader's two reads (#859, ADR-0022) through their command bodies,
//! `docs_tree_inner` and `docs_page_inner`, over the embedded PostgreSQL with
//! Confluence pages stored the way the adapter stores them.
//!
//! This file's database is its own (#572). Every test configures a source id
//! nothing else holds, so the trees and pages it reads are its own. The page
//! read last is the newest visit in the whole database, so a test that asserts
//! it takes a database of its own ([`scratch`], #1907). The
//! identity `@me` resolves to includes `mara`, the username the Confluence
//! sources here are configured with.

use knobas_app::commands::entity::{
    create_link_inner, docs_create_page_payload, docs_edit_section_payload,
    docs_export_markdown_inner, docs_page_inner, docs_search_inner, docs_tree_inner,
    move_notes_payloads,
};
use knobas_app::docs::DocsScope;
use knobas_app::sources::Registry;
use knobas_source::docs::indexed::{comment_line, link_line, section_line};
use knobas_source::docs::{DocBlock, Inline};
use serde_json::{Value, json};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A database of its own, for a test that asserts a read answering one
/// database-wide value (`last_read` is the newest visit to any live page):
/// on the shared database another test's visit decides it by scheduling (#1907).
async fn scratch(label: &str) -> sqlx::PgPool {
    let scratch = knobas_db::test_util::scratch_database(label).await;
    scratch
        .pool(2)
        .await
        .expect("a pool on the scratch database")
}

/// A Confluence source nothing else in this database uses.
async fn confluence(pool: &sqlx::PgPool) -> String {
    let id = format!("conf{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    crate::support::source_config(
        pool,
        &id,
        "confluence",
        "Confluence",
        &serde_json::json!({"username": "mara"}),
    )
    .await;
    id
}

async fn item(
    pool: &sqlx::PgPool,
    id: &str,
    source: &str,
    kind: &str,
    title: &str,
    payload: Value,
) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, author, payload, \
           item_updated_at, web_url)
         values ($1, $2, $3, $4, '', 'tomasz', $5, now(), 'http://x/pages/1')",
    )
    .bind(id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

/// A Confluence page record in the shape `api::EXPAND` asks for: its space,
/// its ancestors (outermost first), its body.
fn page_payload(space: &str, ancestors: &[&str], storage: &str) -> Value {
    json!({
        "type": "page",
        "space": {"key": space, "name": format!("{space} space")},
        "ancestors": ancestors.iter().map(|t| json!({"id": "1", "title": t})).collect::<Vec<_>>(),
        "body": {"storage": {"value": storage}},
        "version": {"number": 7, "by": {"username": "tomasz", "userKey": "k-tw", "displayName": "Tomasz Wójcik"}},
        "metadata": {"labels": {"results": [{"name": "ops"}, {"name": "runbook"}]}},
        "children": {"comment": {"results": [{
            "body": {"storage": {"value": "<p>Thanks <ac:link><ri:user ri:username=\"mara\" /></ac:link></p>"}},
            "history": {"createdBy": {"username": "priya", "displayName": "Priya Nair", "userKey": "k-pn"},
                        "createdDate": "2026-08-19T10:00:00.000Z"}
        }]}}
    })
}

async fn page(
    pool: &sqlx::PgPool,
    source: &str,
    id: &str,
    title: &str,
    space: &str,
    ancestors: &[&str],
    storage: &str,
) -> String {
    let entity = format!("{source}:{id}");
    item(
        pool,
        &entity,
        source,
        "page",
        title,
        page_payload(space, ancestors, storage),
    )
    .await;
    entity
}

/// The tree is the declared parent chain: each page hangs under the innermost
/// name of its chain, a page whose parent the mirror does not hold sits at the
/// top, and each space is named from its pages.
#[tokio::test]
async fn the_tree_hangs_each_page_under_its_declared_parent() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let home = page(&pool, &src, "1", "Ops home", "OPS", &[], "<p>Home</p>").await;
    let runbooks = page(&pool, &src, "2", "Runbooks", "OPS", &["Ops home"], "").await;
    let restart = page(
        &pool,
        &src,
        "3",
        "Payout worker restart",
        "OPS",
        &["Ops home", "Runbooks"],
        "",
    )
    .await;
    let orphan = page(&pool, &src, "4", "Orphan", "OPS", &["Not synced"], "").await;
    let other = page(&pool, &src, "5", "Eng home", "ENG", &[], "").await;

    let tree = docs_tree_inner(&pool, &Registry::for_process())
        .await
        .unwrap();
    let mine: Vec<_> = tree.spaces.iter().filter(|s| s.source_id == src).collect();
    assert_eq!(
        mine.iter()
            .map(|s| (s.key.as_str(), s.name.as_deref()))
            .collect::<Vec<_>>(),
        vec![("ENG", Some("ENG space")), ("OPS", Some("OPS space"))]
    );
    assert_eq!(mine[0].pages.len(), 1);
    assert_eq!(mine[0].pages[0].entity_id, other);
    let parent_of = |id: &str| {
        mine[1]
            .pages
            .iter()
            .find(|p| p.entity_id == id)
            .unwrap_or_else(|| panic!("{id} is in the OPS tree"))
            .parent
            .clone()
    };
    assert_eq!(parent_of(&home), None);
    assert_eq!(parent_of(&runbooks), Some(home.clone()));
    assert_eq!(parent_of(&restart), Some(runbooks.clone()));
    assert_eq!(
        parent_of(&orphan),
        None,
        "a parent the mirror does not hold is no parent"
    );
    assert_eq!(
        mine[1]
            .pages
            .iter()
            .map(|p| p.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Ops home", "Orphan", "Payout worker restart", "Runbooks"],
        "ordered by title"
    );
}

/// Only live pages are in the tree: not a withdrawn one, and none of a source
/// the user turned off (#981 reads the pages without `sync.live_item`).
#[tokio::test]
async fn the_tree_holds_only_live_pages() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let kept = page(&pool, &src, "1", "Kept", "OPS", &[], "").await;
    let withdrawn = page(&pool, &src, "2", "Withdrawn", "OPS", &[], "").await;
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&withdrawn)
        .execute(&pool)
        .await
        .unwrap();
    let off = confluence(&pool).await;
    page(&pool, &off, "1", "Turned off", "OPS", &[], "").await;
    sqlx::query("update knobas.source_config set enabled = false where id = $1")
        .bind(&off)
        .execute(&pool)
        .await
        .unwrap();

    let tree = docs_tree_inner(&pool, &Registry::for_process())
        .await
        .unwrap();
    let pages: Vec<&str> = tree
        .spaces
        .iter()
        .filter(|s| s.source_id == src || s.source_id == off)
        .flat_map(|s| s.pages.iter().map(|p| p.entity_id.as_str()))
        .collect();
    assert_eq!(pages, vec![kept.as_str()]);
}

/// #953: a parent's children are in the source's own order, the declared
/// `position`, lowest first; a child the source gives no position follows,
/// by title.
#[tokio::test]
async fn children_are_in_the_sources_order_and_by_title_where_it_has_none() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    page(&pool, &src, "1", "Ops home", "OPS", &[], "").await;
    for (id, title, position) in [
        ("2", "Alpha", Some(1)),
        ("3", "Mid", None),
        ("4", "Zeta", Some(0)),
        ("5", "Beta", None),
    ] {
        let mut payload = page_payload("OPS", &["Ops home"], "");
        if let Some(position) = position {
            payload["position"] = json!(position);
        }
        item(&pool, &format!("{src}:{id}"), &src, "page", title, payload).await;
    }
    let tree = docs_tree_inner(&pool, &Registry::for_process())
        .await
        .unwrap();
    let space = tree
        .spaces
        .iter()
        .find(|s| s.source_id == src)
        .expect("the OPS space");
    let children: Vec<&str> = space
        .pages
        .iter()
        .filter(|p| p.parent.is_some())
        .map(|p| p.title.as_str())
        .collect();
    assert_eq!(children, vec!["Zeta", "Alpha", "Beta", "Mid"]);
}

/// `#/docs` opens the page read last: the newest visit to a live page.
#[tokio::test]
async fn the_page_read_last_is_the_newest_visit() {
    let pool = scratch("docs_ipc_last_read").await;
    let src = confluence(&pool).await;
    let first = page(&pool, &src, "1", "First", "OPS", &[], "").await;
    let second = page(&pool, &src, "2", "Second", "OPS", &[], "").await;
    knobas_app::conversation::visit(&pool, &first)
        .await
        .unwrap();
    knobas_app::conversation::visit(&pool, &second)
        .await
        .unwrap();
    assert_eq!(
        docs_tree_inner(&pool, &Registry::for_process())
            .await
            .unwrap()
            .last_read,
        Some(second)
    );
    knobas_app::conversation::visit(&pool, &first)
        .await
        .unwrap();
    assert_eq!(
        docs_tree_inner(&pool, &Registry::for_process())
            .await
            .unwrap()
            .last_read,
        Some(first)
    );
}

/// A page reads as the header fields, the adapter's blocks, and what only the
/// mirror knows filled in: the ancestors' pages, the page link's page, the
/// ticket chip's summary and status now, and which mention is the reader's.
#[tokio::test]
async fn a_page_reads_as_blocks_with_the_mirror_filled_in() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    // A Jira ticket in the mirror, with its status where Jira keeps it.
    let jira = format!("jira{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    crate::support::source_config(&pool, &jira, "jira", "Jira", &serde_json::json!({})).await;
    let key = format!(
        "D{}-77",
        uuid::Uuid::new_v4().simple().to_string()[..4]
            .to_uppercase()
            .replace(|c: char| c.is_ascii_digit(), "Q")
    );
    let ticket = format!("{jira}:{key}");
    item(
        &pool,
        &ticket,
        &jira,
        "ticket",
        "On-call runbook outdated",
        json!({"key": key, "fields": {"status": {"name": "In Progress"}}}),
    )
    .await;

    let home = page(&pool, &src, "1", "Ops home", "OPS", &[], "").await;
    let runbook = page(
        &pool,
        &src,
        "2",
        "Ledger reconciliation runbook",
        "OPS",
        &["Ops home"],
        "",
    )
    .await;
    let storage = format!(
        r#"<h2>Ledger</h2><p>See <ac:link><ri:page ri:content-title="Ledger reconciliation runbook" /></ac:link> and <ac:structured-macro ac:name="jira"><ac:parameter ac:name="key">{key}</ac:parameter></ac:structured-macro>, ask <ac:link><ri:user ri:username="mara" /></ac:link> or <ac:link><ri:user ri:userkey="k-tw" /></ac:link>.</p><ac:structured-macro ac:name="roadmap" />"#
    );
    let handbook = page(
        &pool,
        &src,
        "3",
        "On-call handbook",
        "OPS",
        &["Ops home"],
        &storage,
    )
    .await;

    let doc = docs_page_inner(&pool, &Registry::for_process(), &handbook)
        .await
        .unwrap();
    assert_eq!(doc.title, "On-call handbook");
    assert_eq!(
        doc.source_name, "Confluence",
        "the configured name, for Open in …"
    );
    assert!(doc.me.contains(&"mara".to_owned()));
    assert_eq!(
        doc.space
            .as_ref()
            .map(|s| (s.key.as_str(), s.name.as_deref())),
        Some(("OPS", Some("OPS space")))
    );
    assert_eq!(doc.ancestors.len(), 1);
    assert_eq!(doc.ancestors[0].title, "Ops home");
    assert_eq!(doc.ancestors[0].entity_id.as_deref(), Some(home.as_str()));
    assert_eq!(doc.editor.as_deref(), Some("tomasz"));
    assert_eq!(doc.editor_name.as_deref(), Some("Tomasz Wójcik"));
    assert_eq!(doc.web_url.as_deref(), Some("http://x/pages/1"));
    assert!(doc.readable);
    assert_eq!(doc.version, Some(7));
    assert_eq!(doc.labels, vec!["ops", "runbook"]);

    let DocBlock::Paragraph { text } = &doc.blocks[1] else {
        panic!("the second block is the paragraph: {:?}", doc.blocks)
    };
    let link = text.iter().find_map(|i| match i {
        Inline::PageLink { entity_id, .. } => Some(entity_id.clone()),
        _ => None,
    });
    assert_eq!(
        link,
        Some(Some(runbook)),
        "the page link names the mirrored page"
    );
    let chip = text.iter().find_map(|i| match i {
        Inline::Issue {
            entity_id,
            title,
            status,
            ..
        } => Some((entity_id.clone(), title.clone(), status.clone())),
        _ => None,
    });
    assert_eq!(
        chip,
        Some((
            Some(ticket),
            Some("On-call runbook outdated".to_owned()),
            Some("In Progress".to_owned())
        )),
        "the chip carries the ticket's summary and status from the mirror"
    );
    let mentions: Vec<(Option<String>, bool)> = text
        .iter()
        .filter_map(|i| match i {
            Inline::Mention { user, me, .. } => Some((user.clone(), *me)),
            _ => None,
        })
        .collect();
    assert_eq!(
        mentions,
        vec![
            (Some("mara".to_owned()), true),
            (Some("tomasz".to_owned()), false)
        ],
        "only the reader's own mention is marked"
    );
    assert_eq!(
        doc.blocks[2],
        DocBlock::Placeholder {
            name: "roadmap".into()
        }
    );

    assert_eq!(doc.comments.len(), 1);
    assert_eq!(doc.comments[0].author.as_deref(), Some("priya"));
    let DocBlock::Paragraph { text } = &doc.comments[0].blocks[0] else {
        panic!("a comment is a paragraph")
    };
    assert!(
        text.iter()
            .any(|i| matches!(i, Inline::Mention { me: true, .. })),
        "a comment's mention of the reader is marked too"
    );
}

/// A ticket chip for a ticket the mirror does not hold keeps its key and
/// nothing else; a page link to an unsynced page names no page.
#[tokio::test]
async fn what_the_mirror_does_not_hold_stays_unresolved() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let storage = r#"<p><ac:structured-macro ac:name="jira"><ac:parameter ac:name="key">NOPE-1</ac:parameter></ac:structured-macro> <ac:link><ri:page ri:content-title="Gone" /></ac:link></p>"#;
    let id = page(&pool, &src, "1", "Lonely", "OPS", &[], storage).await;
    let doc = docs_page_inner(&pool, &Registry::for_process(), &id)
        .await
        .unwrap();
    assert_eq!(
        doc.blocks,
        vec![DocBlock::Paragraph {
            text: vec![
                Inline::Issue {
                    key: "NOPE-1".into(),
                    entity_id: None,
                    title: None,
                    status: None
                },
                Inline::text(" "),
                Inline::PageLink {
                    text: "Gone".into(),
                    title: "Gone".into(),
                    space: None,
                    entity_id: None
                },
            ]
        }]
    );
}

/// Only a page is a page: another kind is `not_found`, and so is an id the
/// mirror does not hold; a non-id is `invalid`.
#[tokio::test]
async fn only_a_mirrored_page_reads() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let ticket = format!("{src}:T-1");
    item(&pool, &ticket, &src, "ticket", "Not a page", json!({})).await;
    use knobas_app::IpcErrorCode::{Invalid, NotFound};
    assert_eq!(
        docs_page_inner(&pool, &Registry::for_process(), &ticket)
            .await
            .unwrap_err()
            .code,
        NotFound
    );
    assert_eq!(
        docs_page_inner(&pool, &Registry::for_process(), &format!("{src}:404"))
            .await
            .unwrap_err()
            .code,
        NotFound
    );
    assert_eq!(
        docs_page_inner(&pool, &Registry::for_process(), "no colon")
            .await
            .unwrap_err()
            .code,
        Invalid
    );
}

/// Two pages that name each other as parent (a title reused after a move)
/// would have no top; each is put at the top instead, so both are drawn and no
/// walk of the tree loops.
#[tokio::test]
async fn a_parent_loop_is_cut_and_both_pages_sit_at_the_top() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let a = page(&pool, &src, "1", "Alpha", "OPS", &["Beta"], "").await;
    let b = page(&pool, &src, "2", "Beta", "OPS", &["Alpha"], "").await;
    let tree = docs_tree_inner(&pool, &Registry::for_process())
        .await
        .unwrap();
    let space = tree.spaces.iter().find(|s| s.source_id == src).unwrap();
    for id in [&a, &b] {
        let node = space.pages.iter().find(|p| p.entity_id == *id).unwrap();
        assert_eq!(node.parent, None, "{id} is cut loose from the loop");
    }
}

/// A page under a loop is not on it: once the loop is cut, it still hangs
/// under the page it names, which now sits at the top.
#[tokio::test]
async fn a_page_under_a_parent_loop_keeps_its_parent() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let a = page(&pool, &src, "1", "Alpha", "OPS", &["Beta"], "").await;
    let b = page(&pool, &src, "2", "Beta", "OPS", &["Alpha"], "").await;
    let c = page(&pool, &src, "3", "Gamma", "OPS", &["Alpha"], "").await;
    let tree = docs_tree_inner(&pool, &Registry::for_process())
        .await
        .unwrap();
    let space = tree.spaces.iter().find(|s| s.source_id == src).unwrap();
    let parent_of = |id: &str| {
        space
            .pages
            .iter()
            .find(|p| p.entity_id == id)
            .unwrap()
            .parent
            .clone()
    };
    assert_eq!(parent_of(&a), None);
    assert_eq!(parent_of(&b), None);
    assert_eq!(parent_of(&c), Some(a.clone()), "Gamma hangs under Alpha");
}

// -- the Docs search (#860) --------------------------------------------------

/// Give a page the `body_text` its adapter writes (title, sections, comments,
/// links), its editor and its age in days.
async fn indexed(pool: &sqlx::PgPool, id: &str, lines: &[String], author: &str, days: i32) {
    sqlx::query(
        "update sync.item set body_text = $2, author = $3, \
           item_updated_at = now() - make_interval(days => $4) where entity_id = $1",
    )
    .bind(id)
    .bind(lines.join("\n"))
    .bind(author)
    .bind(days)
    .execute(pool)
    .await
    .unwrap();
}

fn word() -> String {
    // A word no other test's page holds, so the whole mirror can be searched.
    format!("zq{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
}

fn scope(source: &str, key: &str) -> DocsScope {
    DocsScope {
        source_id: source.to_owned(),
        key: key.to_owned(),
    }
}

/// Every word must appear, the snippet is the best sentence and says its
/// section, and a hit carries its space, path, match count and editor.
#[tokio::test]
async fn a_search_finds_pages_with_every_word_and_says_where_the_snippet_is() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let w = word();
    let sepa = page(
        &pool,
        &src,
        "1",
        "SEPA payout retry design",
        "ENG",
        &["Payments"],
        "",
    )
    .await;
    indexed(
        &pool,
        &sepa,
        &[
            "SEPA payout retry design".into(),
            "Why we retry.".into(),
            section_line("Backoff policy"),
            format!("Retries stop after five {w} attempts. Then the batch is parked."),
            comment_line(Some("Priya Nair")),
            format!("Is {w} right?"),
        ],
        "tomasz",
        1,
    )
    .await;
    let other = page(&pool, &src, "2", "Unrelated", "ENG", &[], "").await;
    indexed(
        &pool,
        &other,
        &["Unrelated".into(), format!("Only {w} here.")],
        "priya",
        1,
    )
    .await;

    let found = docs_search_inner(
        &pool,
        &Registry::for_process(),
        &format!("{w} attempts"),
        None,
    )
    .await
    .unwrap()
    .expect("words are a query");
    assert_eq!(found.total, 1);
    assert_eq!(found.needles, [w.clone(), "attempts".to_owned()]);
    let hit = &found.hits[0];
    assert_eq!(hit.entity_id, sepa);
    assert_eq!(hit.space.as_ref().map(|s| s.key.as_str()), Some("ENG"));
    assert_eq!(hit.path.as_deref(), Some("Payments"));
    assert_eq!(hit.snippet.section.as_deref(), Some("Backoff policy"));
    let text: String = hit
        .snippet
        .segments
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(text, format!("Retries stop after five {w} attempts."));
    assert!(hit.snippet.segments.iter().any(|s| s.hit && s.text == w));
    assert_eq!(hit.matches, 3, "{w} twice, attempts once");
    assert_eq!(hit.editor.as_deref(), Some("tomasz"));
    assert_eq!(hit.editor_name.as_deref(), Some("Tomasz Wójcik"));

    // A comment's sentence is the snippet when it holds the most words.
    let found = docs_search_inner(&pool, &Registry::for_process(), &format!("{w} right"), None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        found.hits[0].snippet.comment_by.as_deref(),
        Some("Priya Nair")
    );
}

/// The picker keeps the search to its space; a typed `space:` wins over it;
/// `space:` is `project:` by another name; `-word`, `by:` and `updated:`
/// filter; a phrase is a phrase; a query of nothing is the tree's.
#[tokio::test]
async fn the_space_the_grammar_and_the_empty_query() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let w = word();
    let eng = page(&pool, &src, "1", "Eng page", "ENG", &[], "").await;
    indexed(
        &pool,
        &eng,
        &["Eng page".into(), format!("{w} backoff policy here.")],
        "tomasz",
        2,
    )
    .await;
    let ops = page(&pool, &src, "2", "Ops page", "OPS", &[], "").await;
    indexed(
        &pool,
        &ops,
        &["Ops page".into(), format!("{w} policy of backoff, cron.")],
        "priya",
        60,
    )
    .await;

    let ids = |hits: Option<knobas_app::docs::DocsHits>| -> Vec<String> {
        let mut ids: Vec<String> = hits
            .unwrap()
            .hits
            .into_iter()
            .map(|h| h.entity_id)
            .collect();
        ids.sort();
        ids
    };
    let mut both = vec![eng.clone(), ops.clone()];
    both.sort();
    assert_eq!(
        ids(docs_search_inner(&pool, &Registry::for_process(), &w, None)
            .await
            .unwrap()),
        both
    );
    assert_eq!(
        ids(docs_search_inner(
            &pool,
            &Registry::for_process(),
            &w,
            Some(&scope(&src, "ENG"))
        )
        .await
        .unwrap()),
        [eng.as_str()]
    );
    assert_eq!(
        ids(docs_search_inner(
            &pool,
            &Registry::for_process(),
            &format!("{w} space:OPS"),
            Some(&scope(&src, "ENG"))
        )
        .await
        .unwrap()),
        [ops.as_str()],
        "the typed space wins over the picker"
    );
    assert_eq!(
        ids(docs_search_inner(
            &pool,
            &Registry::for_process(),
            &format!("{w} project:ops"),
            None
        )
        .await
        .unwrap()),
        [ops.as_str()]
    );
    assert_eq!(
        ids(
            docs_search_inner(&pool, &Registry::for_process(), &format!("{w} -cron"), None)
                .await
                .unwrap()
        ),
        [eng.as_str()]
    );
    assert_eq!(
        ids(docs_search_inner(
            &pool,
            &Registry::for_process(),
            &format!("{w} \"backoff policy\""),
            None
        )
        .await
        .unwrap()),
        [eng.as_str()]
    );
    assert_eq!(
        ids(docs_search_inner(
            &pool,
            &Registry::for_process(),
            &format!("{w} by:priya"),
            None
        )
        .await
        .unwrap()),
        [ops.as_str()]
    );
    assert_eq!(
        ids(docs_search_inner(
            &pool,
            &Registry::for_process(),
            &format!("{w} updated:30d"),
            None
        )
        .await
        .unwrap()),
        [eng.as_str()]
    );
    assert!(
        ids(docs_search_inner(
            &pool,
            &Registry::for_process(),
            &format!("{w} type:ticket"),
            None
        )
        .await
        .unwrap())
        .is_empty()
    );
    assert!(
        docs_search_inner(&pool, &Registry::for_process(), "   ", None)
            .await
            .unwrap()
            .is_none()
    );
}

// -- the rail, the export and the writes (#861, #862) -------------------------

/// A Jira source and a ticket in it with a status, for *Work it belongs to*.
async fn ticket(pool: &sqlx::PgPool, title: &str, status: &str) -> (String, String) {
    let jira = format!("jira{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    crate::support::source_config(pool, &jira, "jira", "Jira", &serde_json::json!({})).await;
    // A Jira key is letters, then a number. Every hex digit stays distinct
    // (0-9 become G-P, a-f stay A-F): collapsing digits to one letter left
    // about one key in a hundred shared by two tickets of one test, and a
    // chip resolves by key alone, so it then found the other ticket (#1060).
    let letters: String = uuid::Uuid::new_v4().simple().to_string()[..8]
        .chars()
        .map(|c| match c.to_digit(10) {
            Some(d) => char::from(b'G' + d as u8),
            None => c.to_ascii_uppercase(),
        })
        .collect();
    let key = format!("W{letters}-1");
    let id = format!("{jira}:{key}");
    item(
        pool,
        &id,
        &jira,
        "ticket",
        title,
        json!({"key": key, "fields": {"status": {"name": status}}}),
    )
    .await;
    (id, key)
}

/// The rail: pages that link here (from the link lines written at sync, in
/// the same source, never itself), the work it belongs to (chips and knobas
/// links, with status), the history and the sections; and the writes the
/// source takes.
#[tokio::test]
async fn a_page_reads_with_its_rail_and_sections() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let (chip, chip_key) = ticket(&pool, "Retry failed payouts", "In Progress").await;
    let (linked, linked_key) = ticket(&pool, "Cap the budget", "Done").await;
    let storage = format!(
        "<h2>Text</h2><p>Plain.</p><h2>Refs</h2><p>See <ac:structured-macro ac:name=\"jira\">\
         <ac:parameter ac:name=\"key\">{chip_key}</ac:parameter></ac:structured-macro></p>"
    );
    let target = page(&pool, &src, "1", "Retry budget", "ENG", &[], &storage).await;
    create_link_inner(&pool, &target, &linked, Some("documents"), None)
        .await
        .unwrap();
    let from = page(&pool, &src, "2", "Payout design", "ENG", &[], "").await;
    indexed(
        &pool,
        &from,
        &[
            "Payout design".into(),
            "See Retry budget.".into(),
            link_line("ENG", "Retry budget"),
        ],
        "tomasz",
        1,
    )
    .await;
    // A link to a longer title that starts the same is not a link here.
    let near = page(&pool, &src, "3", "Near miss", "ENG", &[], "").await;
    indexed(
        &pool,
        &near,
        &[
            "Near miss".into(),
            "Retry budget v2.".into(),
            link_line("ENG", "Retry budget v2"),
        ],
        "tomasz",
        1,
    )
    .await;
    // The same title in another space is another page.
    let other = page(&pool, &src, "4", "Elsewhere", "OPS", &[], "").await;
    indexed(
        &pool,
        &other,
        &[
            "Elsewhere".into(),
            "Retry budget.".into(),
            link_line("OPS", "Retry budget"),
        ],
        "tomasz",
        1,
    )
    .await;

    let doc = docs_page_inner(&pool, &Registry::for_process(), &target)
        .await
        .unwrap();
    assert_eq!(
        doc.backlinks
            .iter()
            .map(|b| b.entity_id.as_str())
            .collect::<Vec<_>>(),
        [from.as_str()]
    );
    assert_eq!(doc.backlinks[0].title, "Payout design");
    let work: Vec<(&str, &str, Option<&str>, bool)> = doc
        .work
        .iter()
        .map(|w| {
            (
                w.entity_id.as_str(),
                w.key.as_str(),
                w.status.as_deref(),
                w.linked,
            )
        })
        .collect();
    assert_eq!(
        work,
        [
            (chip.as_str(), chip_key.as_str(), Some("In Progress"), false),
            (linked.as_str(), linked_key.as_str(), Some("Done"), true),
        ]
    );
    assert_eq!(
        doc.history.len(),
        1,
        "version 7 of a record that names no creator"
    );
    assert_eq!(doc.history[0].number, Some(7));
    assert_eq!(doc.history[0].by_name.as_deref(), Some("Tomasz Wójcik"));
    assert_eq!(doc.sections.len(), 2);
    assert_eq!(doc.sections[0].text.as_deref(), Some("Plain."));
    assert_eq!(
        doc.sections[1].refusal,
        Some(knobas_source::docs::SectionRefusal::Macro)
    );
    assert_eq!(doc.writes, ["comment", "update_page", "create_page"]);
}

/// *For you*: a page whose mention of the reader is in the inbox, with who
/// mentioned them; a page that does not mention them is not.
#[tokio::test]
async fn for_you_is_the_pages_that_mention_the_reader() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let w = word();
    let mentioning = page(
        &pool,
        &src,
        "1",
        &format!("Mentions {w}"),
        "ENG",
        &[],
        "<p>Body</p>",
    )
    .await;
    indexed(
        &pool,
        &mentioning,
        &[
            format!("Mentions {w}"),
            "Body".into(),
            comment_line(Some("Priya Nair")),
            "Thanks @mara ".into(),
        ],
        "tomasz",
        0,
    )
    .await;
    let quiet = page(
        &pool,
        &src,
        "2",
        &format!("Quiet {w}"),
        "ENG",
        &[],
        "<p>Body</p>",
    )
    .await;
    indexed(
        &pool,
        &quiet,
        &[format!("Quiet {w}"), "Body".into()],
        "tomasz",
        0,
    )
    .await;

    let tree = docs_tree_inner(&pool, &Registry::for_process())
        .await
        .unwrap();
    let mine: Vec<_> = tree
        .for_you
        .iter()
        .filter(|f| f.entity_id == mentioning || f.entity_id == quiet)
        .collect();
    assert_eq!(mine.len(), 1, "{:?}", tree.for_you);
    assert_eq!(mine[0].entity_id, mentioning);
    assert_eq!(
        mine[0].by_name.as_deref(),
        Some("Priya Nair"),
        "the comment that mentions"
    );
    assert_eq!(mine[0].space_key.as_deref(), Some("ENG"));
}

/// *Export as Markdown* writes one file named for the page through the one
/// converter: the title, headings, chips as keys, macros as comments.
#[tokio::test]
async fn export_writes_the_page_as_markdown() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let id = page(
        &pool,
        &src,
        "1",
        "Retry budget",
        "ENG",
        &[],
        "<h2>Cap</h2><p>Five <strong>at most</strong>, see <ac:link><ri:page ri:content-title=\"Payout design\" /></ac:link>.</p>\
         <ac:structured-macro ac:name=\"roadmap\" />",
    )
    .await;
    let folder =
        std::env::temp_dir().join(format!("knobas-export-{}", uuid::Uuid::new_v4().simple()));
    let done = docs_export_markdown_inner(&pool, &Registry::for_process(), &id, &folder)
        .await
        .unwrap();
    assert_eq!(done.file, "retry-budget.md");
    let text = std::fs::read_to_string(folder.join(&done.file)).unwrap();
    std::fs::remove_dir_all(&folder).ok();
    assert!(text.starts_with("# Retry budget\n"), "{text}");
    assert!(text.contains("## Cap"), "{text}");
    assert!(text.contains("**at most**"), "{text}");
    assert!(text.contains("](payout-design.md)"), "{text}");
    assert!(text.contains("<!-- macro: roadmap -->"), "{text}");
}

/// *Export as Markdown* never replaces a file already in the folder (#1454):
/// a taken name gets ` (1)`, then ` (2)`, as a download does, and the answer
/// names the file written. Two same-titled pages end up as two files.
#[tokio::test]
async fn export_numbers_a_taken_name_and_leaves_the_file_there() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let eng = page(
        &pool,
        &src,
        "1",
        "Notes",
        "ENG",
        &[],
        "<p>Engineering's notes.</p>",
    )
    .await;
    let ops = page(
        &pool,
        &src,
        "2",
        "Notes",
        "OPS",
        &[],
        "<p>Operations' notes.</p>",
    )
    .await;
    let folder =
        std::env::temp_dir().join(format!("knobas-export-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&folder).unwrap();
    let mine = b"my own notes, not knobas'\n";
    std::fs::write(folder.join("notes.md"), mine).unwrap();

    let first = docs_export_markdown_inner(&pool, &Registry::for_process(), &eng, &folder)
        .await
        .unwrap();
    let second = docs_export_markdown_inner(&pool, &Registry::for_process(), &ops, &folder)
        .await
        .unwrap();

    let read = |name: &str| std::fs::read(folder.join(name)).unwrap();
    let (kept, one, two) = (read("notes.md"), read(&first.file), read(&second.file));
    std::fs::remove_dir_all(&folder).ok();
    assert_eq!(kept, mine);
    assert_eq!(first.file, "notes (1).md");
    assert_eq!(second.file, "notes (2).md");
    assert!(
        String::from_utf8(one)
            .unwrap()
            .contains("Engineering's notes.")
    );
    assert!(
        String::from_utf8(two)
            .unwrap()
            .contains("Operations' notes.")
    );
}

/// *Edit section* queues the page's whole new body against the version read,
/// and refuses a section knobas does not edit and a page that moved on.
#[tokio::test]
async fn a_section_edit_is_the_whole_body_against_the_version_read() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let id = page(
        &pool,
        &src,
        "1",
        "Retry budget",
        "ENG",
        &[],
        "<h2>Cap</h2><p>Five.</p><h2>Table</h2><table><tr><td>x</td></tr></table>",
    )
    .await;
    let payload = docs_edit_section_payload(
        &pool,
        &Registry::for_process(),
        &id,
        1,
        Some(7),
        "Six.\n- one",
    )
    .await
    .unwrap();
    assert_eq!(
        payload,
        json!({"UpdatePage": {
            "entity": id,
            "base_version": 7,
            "body": "<h2>Cap</h2><p>Six.</p><ul><li>one</li></ul><h2>Table</h2><table><tr><td>x</td></tr></table>",
        }})
    );
    let refused = docs_edit_section_payload(&pool, &Registry::for_process(), &id, 2, Some(7), "x")
        .await
        .unwrap_err();
    assert!(refused.message.contains("table"), "{}", refused.message);
    let moved = docs_edit_section_payload(&pool, &Registry::for_process(), &id, 1, Some(6), "x")
        .await
        .unwrap_err();
    assert!(moved.message.contains("changed"), "{}", moved.message);
}

/// *New page* queues a `CreatePage` under the parent, in its space, with the
/// plain text in storage format; a blank title is refused.
#[tokio::test]
async fn a_new_page_goes_under_its_parent_in_storage_format() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let parent = page(&pool, &src, "1", "Payments", "ENG", &[], "").await;
    let new = knobas_app::docs::NewPage {
        parent: parent.clone(),
        title: " Retry runbook ".to_owned(),
        text: "Intro\n\n## Steps\n- restart".to_owned(),
    };
    let (payload, source) = docs_create_page_payload(&pool, &Registry::for_process(), &new)
        .await
        .unwrap();
    assert_eq!(source, src);
    assert_eq!(
        payload,
        json!({"CreatePage": {
            "parent": parent,
            "space": "ENG",
            "title": "Retry runbook",
            "body": "<p>Intro</p><h2>Steps</h2><ul><li>restart</li></ul>",
        }})
    );
    let blank = knobas_app::docs::NewPage {
        title: "  ".to_owned(),
        ..new
    };
    assert!(
        docs_create_page_payload(&pool, &Registry::for_process(), &blank)
            .await
            .is_err()
    );
}

/// *Move to Confluence…* (#941, D10): one `CreatePage` per note under the
/// picked page, oldest note first, its Markdown in storage format; two notes
/// of one title get pages of two titles, and the notes stay.
///
/// The only test in this file that writes notes, so `note::all` is these three.
#[tokio::test]
async fn moving_the_notes_queues_one_page_each_under_the_parent() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let parent = page(&pool, &src, "1", "Notes", "ENG", &[], "").await;
    for (title, body) in [
        (
            "Retry runbook",
            "Intro with [[PAY-1]]\n\n## Steps\n- restart",
        ),
        ("Untitled note", ""),
        ("untitled note", "a < b"),
    ] {
        knobas_core::note::create(&pool, title, body, &[], "user")
            .await
            .unwrap();
    }

    let payloads = move_notes_payloads(&pool, &Registry::for_process(), &parent)
        .await
        .unwrap();

    assert_eq!(
        payloads,
        [
            (
                json!({"CreatePage": {"parent": parent, "space": "ENG", "title": "Retry runbook",
                    "body": "<p>Intro with [[PAY-1]]</p><h2>Steps</h2><ul><li>restart</li></ul>"}}),
                src.clone(),
            ),
            (
                json!({"CreatePage": {"parent": parent, "space": "ENG", "title": "Untitled note",
                    "body": ""}}),
                src.clone(),
            ),
            (
                json!({"CreatePage": {"parent": parent, "space": "ENG", "title": "untitled note (2)",
                    "body": "<p>a &lt; b</p>"}}),
                src.clone(),
            ),
        ]
    );
    assert_eq!(knobas_core::note::all(&pool).await.unwrap().len(), 3);
    let elsewhere = move_notes_payloads(&pool, &Registry::for_process(), "conf-none:9")
        .await
        .unwrap_err();
    assert_eq!(elsewhere.code, knobas_app::IpcErrorCode::NotFound);
}

/// A source configured as `adapter`, its user `mara` as [`confluence`]'s.
async fn configured_as(pool: &sqlx::PgPool, adapter: &str) -> String {
    let id = format!("role{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    crate::support::source_config(
        pool,
        &id,
        adapter,
        adapter,
        &serde_json::json!({"username": "mara"}),
    )
    .await;
    id
}

/// The tree is the wiki role's read by definition, so it narrows itself to
/// the role's fillers (#1296, ADR-0029 as amended on 2026-10-19): a wiki
/// filler's space comes back with its tree, and a Confluence source with a
/// wiki miss, a source whose adapter claims no wiki, a removed source's
/// leftover pages and a source whose adapter is not installed contribute no
/// space, are never the page read last, are never *For you* and draw no
/// search hit. Every page here mentions the reader and holds the same word.
#[tokio::test]
async fn the_docs_reads_hold_only_the_wiki_fillers_pages() {
    let pool = scratch("docs_ipc_wiki_fillers").await;
    let w = word();
    let filler = confluence(&pool).await;
    let with_miss = confluence(&pool).await;
    crate::support::role_miss(&pool, &with_miss, "wiki", "page", "parent", "ancestors").await;
    let not_claiming = configured_as(&pool, "jira").await;
    let leftover = confluence(&pool).await;
    let uninstalled = configured_as(&pool, "role_nowhere").await;
    let ours = [&filler, &with_miss, &not_claiming, &leftover, &uninstalled];

    let mut pages = Vec::new();
    for source in ours {
        let home_title = format!("Home {w}");
        let home = page(&pool, source, "1", &home_title, "OPS", &[], "").await;
        let child = page(
            &pool,
            source,
            "2",
            &format!("Child {w}"),
            "OPS",
            &[&home_title],
            "",
        )
        .await;
        for id in [&home, &child] {
            indexed(
                &pool,
                id,
                &[
                    format!("Page {w}"),
                    comment_line(Some("Priya Nair")),
                    "Thanks @mara ".into(),
                ],
                "tomasz",
                0,
            )
            .await;
        }
        pages.push((home, child));
    }
    sqlx::query("delete from knobas.source_config where id = $1")
        .bind(&leftover)
        .execute(&pool)
        .await
        .unwrap();
    // The filler's page is visited first, every other page after it.
    knobas_app::conversation::visit(&pool, &pages[0].0)
        .await
        .unwrap();
    for (home, _) in &pages[1..] {
        knobas_app::conversation::visit(&pool, home).await.unwrap();
    }

    let tree = docs_tree_inner(&pool, &Registry::for_process())
        .await
        .unwrap();
    type Node<'a> = (&'a str, Option<&'a str>);
    let spaces: Vec<(&str, Vec<Node>)> = tree
        .spaces
        .iter()
        .filter(|s| ours.contains(&&s.source_id))
        .map(|s| {
            let nodes = s.pages.iter();
            let nodes = nodes.map(|p| (p.entity_id.as_str(), p.parent.as_deref()));
            (s.source_id.as_str(), nodes.collect())
        })
        .collect();
    let (home, child) = (pages[0].0.as_str(), pages[0].1.as_str());
    assert_eq!(
        spaces,
        vec![(filler.as_str(), vec![(child, Some(home)), (home, None)])],
        "the filler's space alone, with its tree"
    );
    let all: Vec<&String> = pages.iter().flat_map(|(h, c)| [h, c]).collect();
    let outsiders = &all[2..];
    assert!(
        tree.last_read
            .as_ref()
            .is_none_or(|id| !outsiders.contains(&id)),
        "the page read last is a filler's: {:?}",
        tree.last_read
    );
    let mut mine: Vec<&str> = tree
        .for_you
        .iter()
        .filter(|f| all.contains(&&f.entity_id))
        .map(|f| f.entity_id.as_str())
        .collect();
    mine.sort_unstable();
    let mut wanted = vec![home, child];
    wanted.sort_unstable();
    assert_eq!(mine, wanted, "For you is the filler's pages alone");

    let found = docs_search_inner(&pool, &Registry::for_process(), &w, None)
        .await
        .unwrap()
        .unwrap();
    let mut hits: Vec<&str> = found.hits.iter().map(|h| h.entity_id.as_str()).collect();
    hits.sort_unstable();
    assert_eq!(hits, wanted, "the search hits the filler's pages alone");
    assert_eq!(found.total, 2, "and counts no other");
}
