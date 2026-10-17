//! Who a mention names, looked up at sync (#953).
//!
//! A Confluence mention is `<ri:user ri:userkey="…"/>`: a key and no name.
//! The page's own record names some users -- its editor, its creator, each
//! comment's author -- and a mention of one of those resolves from the record
//! alone ([`crate::blocks`]). Any other key would draw as *@someone*. So the
//! sync looks those keys up, `GET /rest/api/user?key=…`, and keeps the user
//! records it got beside the page's, at [`MENTIONED_USERS`], which is where
//! the Docs projection reads them. The reader opens from the mirror, with no
//! network, so the lookup has to happen here.
//!
//! # What it costs
//!
//! One request per **distinct** key per run, and only for keys the record does
//! not already name: [`Directory`] keeps every answer for the rest of the run,
//! a miss included, and starts out knowing the credential's own key from
//! `rest/api/user/current`. Data Center has no bulk form of the call. A space
//! of a thousand pages that mention the same twenty people costs twenty
//! requests on its full sync, and an incremental run costs the new names only.
//! Every request goes through the instance's rate limiter, retry budget and
//! `Retry-After` handling in `knobas-http`, like every other call.
//!
//! # When a lookup fails
//!
//! A `404` is a key Confluence does not know (a deleted user): that key is a
//! miss for the run. **Any other failure stops the lookups for the rest of the
//! run**, and every key not yet known is a miss: a lookup this credential may
//! not make (`403`), or a server still refusing after the retry budget, is not
//! a reason to fail the sync of the pages themselves, nor to keep asking. The
//! cost is a mention drawn as *@someone* until that page syncs again.

use std::collections::{BTreeSet, HashMap};

use knobas_source::SourceError;
use serde_json::{Value, json};

use crate::api::ConfluenceApi;
use crate::model::{CurrentUser, RawContent};
use crate::storage::{attr, split_tag, tag_name};

/// The key a page's payload keeps the looked-up users at: an array of
/// Confluence user records (`userKey`, `username`, `displayName`, …), ordered
/// by key. Absent where the sync looked nobody up.
pub(crate) const MENTIONED_USERS: &str = "mentioned_users";

/// The run's answers to "who is this key", kept for the whole run.
pub(crate) struct Directory {
    /// A key -> the user record, or `None` for a key Confluence does not know.
    known: HashMap<String, Option<Value>>,
    /// Set by the first lookup that failed for any reason but `404`.
    stopped: bool,
}

impl Directory {
    /// A directory that already knows the credential's own key.
    pub(crate) fn new(me: &CurrentUser) -> Self {
        let mut known = HashMap::new();
        if let Some(key) = me.user_key.as_deref().filter(|k| !k.is_empty()) {
            known.insert(
                key.to_owned(),
                Some(json!({
                    "userKey": key,
                    "username": me.username,
                    "displayName": me.display_name,
                })),
            );
        }
        Self {
            known,
            stopped: false,
        }
    }

    /// Look up every key the page and its comments mention that its record
    /// does not name, and keep the users found at [`MENTIONED_USERS`].
    pub(crate) async fn annotate(&mut self, api: &(dyn ConfluenceApi + '_), raw: &mut RawContent) {
        let named = crate::blocks::people(&raw.raw);
        let mut keys: BTreeSet<String> = mentioned_keys(raw.storage());
        if let Some(comments) = raw
            .content
            .children
            .as_ref()
            .and_then(|c| c.comment.as_ref())
        {
            for c in &comments.results {
                keys.extend(mentioned_keys(c.storage()));
            }
        }
        keys.retain(|key| !named.contains_key(key));
        let mut users = Vec::new();
        for key in keys {
            if let Some(user) = self.lookup(api, &key).await {
                users.push(user);
            }
        }
        if !users.is_empty()
            && let Some(object) = raw.raw.as_object_mut()
        {
            object.insert(MENTIONED_USERS.to_owned(), Value::Array(users));
        }
    }

    async fn lookup(&mut self, api: &(dyn ConfluenceApi + '_), key: &str) -> Option<Value> {
        if let Some(known) = self.known.get(key) {
            return known.clone();
        }
        if self.stopped {
            return None;
        }
        let answer = match api.user_by_key(key).await {
            // Only a record that says it is this key's: anything else would
            // name the wrong person.
            Ok(user) if user.get("userKey").and_then(Value::as_str) == Some(key) => Some(user),
            Ok(_) => None,
            Err(e) if is_unknown(&e) => None,
            Err(_) => {
                self.stopped = true;
                return None;
            }
        };
        self.known.insert(key.to_owned(), answer.clone());
        answer
    }
}

fn is_unknown(e: &SourceError) -> bool {
    matches!(e, SourceError::Protocol { .. }) && e.status() == Some(404)
}

/// Every non-empty `ri:userkey` a storage-format body mentions. A tag inside
/// CDATA (a code macro's text) or a comment is not one.
pub(crate) fn mentioned_keys(storage: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = storage;
    while let Some(open) = rest.find('<') {
        rest = &rest[open..];
        if let Some(tail) = rest.strip_prefix("<![CDATA[") {
            rest = tail.split_once("]]>").map_or("", |(_, tail)| tail);
            continue;
        }
        if let Some(tail) = rest.strip_prefix("<!--") {
            rest = tail.split_once("-->").map_or("", |(_, tail)| tail);
            continue;
        }
        let Some((tag, tail)) = split_tag(rest) else {
            break;
        };
        if tag_name(tag) == "ri:user"
            && let Some(key) = attr(tag, "ri:userkey").filter(|k| !k.is_empty())
        {
            out.insert(key.to_owned());
        }
        rest = tail;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_of_a_body_are_its_user_links_and_not_its_code() {
        let storage = concat!(
            r#"<p><ac:link><ri:user ri:userkey="k-1" /></ac:link> "#,
            r#"<ac:link><ri:user ri:username="mara" /></ac:link> "#,
            r#"<ac:link><ri:user ri:userkey="" /></ac:link></p>"#,
            r#"<ac:structured-macro ac:name="code"><ac:plain-text-body><![CDATA[<ri:user ri:userkey="k-code"/>]]></ac:plain-text-body></ac:structured-macro>"#,
            r#"<!-- <ri:user ri:userkey="k-comment"/> --><p><ri:user ri:userkey="k-2"/><ri:user ri:userkey="k-1"/></p>"#,
        );
        assert_eq!(
            mentioned_keys(storage).into_iter().collect::<Vec<_>>(),
            vec!["k-1", "k-2"]
        );
    }
}
