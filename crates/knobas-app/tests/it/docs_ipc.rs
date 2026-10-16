//! The Docs reader's two reads (#859, ADR-0022) through their command bodies,
//! `docs_tree_inner` and `docs_page_inner`, over the embedded PostgreSQL with
//! Confluence pages stored the way the adapter stores them.
//!
//! This file's database is its own (#572). Every test configures a source id
//! nothing else holds, so the trees and pages it reads are its own. The
//! identity `@me` resolves to includes `mara`, the username the Confluence
//! sources here are configured with.

use knobas_app::commands::entity::{docs_page_inner, docs_tree_inner};
use knobas_source::docs::{DocBlock, Inline};
use serde_json::{Value, json};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A Confluence source nothing else in this database uses.
async fn confluence(pool: &sqlx::PgPool) -> String {
    let id = format!("conf{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ($1, 'confluence', 'Confluence', 'http://x', 'Pat', '{\"username\":\"mara\"}')",
    )
    .bind(&id)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn item(pool: &sqlx::PgPool, id: &str, source: &str, kind: &str, title: &str, payload: Value) {
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

async fn page(pool: &sqlx::PgPool, source: &str, id: &str, title: &str, space: &str, ancestors: &[&str], storage: &str) -> String {
    let entity = format!("{source}:{id}");
    item(pool, &entity, source, "page", title, page_payload(space, ancestors, storage)).await;
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
    let restart = page(&pool, &src, "3", "Payout worker restart", "OPS", &["Ops home", "Runbooks"], "").await;
    let orphan = page(&pool, &src, "4", "Orphan", "OPS", &["Not synced"], "").await;
    let other = page(&pool, &src, "5", "Eng home", "ENG", &[], "").await;

    let tree = docs_tree_inner(&pool).await.unwrap();
    let mine: Vec<_> = tree.spaces.iter().filter(|s| s.source_id == src).collect();
    assert_eq!(
        mine.iter().map(|s| (s.key.as_str(), s.name.as_deref())).collect::<Vec<_>>(),
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
    assert_eq!(parent_of(&orphan), None, "a parent the mirror does not hold is no parent");
    assert_eq!(
        mine[1].pages.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(),
        vec!["Ops home", "Orphan", "Payout worker restart", "Runbooks"],
        "ordered by title"
    );
}

/// `#/docs` opens the page read last: the newest visit to a live page.
#[tokio::test]
async fn the_page_read_last_is_the_newest_visit() {
    let pool = pool().await;
    let src = confluence(&pool).await;
    let first = page(&pool, &src, "1", "First", "OPS", &[], "").await;
    let second = page(&pool, &src, "2", "Second", "OPS", &[], "").await;
    knobas_app::conversation::visit(&pool, &first).await.unwrap();
    knobas_app::conversation::visit(&pool, &second).await.unwrap();
    assert_eq!(docs_tree_inner(&pool).await.unwrap().last_read, Some(second));
    knobas_app::conversation::visit(&pool, &first).await.unwrap();
    assert_eq!(docs_tree_inner(&pool).await.unwrap().last_read, Some(first));
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
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ($1, 'jira', 'Jira', 'http://x', 'Pat', '{}')",
    )
    .bind(&jira)
    .execute(&pool)
    .await
    .unwrap();
    let key = format!("D{}-77", &uuid::Uuid::new_v4().simple().to_string()[..4].to_uppercase().replace(|c: char| c.is_ascii_digit(), "Q"));
    let ticket = format!("{jira}:{key}");
    item(&pool, &ticket, &jira, "ticket", "On-call runbook outdated",
        json!({"key": key, "fields": {"status": {"name": "In Progress"}}})).await;

    let home = page(&pool, &src, "1", "Ops home", "OPS", &[], "").await;
    let runbook = page(&pool, &src, "2", "Ledger reconciliation runbook", "OPS", &["Ops home"], "").await;
    let storage = format!(
        r#"<h2>Ledger</h2><p>See <ac:link><ri:page ri:content-title="Ledger reconciliation runbook" /></ac:link> and <ac:structured-macro ac:name="jira"><ac:parameter ac:name="key">{key}</ac:parameter></ac:structured-macro>, ask <ac:link><ri:user ri:username="mara" /></ac:link> or <ac:link><ri:user ri:userkey="k-tw" /></ac:link>.</p><ac:structured-macro ac:name="roadmap" />"#
    );
    let handbook = page(&pool, &src, "3", "On-call handbook", "OPS", &["Ops home"], &storage).await;

    let doc = docs_page_inner(&pool, &handbook).await.unwrap();
    assert_eq!(doc.title, "On-call handbook");
    assert_eq!(doc.space.as_ref().map(|s| (s.key.as_str(), s.name.as_deref())), Some(("OPS", Some("OPS space"))));
    assert_eq!(doc.ancestors.len(), 1);
    assert_eq!(doc.ancestors[0].title, "Ops home");
    assert_eq!(doc.ancestors[0].entity_id.as_deref(), Some(home.as_str()));
    assert_eq!(doc.editor.as_deref(), Some("tomasz"));
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
    assert_eq!(link, Some(Some(runbook)), "the page link names the mirrored page");
    let chip = text.iter().find_map(|i| match i {
        Inline::Issue { entity_id, title, status, .. } => Some((entity_id.clone(), title.clone(), status.clone())),
        _ => None,
    });
    assert_eq!(
        chip,
        Some((Some(ticket), Some("On-call runbook outdated".to_owned()), Some("In Progress".to_owned()))),
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
        vec![(Some("mara".to_owned()), true), (Some("tomasz".to_owned()), false)],
        "only the reader's own mention is marked"
    );
    assert_eq!(doc.blocks[2], DocBlock::Placeholder { name: "roadmap".into() });

    assert_eq!(doc.comments.len(), 1);
    assert_eq!(doc.comments[0].author.as_deref(), Some("priya"));
    let DocBlock::Paragraph { text } = &doc.comments[0].blocks[0] else {
        panic!("a comment is a paragraph")
    };
    assert!(
        text.iter().any(|i| matches!(i, Inline::Mention { me: true, .. })),
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
    let doc = docs_page_inner(&pool, &id).await.unwrap();
    assert_eq!(
        doc.blocks,
        vec![DocBlock::Paragraph {
            text: vec![
                Inline::Issue { key: "NOPE-1".into(), entity_id: None, title: None, status: None },
                Inline::text(" "),
                Inline::PageLink { text: "Gone".into(), title: "Gone".into(), space: None, entity_id: None },
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
    assert_eq!(docs_page_inner(&pool, &ticket).await.unwrap_err().code, NotFound);
    assert_eq!(docs_page_inner(&pool, &format!("{src}:404")).await.unwrap_err().code, NotFound);
    assert_eq!(docs_page_inner(&pool, "no colon").await.unwrap_err().code, Invalid);
}
