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
//! * [`thread`] -- the **comments** and the **changes**. The comments are a
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
//!
//! # What *changes* are
//!
//! *Include changes* interleaves quiet lines between the comments (R-5.2),
//! from three records, none of them a comment (R-5.1):
//!
//! * **Status moves made anywhere** (#929): the Jira adapter asks `/search`
//!   for `expand=changelog`, so the stored payload carries Jira's
//!   `changelog.histories`; each history item on the `status` field is a line
//!   with who moved it, from what and to what. A named payload read,
//!   [`HISTORIES`], as [`COMMENTS`] is.
//! * **Status moves knobas delivered** (#819): a `transition` write the queue
//!   settled as `sent`. Jira's changelog holds the same move once the mirror
//!   has synced after it, so a knobas move is drawn only while the changelog
//!   has no move to the same status from around its queueing on
//!   ([`SKEW`]) -- and for a record with no changelog at all (the mock
//!   adapter, or a ticket not synced since the adapter started asking).
//! * **Pull-request events** (#929): the opening and the merge of every pull
//!   request confirm-linked to the ticket, read through the `opened` and
//!   `merged_at` times its source declares (ADR-0007), named by the `#N` its
//!   key ends in (the signals' rule) and its repository's shortest declared
//!   name.
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ThreadComment {
    /// Who wrote it, as the source names them to a person (Jira's
    /// `displayName`), else their account. `None` when the record says nobody.
    pub author: Option<String>,
    /// When it was written. `None` when the source's stamp is absent or reads
    /// as no time: such a comment keeps its place in the source's order.
    pub at: Option<DateTime<Utc>>,
    /// The text as the source wrote it (Jira's wiki markup), never markup here.
    pub body: String,
    /// The reader wrote it: its author's account is one of the identity's.
    pub mine: bool,
    /// The body names the reader (R-5.3), by [`mentions`]'s rule.
    pub mentions_me: bool,
}

/// What a [`ThreadChange`] says happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    /// The ticket was moved to another status.
    Status,
    /// A linked pull request was opened.
    Opened,
    /// A linked pull request was merged.
    Merged,
}

/// One change, drawn between the comments when *Include changes* is on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ThreadChange {
    pub at: DateTime<Utc>,
    pub kind: ChangeKind,
    /// A status move's new status. `None` for a pull-request line.
    pub status: Option<String>,
    /// A status move's old status, as Jira's changelog names it. `None` for a
    /// move knobas sent (the queue records only where it went) and for a
    /// pull-request line.
    pub from: Option<String>,
    /// Who moved it, as the source names them to a person (Jira's
    /// `displayName`), else their account. `None` for a move knobas sent
    /// (the reader, `mine`) and for a pull-request line.
    pub by: Option<String>,
    /// The reader made it: a move knobas sent, or a changelog entry whose
    /// author's account is one of the identity's.
    pub mine: bool,
    /// A pull request's number, the `#N` its key ends in, without the `#`.
    pub number: Option<String>,
    /// A pull request's repository, the shortest name it is declared under.
    pub repo: Option<String>,
}

/// A ticket's thread, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Thread {
    pub comments: Vec<ThreadComment>,
    pub changes: Vec<ThreadChange>,
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
       '[]'::jsonb) as comments
  from sync.item
 where entity_id = $1";

/// A knobas-delivered status move on `$1`: a settled `transition` write.
///
/// `payload` is `knobas_source::WriteOp`'s serde shape, externally tagged
/// (`{"Transition": {"entity", "status"}}`); a drifted shape misses and the
/// move is not drawn, which is the safe direction for a quiet line. Pinned by
/// `a_status_move_knobas_sent_is_a_change_and_not_a_comment`.
const MOVES: &str = "select coalesce(settled_at, queued_at) as at, queued_at,
       payload #>> '{Transition,status}' as status
  from knobas.write_queue
 where entity_id = $1 and op = 'transition' and state = 'sent'
   and jsonb_typeof(payload #> '{Transition,status}') = 'string'
 order by at, id";

/// **Jira's status history, the second named payload read** (ADR-0007):
/// `changelog.histories`, which `/search` carries since the adapter asks for
/// `expand=changelog` (#929). `None` when the record has no such array --
/// another adapter, or a ticket the mirror has not synced since -- and then
/// the knobas-sent moves stand alone. [`moves_of`] reads each entry.
///
/// Failure direction as [`COMMENTS`]': a drifted shape is no line, never a
/// wrong one. Pinned by `a_changelog_of_another_shape_reads_as_no_moves`.
const HISTORIES: &str =
    "select case when jsonb_typeof(payload #> '{changelog,histories}') = 'array'
            then payload #> '{changelog,histories}' end as histories
  from sync.item
 where entity_id = $1";

/// The pull requests confirm-linked to `$1`, either direction, with the times
/// their source declares (`$2`, the declarations): one row each, live ones
/// only. The number is the `#N` the key ends in, as the signals read it.
const PULLS: &str = concat!(
    "select substring(i.entity_id from '#([0-9]+)$') as num, (select n.name from ",
    knobas_core::declared_parent_names!("$2"),
    " n order by length(n.name), n.name limit 1) as repo, ",
    knobas_core::declared_string!("$2", "opened"),
    " as opened, ",
    knobas_core::declared_string!("$2", "merged_at"),
    " as merged from sync.item i
 where i.kind = 'pr' and ",
    knobas_core::live_by_key!("i"),
    " and i.entity_id in (select l.to_id from knobas.confirmed_link l where l.from_id = $1
                       union select l.from_id from knobas.confirmed_link l where l.to_id = $1)"
);

/// How far before its queueing a changelog move to the same status still
/// counts as Jira's record of a move knobas sent: the two clocks are the
/// server's and this machine's.
const SKEW: chrono::TimeDelta = chrono::TimeDelta::minutes(2);

/// The status moves of a changelog's `histories`, oldest first: every item on
/// the `status` field of every entry that has a time and a new status.
fn moves_of(histories: &serde_json::Value, me: &[String]) -> Vec<ThreadChange> {
    let mut moves: Vec<ThreadChange> = histories
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|entry| {
            let at = text_at(entry, &["created"]).and_then(stamp);
            let account =
                text_at(entry, &["author", "name"]).or_else(|| text_at(entry, &["author", "key"]));
            let by = text_at(entry, &["author", "displayName"]).or(account);
            entry
                .get("items")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter(|item| text_at(item, &["field"]) == Some("status"))
                .filter_map(move |item| {
                    Some(ThreadChange {
                        at: at?,
                        kind: ChangeKind::Status,
                        status: Some(text_at(item, &["toString"])?.to_owned()),
                        from: text_at(item, &["fromString"]).map(str::to_owned),
                        by: by.map(str::to_owned),
                        mine: account.is_some_and(|a| me.iter().any(|name| name == a)),
                        number: None,
                        repo: None,
                    })
                })
        })
        .collect();
    moves.sort_by_key(|change| change.at);
    moves
}

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
fn comment_of(entry: &serde_json::Value, me: &[String]) -> Option<ThreadComment> {
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
/// and the Jira adapter reads it so), and its changes oldest first: status
/// moves from Jira's changelog and from the queue, and the opening and merge
/// of each linked pull request (the module's *What changes are*).
///
/// # Errors
///
/// `invalid` for something that is not an entity id, `not_found` for one the
/// mirror does not hold, `internal` for a query failure.
pub async fn thread(pool: &PgPool, entity_id: &str) -> Result<Thread, IpcError> {
    let entity = EntityRef::parse(entity_id).map_err(IpcError::invalid)?;
    let id = entity.to_string();
    let row = sqlx::query(COMMENTS)
        .bind(&id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("{id} is not in the mirror")))?;
    let list: serde_json::Value = row.get("comments");
    let me = identity(pool).await?;
    let comments: Vec<ThreadComment> = list
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| comment_of(entry, &me))
                .collect()
        })
        .unwrap_or_default();
    let histories: Option<serde_json::Value> = sqlx::query_scalar(HISTORIES)
        .bind(&id)
        .fetch_one(pool)
        .await?;
    let mut changes = histories
        .as_ref()
        .map(|histories| moves_of(histories, &me))
        .unwrap_or_default();
    let recorded = changes.clone();
    for row in sqlx::query(MOVES).bind(&id).fetch_all(pool).await? {
        let queued: DateTime<Utc> = row.get("queued_at");
        let status: String = row.get("status");
        let in_jira = recorded
            .iter()
            .any(|m| m.status.as_deref() == Some(status.as_str()) && m.at >= queued - SKEW);
        if !in_jira {
            changes.push(ThreadChange {
                at: row.get("at"),
                kind: ChangeKind::Status,
                status: Some(status),
                from: None,
                by: None,
                mine: true,
                number: None,
                repo: None,
            });
        }
    }
    let declarations =
        crate::sources::paths::declared_paths(pool, &crate::sources::Registry::for_process())
            .await?;
    for row in sqlx::query(PULLS)
        .bind(&id)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?
    {
        let number: Option<String> = row.get("num");
        let repo: Option<String> = row.get("repo");
        for (kind, column) in [
            (ChangeKind::Opened, "opened"),
            (ChangeKind::Merged, "merged"),
        ] {
            let at = row
                .get::<Option<String>, _>(column)
                .as_deref()
                .and_then(knobas_core::payload::parse_instant);
            if let Some(at) = at {
                changes.push(ThreadChange {
                    at,
                    kind,
                    status: None,
                    from: None,
                    by: None,
                    mine: false,
                    number: number.clone(),
                    repo: repo.clone(),
                });
            }
        }
    }
    // Stable: same-instant lines keep the order they were gathered in.
    changes.sort_by_key(|change| change.at);
    Ok(Thread {
        comments,
        changes,
        me,
    })
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
        let read = comment_of(&jira, &me()).unwrap();
        assert_eq!(read.author.as_deref(), Some("Priya Nair"));
        assert_eq!(read.at, Some("2026-08-12T07:40:00Z".parse().unwrap()));
        assert!(read.mentions_me);
        assert!(!read.mine);

        let mock =
            serde_json::json!({ "who": "mara", "when": "2026-08-12T10:02:00Z", "text": "Sure." });
        let read = comment_of(&mock, &me()).unwrap();
        assert_eq!(read.author.as_deref(), Some("mara"));
        assert!(read.mine);
        assert!(!read.mentions_me);
    }

    /// The wire shape and its hand-written mirror (`app/src/lib/ipc/entity.ts`)
    /// name the same fields and the same two commands.
    #[test]
    fn the_thread_and_its_mirror_agree() {
        let mirror = include_str!("../../../app/src/lib/ipc/entity.ts");
        let thread = serde_json::to_value(Thread {
            comments: vec![ThreadComment {
                author: None,
                at: None,
                body: String::new(),
                mine: false,
                mentions_me: false,
            }],
            changes: vec![ThreadChange {
                at: Utc::now(),
                kind: ChangeKind::Opened,
                status: None,
                from: None,
                by: None,
                mine: false,
                number: None,
                repo: None,
            }],
            me: vec![],
        })
        .unwrap();
        let fields = |value: &serde_json::Value| -> Vec<String> {
            value.as_object().unwrap().keys().cloned().collect()
        };
        for key in fields(&thread)
            .iter()
            .chain(&fields(&thread["comments"][0]))
            .chain(&fields(&thread["changes"][0]))
        {
            assert!(
                mirror.contains(&format!("  {key}:")),
                "{key} is not mirrored"
            );
        }
        assert!(mirror.contains("export interface Thread {"));
        for kind in [ChangeKind::Status, ChangeKind::Opened, ChangeKind::Merged] {
            let word = serde_json::to_value(kind).unwrap();
            assert!(
                mirror.contains(&format!("\"{}\"", word.as_str().unwrap())),
                "{word} is not mirrored"
            );
        }
        assert!(mirror.contains("\"visit_entity\""));
        assert!(mirror.contains("\"entity_thread\""));
    }

    #[test]
    fn a_comment_with_no_text_is_not_a_comment_and_a_bad_stamp_is_no_time() {
        assert_eq!(
            comment_of(&serde_json::json!({ "body": "  " }), &me()),
            None
        );
        assert_eq!(comment_of(&serde_json::json!({ "body": 4 }), &me()), None);
        let undated = comment_of(
            &serde_json::json!({ "body": "x", "created": "yesterday" }),
            &me(),
        )
        .unwrap();
        assert_eq!(undated.at, None);
        assert_eq!(undated.author, None);
    }
}
