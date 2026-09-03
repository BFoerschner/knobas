//! One Confluence page → one [`SyncItem`], per contract §4.1
//! "Normalization".

use knobas_core::entity::EntityRef;
use knobas_source::SyncItem;

use crate::model::RawContent;

/// Map one page, in the namespace of the instance that fetched it.
///
/// `source_id` is `SourceInstance::id` and never the adapter kind (P10): two
/// Confluences are `confluence` and `confluence-eu`, and their items must not
/// collide.
pub(crate) fn to_sync_item(source_id: &str, base_url: &str, raw: &RawContent) -> SyncItem {
    let content = &raw.content;
    let title = content.title.clone().unwrap_or_default();
    let body = crate::storage::to_text(raw.storage());
    let comments = content
        .children
        .as_ref()
        .and_then(|c| c.comment.as_ref())
        .map(|c| c.results.as_slice())
        .unwrap_or_default();
    let body_text = join(
        [title.clone(), body].into_iter().chain(
            comments
                .iter()
                .map(|c| crate::storage::to_text(c.storage())),
        ),
    );
    SyncItem {
        // **The content id, never the title.** A page renamed in Confluence
        // keeps its id, so every link, note and worklog knobas drew to it
        // survives the rename -- which is the criterion, and which a
        // title-shaped or space-and-title-shaped key could not do.
        entity: EntityRef::new(source_id, &content.id),
        kind: crate::KIND_PAGE.to_owned(),
        title,
        body_text,
        // §4.1 "the source's username string". The person who made the
        // version this record *is* -- which is what `updated_at` is the time
        // of, so the two answer about the same event. A page nobody has edited
        // since it was created falls back to its creator rather than to
        // nothing.
        author: version_author(raw).or_else(|| created_by(raw)),
        // The source's own timestamp, never now(). `version.when` is exactly
        // what CQL's `lastmodified` matches, so the mirror's idea of "changed"
        // and the cursor's are the same idea.
        updated_at: content
            .version
            .as_ref()
            .and_then(|v| v.when.as_deref())
            .and_then(crate::time::parse_time),
        payload: raw.raw.clone(),
        // P5: the page Confluence itself names in `_links.webui`, on the base
        // URL the user typed -- which is the one reachable from the user's
        // machine, unlike `_links.base`, which is whatever the instance
        // believes it is called. `None` where the server said nothing rather
        // than a URL knobas composed: *Open in browser* is simply absent then.
        web_url: content
            .links
            .as_ref()
            .and_then(|l| l.webui.as_deref())
            .filter(|webui| !webui.trim().is_empty())
            .map(|webui| {
                format!(
                    "{}/{}",
                    base_url.trim_end_matches('/'),
                    webui.trim_start_matches('/')
                )
            }),
        // A CQL search cannot report deletions; the engine's full-sync sweep
        // tombstones what stopped coming back (contract §4.1).
        deleted: false,
    }
}

fn version_author(raw: &RawContent) -> Option<String> {
    raw.content
        .version
        .as_ref()
        .and_then(|v| v.by.as_ref())
        .and_then(|u| u.username.clone())
}

fn created_by(raw: &RawContent) -> Option<String> {
    raw.content
        .history
        .as_ref()
        .and_then(|h| h.created_by.as_ref())
        .and_then(|u| u.username.clone())
}

/// Join the non-empty parts into the blob FTS indexes -- the same rule
/// `knobas-source-mock` and the Jira adapter use, so a Confluence page and a
/// Jira ticket are indexed alike.
fn join(parts: impl IntoIterator<Item = String>) -> String {
    parts
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The seeded page as the real server answers it, with the expansions
    /// [`crate::api::EXPAND`] asks for. Every path asserted below is one
    /// `tests/live_confluence_seeded.rs` re-asserts against Confluence itself
    /// -- this fixture is a convenience, never the witness (ADR-0013).
    fn a_page() -> RawContent {
        serde_json::from_value(json!({
            "id": "98307",
            "type": "page",
            "status": "current",
            "title": "SEPA payout retry design",
            "space": { "key": "ENG", "name": "Engineering", "type": "global" },
            "body": {
                "storage": {
                    "value": "<h2>Backoff policy</h2><p>base 30 s, factor 2, max 5 attempts.</p>",
                    "representation": "storage"
                }
            },
            "version": {
                "number": 3,
                "when": "2026-08-22T12:40:00.000+02:00",
                "by": { "username": "knobas", "displayName": "knobas" }
            },
            "history": { "createdBy": { "username": "mara.lindqvist" } },
            "ancestors": [
                { "id": "65537", "title": "Engineering", "type": "page" },
                { "id": "65540", "title": "Payments", "type": "page" }
            ],
            "children": {
                "comment": {
                    "results": [{
                        "id": "98320",
                        "type": "comment",
                        "body": { "storage": { "value": "<p>@Mara can you add the SLA?</p>" } }
                    }],
                    "size": 1,
                    "_links": {}
                }
            },
            "_links": { "webui": "/display/ENG/SEPA+payout+retry+design" }
        }))
        .expect("the golden page is a valid content record")
    }

    /// Contract §4.1 normalization, and the criterion's key form
    /// `confluence:<content id>`.
    #[test]
    fn maps_a_page_the_way_the_conventions_say() {
        let item = to_sync_item("confluence", "http://127.0.0.1:8090", &a_page());
        assert_eq!(item.entity.to_string(), "confluence:98307");
        assert_eq!(item.kind, crate::KIND_PAGE);
        assert_eq!(item.title, "SEPA payout retry design");
        // The person who made this version, in the source's own spelling.
        assert_eq!(item.author.as_deref(), Some("knobas"));
        assert_eq!(
            item.updated_at.map(|t| t.to_rfc3339()),
            Some("2026-08-22T10:40:00+00:00".to_owned())
        );
        // P5: the path Confluence itself named, on the URL the user typed.
        assert_eq!(
            item.web_url.as_deref(),
            Some("http://127.0.0.1:8090/display/ENG/SEPA+payout+retry+design")
        );
        assert!(!item.deleted, "a CQL search cannot report deletions");
    }

    /// The identity criterion, made a test: **the id, not the title**. A page
    /// renamed in Confluence is the same entity, so every link drawn to it
    /// survives -- and a title in the key would have broken all of them.
    #[test]
    fn a_renamed_page_is_the_same_entity() {
        let before = to_sync_item("confluence", "http://x.example", &a_page());
        let mut renamed = a_page();
        renamed.raw["title"] = json!("SEPA payout retry design (v2)");
        renamed.content.title = Some("SEPA payout retry design (v2)".to_owned());
        let after = to_sync_item("confluence", "http://x.example", &renamed);
        assert_eq!(after.entity, before.entity);
        assert_ne!(after.title, before.title);
    }

    /// `body_text` = title + the storage format **stripped to text** + the
    /// comment texts, blank-line joined. No markup reaches it: that is the
    /// criterion, and it is roadmap §4 gotcha 7 at the same time.
    #[test]
    fn body_text_is_the_stripped_body_and_the_discussion() {
        let item = to_sync_item("confluence", "http://x.example", &a_page());
        assert_eq!(
            item.body_text,
            "SEPA payout retry design\n\nBackoff policy\nbase 30 s, factor 2, max 5 \
             attempts.\n\n@Mara can you add the SLA?"
        );
        assert!(!item.body_text.contains('<'), "{}", item.body_text);
    }

    /// Spec §3a: `payload` is the raw record, so a later mapping can
    /// re-project it without re-syncing -- which is precisely what the next
    /// ticket's storage-format renderer does. The **verbatim** storage format
    /// is the half that matters, and it is the half `body_text` threw away.
    #[test]
    fn payload_keeps_the_storage_format_verbatim() {
        let item = to_sync_item("confluence", "http://x.example", &a_page());
        assert_eq!(
            item.payload["body"]["storage"]["value"],
            "<h2>Backoff policy</h2><p>base 30 s, factor 2, max 5 attempts.</p>"
        );
        assert_eq!(item.payload["body"]["storage"]["representation"], "storage");
        // ADR-0010: a space is Confluence's project, and this is where a
        // reader outside the adapter finds its key and its name.
        assert_eq!(item.payload["space"]["key"], "ENG");
        assert_eq!(item.payload["space"]["name"], "Engineering");
        // The ancestors, outermost first, which is the order a path is read in.
        assert_eq!(item.payload["ancestors"][0]["title"], "Engineering");
        assert_eq!(item.payload["ancestors"][1]["title"], "Payments");
        // The comments, at Confluence's own path.
        assert_eq!(
            item.payload["children"]["comment"]["results"][0]["id"],
            "98320"
        );
        assert_eq!(item.payload["version"]["number"], 3);
    }

    /// A page nobody has edited since it was created -- four of the five
    /// fixture pages -- still has an author, and it is the creator.
    #[test]
    fn a_page_whose_version_names_nobody_falls_back_to_its_creator() {
        let mut page = a_page();
        page.content.version.as_mut().unwrap().by = None;
        assert_eq!(
            to_sync_item("confluence", "http://x.example", &page)
                .author
                .as_deref(),
            Some("mara.lindqvist")
        );
        // And a record naming nobody at all has no author rather than a guess.
        page.content.history = None;
        assert_eq!(
            to_sync_item("confluence", "http://x.example", &page).author,
            None
        );
    }

    /// An empty page is a page: the *Standup protocols* parent the fixture
    /// creates has no body and no comments, and it must still be findable by
    /// its title.
    #[test]
    fn an_empty_page_is_its_title_and_nothing_else() {
        let page: RawContent = serde_json::from_value(json!({
            "id": "98400",
            "title": "Standup protocols",
            "body": { "storage": { "value": "", "representation": "storage" } },
            "version": { "number": 1, "when": "2026-08-21T00:00:00.000+02:00" },
            "_links": { "webui": "/display/ENG/Standup+protocols" }
        }))
        .unwrap();
        let item = to_sync_item("confluence", "http://x.example", &page);
        assert_eq!(item.body_text, "Standup protocols");
        assert_eq!(item.author, None);
        assert_eq!(item.entity.to_string(), "confluence:98400");
    }

    /// P5: a source that names no page gets no *Open in browser* rather than a
    /// URL knobas invented. A blank `webui` is the same absence as a missing
    /// one -- it would otherwise compose to the instance's front page, which
    /// is a link that works and goes to the wrong place.
    #[test]
    fn a_page_with_no_web_link_has_no_web_url() {
        let mut page = a_page();
        page.content.links = None;
        assert_eq!(
            to_sync_item("confluence", "http://x.example", &page).web_url,
            None
        );

        let mut blank = a_page();
        blank.content.links.as_mut().unwrap().webui = Some("  ".to_owned());
        assert_eq!(
            to_sync_item("confluence", "http://x.example", &blank).web_url,
            None
        );
    }

    /// The instance id is the namespace (P10), so a second Confluence does not
    /// collide with the first -- and a base URL the user pasted with a
    /// trailing slash must not produce `…example//display/…`.
    #[test]
    fn the_namespace_is_the_instance_id_and_the_base_url_does_not_double() {
        let item = to_sync_item(
            "confluence-eu",
            "https://wiki.eu.example/confluence/",
            &a_page(),
        );
        assert_eq!(item.entity.namespace, "confluence-eu");
        assert_eq!(
            item.web_url.as_deref(),
            Some("https://wiki.eu.example/confluence/display/ENG/SEPA+payout+retry+design")
        );
    }
}
