//! The job's conversation (#819, spec #808 "Signals, conversation and
//! proposals"; requirements R-5.1 to R-5.5): the thread a ticket carries, who
//! of it is new since the reader last looked, and which comments mention them.
//!
//! Two reads, each its own command on the entity module:
//!
//! * [`visit`] -- **last visit**. Opening a job writes one `visited` line to the
//!   activity stream and answers the one before it, so "new since your last
//!   visit" means since the reader last opened this ticket in knobas. The
//!   stream is the record (R-5.5 names it); there is no table of visits.
//! * [`thread`] -- the **comments**, and nothing else (#2163: the status-move
//!   and pull-request lines of #819 and #929 are retired). The comments are a
//!   named payload read (ADR-0007), [`COMMENTS`]; *mentions me* is matched in
//!   each body against the identity `@me` resolves to.
//!
//! # A `visited` line is bookkeeping, not work
//!
//! It is an action the reader took, so its actor is `user`, and it says
//! nothing about what they did all afternoon. The worklog draft leaves it out
//! (`time::worklog`'s `NOT_WORK`), and so does the activity read the history
//! panel and the status bar draw from (`knobas_core::activity::recent`):
//! a line per opened job would push every real change off both.
use chrono::{DateTime, Utc};
use knobas_core::entity::EntityRef;
use sqlx::{PgPool, Row};

use crate::IpcError;

/// The activity verb a job's opening writes.
pub use knobas_core::activity::VISITED;

/// The actor of a visit: the reader, like every action they take.
const ACTOR: &str = "user";

/// The newest visit to `$1` before this one.
///
/// Through `0001`'s `activity_entity_idx (entity_id, at desc)`.
const LAST_VISIT: &str = "select max(at) from knobas.activity
    where entity_id = $1 and verb = $2 and actor = $3";

/// Record that the reader opened `entity_id`, and answer when they last did
/// before now -- `None` for a first visit.
///
/// The read and the line are one transaction, so two windows opening the same
/// job at once each answer a visit that really happened before theirs.
///
/// # Errors
///
/// `invalid` for something that is not an entity id, `not_found` for one the
/// database does not know, `internal` for a query failure.
pub async fn visit(pool: &PgPool, entity_id: &str) -> Result<Option<DateTime<Utc>>, IpcError> {
    let entity = EntityRef::parse(entity_id).map_err(IpcError::invalid)?;
    let id = entity.to_string();
    let known: Option<(String,)> = sqlx::query_as("select id from knobas.entity where id = $1")
        .bind(&id)
        .fetch_optional(pool)
        .await?;
    if known.is_none() {
        return Err(IpcError::not_found(format!("{id} is not in the mirror")));
    }
    let mut tx = pool.begin().await?;
    let before: Option<DateTime<Utc>> = sqlx::query_scalar(LAST_VISIT)
        .bind(&id)
        .bind(VISITED)
        .bind(ACTOR)
        .fetch_one(&mut *tx)
        .await?;
    knobas_core::activity::record_with(
        &mut *tx,
        ACTOR,
        VISITED,
        Some(&entity),
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(before)
}

/// One comment of a thread.
///
/// Issue #819.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ThreadComment {
    /// Who wrote it, as the source names them to a person (Jira's
    /// `displayName`), else their account. `None` when the record says nobody.
    pub author: Option<String>,
    /// When it was written. `None` when the source's stamp is absent or reads
    /// as no time: such a comment keeps its place in the source's order.
    ///
    /// RFC 3339 on the wire.
    pub at: Option<DateTime<Utc>>,
    /// The text as the source wrote it, untrusted. Drawn by
    /// [`markup`](Self::markup), never as HTML.
    pub body: String,
    /// How the source writes it (#2308), as the ticket's kind declares:
    /// `jira_wiki` for Jira, `text` where nothing is declared.
    pub markup: knobas_source::Markup,
    /// The reader wrote it: its author's account is one of the identity's.
    pub mine: bool,
    /// The body names the reader (R-5.3), by [`mentions`]'s rule.
    ///
    /// `@name` or Jira's `[~name]`, for an account `@me` resolves to.
    pub mentions_me: bool,
}

/// A ticket's thread, oldest first.
///
/// Issue #819.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Thread {
    /// In the source's order, oldest first.
    pub comments: Vec<ThreadComment>,
    /// The accounts `@me` resolves to, so the shell can mark `@name` in a body
    /// by the same identity this read matched.
    pub me: Vec<String>,
}

/// **The comment list, the one named payload read** (ADR-0007).
///
/// Two spellings, each one more `coalesce` candidate here and nowhere else:
/// Jira's `fields.comment.comments` and the mock adapter's `comments`. The
/// list comes back whole; [`comment_of`] reads each entry's four fields.
///
/// **Failure direction: a miss is an empty thread**, never a wrong one. A
/// payload shaped some other way, or a container that is not an array, reads
/// as no comments -- the summary then says *No comments yet*, which is wrong
/// only by omission. Pinned by `a_payload_of_another_shape_reads_as_no_comments`.
const COMMENTS: &str = "select coalesce(
       case when jsonb_typeof(payload #> '{fields,comment,comments}') = 'array'
            then payload #> '{fields,comment,comments}' end,
       case when jsonb_typeof(payload -> 'comments') = 'array'
            then payload -> 'comments' end,
       '[]'::jsonb) as comments, source_id, kind
  from sync.item
 where entity_id = $1";

/// The string at `key` of `value`, trimmed, or `None` (blank and non-strings
/// miss, ADR-0007).
fn text_at<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    let mut at = value;
    for key in path {
        at = at.get(key)?;
    }
    at.as_str().map(str::trim).filter(|text| !text.is_empty())
}

/// A source's comment stamp: RFC 3339 (the mock adapter), or Jira's
/// `2026-08-12T09:14:00.000+0200`.
fn stamp(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .or_else(|_| DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f%z"))
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

/// One entry of [`COMMENTS`], read for both spellings.
fn comment_of(
    entry: &serde_json::Value,
    me: &[String],
    markup: knobas_source::Markup,
) -> Option<ThreadComment> {
    let body = text_at(entry, &["body"]).or_else(|| text_at(entry, &["text"]))?;
    let account = text_at(entry, &["author", "name"])
        .or_else(|| text_at(entry, &["author", "key"]))
        .or_else(|| text_at(entry, &["who"]));
    let author = text_at(entry, &["author", "displayName"]).or(account);
    let at = text_at(entry, &["created"])
        .or_else(|| text_at(entry, &["when"]))
        .and_then(stamp);
    Some(ThreadComment {
        author: author.map(str::to_owned),
        at,
        body: body.to_owned(),
        markup,
        mine: account.is_some_and(|account| me.iter().any(|name| name == account)),
        mentions_me: mentions(body, me),
    })
}

/// Whether `body` names one of `me`.
///
/// The inbox's rule (`knobas_core::inbox`'s `mention!`), so a comment the
/// inbox calls a mention is one here: the account preceded by `@` or `[~` (Jira
/// Data Center writes `[~name]`), and **not** followed by another name
/// character, `.` or `-`, so `mara` does not match `@mara.lindqvist`. A name
/// character is the inbox's ASCII class `[A-Za-z0-9_]`, so the two rules
/// agree on every body. Case-sensitive, as the account is spelled.
pub fn mentions(body: &str, me: &[String]) -> bool {
    me.iter().filter(|name| !name.is_empty()).any(|name| {
        ["@", "[~"].iter().any(|lead| {
            let needle = format!("{lead}{name}");
            body.match_indices(&needle).any(|(at, _)| {
                body[at + needle.len()..].chars().next().is_none_or(|next| {
                    !(next.is_ascii_alphanumeric() || matches!(next, '_' | '.' | '-'))
                })
            })
        })
    })
}

/// The identity behind `@me`: the one place "who am I" lives
/// (`knobas_search::Vocabulary`), as the inbox reads it.
async fn identity(pool: &PgPool) -> Result<Vec<String>, IpcError> {
    Ok(
        knobas_search::Vocabulary::load(pool, knobas_search::KindCatalog::default())
            .await
            .map_err(IpcError::internal)?
            .identity,
    )
}

/// The thread of `entity_id`: its comments in the source's order, which both
/// spellings write oldest first (Jira's comment list is ordered by `created`,
/// and the Jira adapter reads it so). Each comment carries the markup its
/// source declares for the ticket's kind (#2308), read off `registry`.
///
/// # Errors
///
/// `invalid` for something that is not an entity id, `not_found` for one the
/// mirror does not hold, `internal` for a query failure.
pub async fn thread(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    entity_id: &str,
) -> Result<Thread, IpcError> {
    let entity = EntityRef::parse(entity_id).map_err(IpcError::invalid)?;
    let id = entity.to_string();
    let row = sqlx::query(COMMENTS)
        .bind(&id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("{id} is not in the mirror")))?;
    let list: serde_json::Value = row.get("comments");
    let source_id: String = row.get("source_id");
    let kind: String = row.get("kind");
    let markup = crate::sources::paths::declared_paths(pool, registry)
        .await?
        .get(&source_id, &kind)
        .map(|paths| paths.markup)
        .unwrap_or_default();
    let me = identity(pool).await?;
    let comments: Vec<ThreadComment> = list
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| comment_of(entry, &me, markup))
                .collect()
        })
        .unwrap_or_default();
    Ok(Thread { comments, me })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> Vec<String> {
        vec!["mara".to_owned()]
    }

    #[test]
    fn a_mention_is_the_account_after_an_at_or_a_jira_tilde() {
        assert!(mentions("@mara can you pick this up?", &me()));
        assert!(mentions("[~mara] can you pick this up?", &me()));
        assert!(mentions("Thanks @mara", &me()));
        assert!(!mentions("@mara.lindqvist is someone else", &me()));
        assert!(!mentions("@marathon", &me()));
        assert!(!mentions("mara wrote this", &me()));
        assert!(
            !mentions("@Mara", &me()),
            "case-sensitive, as the account is spelled"
        );
        assert!(!mentions("@mara", &[String::new()]));
        assert!(
            mentions("@maraé", &me()),
            "the inbox's ASCII name class, so both call it a mention"
        );
    }

    #[test]
    fn a_jira_comment_and_a_mock_comment_read_the_same() {
        let jira = serde_json::json!({
            "author": { "name": "priya", "displayName": "Priya Nair" },
            "created": "2026-08-12T09:40:00.000+0200",
            "body": "Yes. [~mara] can you pick this up?",
        });
        let read = comment_of(&jira, &me(), knobas_source::Markup::JiraWiki).unwrap();
        assert_eq!(read.author.as_deref(), Some("Priya Nair"));
        assert_eq!(read.at, Some("2026-08-12T07:40:00Z".parse().unwrap()));
        assert!(read.mentions_me);
        assert!(!read.mine);

        let mock =
            serde_json::json!({ "who": "mara", "when": "2026-08-12T10:02:00Z", "text": "Sure." });
        let read = comment_of(&mock, &me(), knobas_source::Markup::Text).unwrap();
        assert_eq!(read.author.as_deref(), Some("mara"));
        assert!(read.mine);
        assert!(!read.mentions_me);
    }

    #[test]
    fn a_comment_with_no_text_is_not_a_comment_and_a_bad_stamp_is_no_time() {
        assert_eq!(
            comment_of(&serde_json::json!({ "body": "  " }), &me(), knobas_source::Markup::Text),
            None
        );
        assert_eq!(comment_of(&serde_json::json!({ "body": 4 }), &me(), knobas_source::Markup::Text), None);
        let undated = comment_of(
            &serde_json::json!({ "body": "x", "created": "yesterday" }),
            &me(),
            knobas_source::Markup::Text,
        )
        .unwrap();
        assert_eq!(undated.at, None);
        assert_eq!(undated.author, None);
    }
}
