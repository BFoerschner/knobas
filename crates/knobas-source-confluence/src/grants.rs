//! What the Confluence user may not write (#2053), read from its permissions.
//!
//! A Confluence credential carries its user's space permissions. A user who
//! may only view a space is offered *Comment*, *Edit section* and *New page*
//! from the adapter's template `write_ops`, and Confluence refuses the write
//! only after it was queued.
//!
//! **No write is sent to find out.** Two reads answer it (measured on the
//! seeded Data Center 9.2.21, 2026-10-25):
//!
//! - `GET /rest/api/content/{id}?expand=space,operations` names the page's
//!   space and the operations **this user** may make on the page. A user who
//!   may edit it is answered `update`; one who may only view the space, or
//!   whom the page's edit restriction leaves out, is answered `read` alone.
//! - `POST /rpc/json-rpc/confluenceservice-v2/getPermissions` with
//!   `["<space key>"]` answers the user's permissions in that space:
//!   `view`, `modify` (*Add page*, which edits and creates pages), `comment`
//!   (*Add comment*) and `admin`. It is a read, the one Data Center offers an
//!   ordinary user: `/rest/api/space/{key}/permissions` and
//!   `?expand=permissions` answer 403 and 500 to anyone but an
//!   administrator, and the content operations never name a comment or a
//!   child page.
//!
//! | op | needs |
//! |---|---|
//! | `comment` | `comment` in the page's space |
//! | `create_page` | `modify` in the parent page's space |
//! | `update_page` | `update` among the page's operations |
//!
//! A `create_page` is asked about its **parent** page, the entity its
//! payload names. Whatever cannot be read is no verdict: a page that does not
//! answer refuses none of its writes, and a space whose permissions do not
//! answer refuses no comment or page in it.

use std::collections::HashMap;

use knobas_source::SourceError;
use knobas_source::grants::{RefusedWrite, WriteAsk};

/// What a page says of itself and of what this user may do to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Page {
    pub(crate) title: String,
    pub(crate) space_key: String,
    /// The space's name, `None` where the record carries none.
    pub(crate) space_name: Option<String>,
    /// Whether `update` is among the page's operations for this user.
    pub(crate) may_update: bool,
}

impl Page {
    fn space(&self) -> &str {
        self.space_name.as_deref().unwrap_or(&self.space_key)
    }
}

/// The two reads, so the judgement is tested without a server.
#[async_trait::async_trait]
pub(crate) trait GrantReads: Send + Sync {
    /// A page by content id, with its space and this user's operations.
    async fn page(&self, id: &str) -> Result<Page, SourceError>;
    /// This user's permissions in the space `key`, as `getPermissions` spells
    /// them.
    async fn space_permissions(&self, key: &str) -> Result<Vec<String>, SourceError>;
}

/// The writes this answers, as `write_ops` spells them.
const OP_COMMENT: &str = "comment";
const OP_CREATE_PAGE: &str = "create_page";
const OP_UPDATE_PAGE: &str = "update_page";

/// The space permission `getPermissions` answers for *Add page*.
const MODIFY: &str = "modify";
/// The space permission `getPermissions` answers for *Add comment*.
const COMMENT: &str = "comment";

/// The asks among `asks` this user's permissions refuse, in the order asked.
/// Each comes with its entity's content id.
pub(crate) async fn refused(
    reads: &dyn GrantReads,
    asks: &[(&WriteAsk, String)],
) -> Vec<RefusedWrite> {
    let governed: Vec<_> = asks
        .iter()
        .filter(|(a, _)| {
            matches!(
                a.op.as_str(),
                OP_COMMENT | OP_CREATE_PAGE | OP_UPDATE_PAGE
            )
        })
        .collect();
    let mut pages: HashMap<&str, Option<Page>> = HashMap::new();
    let mut spaces: HashMap<String, Option<Vec<String>>> = HashMap::new();
    let mut out = Vec::new();
    for (ask, id) in governed {
        if !pages.contains_key(id.as_str()) {
            pages.insert(id, reads.page(id).await.ok());
        }
        let Some(Some(page)) = pages.get(id.as_str()) else {
            continue;
        };
        if !spaces.contains_key(&page.space_key) {
            let held = reads.space_permissions(&page.space_key).await.ok();
            spaces.insert(page.space_key.clone(), held);
        }
        let held = spaces.get(&page.space_key).and_then(Option::as_ref);
        if let Some(reason) = refusal(&ask.op, page, held) {
            out.push(RefusedWrite::of(ask, reason));
        }
    }
    out
}

/// Why `op` cannot be made on `page`, given the space permissions `held`
/// (`None` where they could not be read), or `None` where it can or this
/// cannot tell.
fn refusal(op: &str, page: &Page, held: Option<&Vec<String>>) -> Option<String> {
    let has = |permission: &str| held.map(|h| h.iter().any(|p| p == permission));
    let space = page.space();
    match op {
        OP_COMMENT => (has(COMMENT) == Some(false)).then(|| {
            format!(
                "This Confluence user cannot comment in {space}: it lacks the Add comment \
                 permission"
            )
        }),
        OP_CREATE_PAGE => (has(MODIFY) == Some(false)).then(|| {
            format!(
                "This Confluence user cannot add pages to {space}: it lacks the Add page \
                 permission"
            )
        }),
        OP_UPDATE_PAGE => (!page.may_update).then(|| match has(MODIFY) {
            Some(false) => format!(
                "This Confluence user cannot edit pages in {space}: it lacks the Add page \
                 permission"
            ),
            Some(true) => format!(
                "This Confluence user cannot edit {}: the page's restrictions leave it out",
                page.title
            ),
            None => format!("This Confluence user cannot edit {}", page.title),
        }),
        _ => None,
    }
}

/// The real reads (measured on Data Center 9.2.21, see the module).
#[async_trait::async_trait]
impl GrantReads for crate::http::ConfluenceHttp {
    async fn page(&self, id: &str) -> Result<Page, SourceError> {
        #[derive(serde::Deserialize)]
        struct Space {
            key: String,
            #[serde(default)]
            name: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct Operation {
            operation: String,
            #[serde(rename = "targetType")]
            target_type: String,
        }
        #[derive(serde::Deserialize)]
        struct Record {
            title: String,
            space: Space,
            #[serde(default)]
            operations: Vec<Operation>,
        }
        let record: Record = self
            .get_json(
                &format!("rest/api/content/{id}"),
                &[("expand", "space,operations".to_owned())],
            )
            .await?;
        Ok(Page {
            title: record.title,
            space_key: record.space.key,
            space_name: record.space.name.filter(|n| !n.trim().is_empty()),
            may_update: record
                .operations
                .iter()
                .any(|o| o.operation == "update" && o.target_type == "page"),
        })
    }

    async fn space_permissions(&self, key: &str) -> Result<Vec<String>, SourceError> {
        self.rpc(
            "getPermissions",
            &serde_json::Value::Array(vec![key.into()]),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Pages by id and spaces by key, each read counted; a missing one fails.
    #[derive(Default)]
    struct Fake {
        pages: HashMap<String, Page>,
        spaces: HashMap<String, Vec<String>>,
        read: Mutex<Vec<String>>,
    }

    impl Fake {
        fn page(mut self, id: &str, space: &str, may_update: bool) -> Self {
            self.pages.insert(
                id.to_owned(),
                Page {
                    title: format!("Page {id}"),
                    space_key: space.to_owned(),
                    space_name: Some(format!("Space {space}")),
                    may_update,
                },
            );
            self
        }

        fn space(mut self, key: &str, held: &[&str]) -> Self {
            self.spaces
                .insert(key.to_owned(), held.iter().map(|p| (*p).to_owned()).collect());
            self
        }

        fn reads(&self) -> Vec<String> {
            self.read.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl GrantReads for Fake {
        async fn page(&self, id: &str) -> Result<Page, SourceError> {
            self.read.lock().unwrap().push(format!("page {id}"));
            self.pages
                .get(id)
                .cloned()
                .ok_or_else(|| SourceError::protocol(format!("HTTP 404 Not Found: {id}")))
        }

        async fn space_permissions(&self, key: &str) -> Result<Vec<String>, SourceError> {
            self.read.lock().unwrap().push(format!("space {key}"));
            self.spaces
                .get(key)
                .cloned()
                .ok_or_else(|| SourceError::protocol(format!("no space {key}")))
        }
    }

    fn ask(id: &str, op: &str) -> WriteAsk {
        WriteAsk {
            entity: format!("confluence:{id}"),
            op: op.to_owned(),
        }
    }

    async fn judged(fake: &Fake, asks: &[WriteAsk]) -> Vec<(String, String, String)> {
        let keyed: Vec<_> = asks
            .iter()
            .map(|a| (a, a.entity.trim_start_matches("confluence:").to_owned()))
            .collect();
        refused(fake, &keyed)
            .await
            .into_iter()
            .map(|r| (r.entity, r.op, r.reason))
            .collect()
    }

    const ALL: [&str; 3] = ["comment", "update_page", "create_page"];

    #[tokio::test]
    async fn a_user_who_may_only_view_the_space_is_refused_every_write_saying_why() {
        let fake = Fake::default().page("7", "KRO", false).space("KRO", &["view"]);
        let asks: Vec<_> = ALL.iter().map(|op| ask("7", op)).collect();
        assert_eq!(
            judged(&fake, &asks).await,
            vec![
                (
                    "confluence:7".into(),
                    "comment".into(),
                    "This Confluence user cannot comment in Space KRO: it lacks the Add comment \
                     permission"
                        .into()
                ),
                (
                    "confluence:7".into(),
                    "update_page".into(),
                    "This Confluence user cannot edit pages in Space KRO: it lacks the Add page \
                     permission"
                        .into()
                ),
                (
                    "confluence:7".into(),
                    "create_page".into(),
                    "This Confluence user cannot add pages to Space KRO: it lacks the Add page \
                     permission"
                        .into()
                ),
            ]
        );
        assert_eq!(fake.reads(), ["page 7", "space KRO"], "each read once");
    }

    #[tokio::test]
    async fn a_user_who_may_write_the_space_is_refused_nothing() {
        let fake = Fake::default()
            .page("7", "ENG", true)
            .space("ENG", &["view", "modify", "comment"]);
        let asks: Vec<_> = ALL.iter().map(|op| ask("7", op)).collect();
        assert_eq!(judged(&fake, &asks).await, vec![]);
    }

    #[tokio::test]
    async fn a_page_whose_restrictions_leave_the_user_out_refuses_its_edit_alone() {
        let fake = Fake::default()
            .page("7", "ENG", false)
            .space("ENG", &["view", "modify", "comment"]);
        let asks: Vec<_> = ALL.iter().map(|op| ask("7", op)).collect();
        assert_eq!(
            judged(&fake, &asks).await,
            vec![(
                "confluence:7".into(),
                "update_page".into(),
                "This Confluence user cannot edit Page 7: the page's restrictions leave it out"
                    .into()
            )]
        );
    }

    #[tokio::test]
    async fn comment_and_add_page_are_told_apart() {
        let fake = Fake::default().page("7", "ENG", false).space("ENG", &["view", "comment"]);
        let ops: Vec<_> = judged(
            &fake,
            &[ask("7", "comment"), ask("7", "create_page")],
        )
        .await
        .into_iter()
        .map(|(_, op, _)| op)
        .collect();
        assert_eq!(ops, ["create_page"]);
    }

    #[tokio::test]
    async fn what_cannot_be_read_is_no_verdict() {
        // Page 8 does not answer; space ENG's permissions do not either, so
        // only the page's own operations can refuse, and they say why less.
        let fake = Fake::default().page("7", "ENG", false);
        let asks = [
            ask("8", "comment"),
            ask("7", "comment"),
            ask("7", "create_page"),
            ask("7", "update_page"),
        ];
        assert_eq!(
            judged(&fake, &asks).await,
            vec![(
                "confluence:7".into(),
                "update_page".into(),
                "This Confluence user cannot edit Page 7".into()
            )]
        );
    }

    #[tokio::test]
    async fn an_op_this_adapter_does_not_write_reads_nothing() {
        let fake = Fake::default();
        assert_eq!(judged(&fake, &[ask("7", "transition")]).await, vec![]);
        assert_eq!(fake.reads(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn two_pages_of_one_space_read_its_permissions_once() {
        let fake = Fake::default()
            .page("7", "KRO", true)
            .page("9", "KRO", true)
            .space("KRO", &["view", "modify"]);
        let refused = judged(&fake, &[ask("7", "comment"), ask("9", "comment")]).await;
        assert_eq!(refused.len(), 2);
        assert_eq!(fake.reads(), ["page 7", "space KRO", "page 9"]);
    }
}
