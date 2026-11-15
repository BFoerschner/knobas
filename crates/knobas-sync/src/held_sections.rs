//! Each open pull request's *Files*, held before anyone opens it (the
//! held-files pass of PR #2479, 2026-11-15; migration `0091`, ADR-0042 as
//! amended that day).
//!
//! **Why.** A pull request's *Files changed* was the one first open left that
//! waited a round of the forge (the record, the reviews, the commits and the
//! `.diff` side by side), then drew a diff of hundreds of files. Now the sync
//! reads it behind a run, and a page's first open draws it from
//! `sync.held_read` at once, under the held line, and checks the forge behind
//! as it does any read a person made before (`LiveSources::shown`).
//!
//! **What is held.** The adapter names one section of one kind
//! ([`HeldShape`]: Gitea's pull requests' `files`), read through
//! [`Source::view`], the read the page makes, so the held answer is the
//! page's own; the app's own work on an answer (ADR-0020 rule 7) is done on
//! every draw, as for every held read. A *Files* row may be up to
//! [`held_read::FILES_ROW_CAP`], so a pull request of thousands of files is
//! held too.
//!
//! **Only open records whose commits or threads moved.** A record the mirror
//! holds open is read when the two commits its section stands at (a pull
//! request's merge base and head) are not those the held copy stands at, when
//! the record's count of line comments (`review_comments`) is not the one the
//! copy was held at, or when nothing is held. The whole section is read again,
//! never spliced. A note or a record that names no count takes the record's
//! with no read (rows held before the count was kept, a forge that does not
//! send it). What the count misses -- an edited comment, a resolved thread, a
//! comment deleted and another added -- the page's read behind brings,
//! replacing the held threads in place. An unchanged run costs no request and
//! writes nothing.
//!
//! **Behind a person, capped.** The pass runs after a run, on the run's task,
//! in the background lane: every request yields to a person's on the same
//! server, and passes the per-server quota. At most [`REQUESTS_PER_RUN`]
//! requests inside [`BUDGET`] a run, the most recently updated first; a record
//! the forge failed on, or whose read had the whole budget and did not end,
//! goes last in the runs after, so it holds up no other. Each pass that sent a
//! request logs what it did and which bound stopped it.
//!
//! **Evicted first.** Its rows are `speculative` until a page draws one
//! ([`crate::held_read`]): when the held store is full they go before any row
//! a person read. The pass itself keeps them under [`SHARE`] of the store and
//! under what a person's reads leave, so it never writes a row the bound
//! would drop at once, and never reads one again every run for that reason;
//! rows over [`held_read::ROW_CAP`] take at most half that share between
//! them, so a few large pull requests never push the ordinary ones out. A
//! record no longer open drops its speculative row.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use chrono::{DateTime, Utc};
use knobas_source::Source;
use knobas_source::held::HeldShape;
use sqlx::PgPool;

use crate::held_read;

/// The most requests one run's pass sends: at Gitea's ten a second, twenty
/// seconds of the quota, which the priority lane gives up to a person's reads
/// whenever they come. A pull request's *Files* is about four requests and
/// one a review with line comments, so forty or so a run.
pub const REQUESTS_PER_RUN: u32 = 200;

/// How long one run's pass spends reading, at most.
pub const BUDGET: Duration = Duration::from_secs(45);

/// The share of the held store ([`held_read::BOUND`]) the pass fills, at
/// most: the rest is for what people read.
pub const SHARE: i64 = held_read::BOUND / 2;

/// What one pass did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Held {
    /// Sections read and held.
    pub read: usize,
    /// Sections read and not held for their size: over the ask's cap, or past
    /// the large rows' half of the share. Noted, so not read again until
    /// their commits or count move.
    pub too_large: usize,
    /// Sections the store refused (a text `jsonb` cannot hold) or the source
    /// answered about (a 404): noted, so not read again until their commits
    /// move.
    pub refused: usize,
    /// Sections a person's read already holds at the record's commits, or a
    /// note that took the record's count: noted, with no request.
    pub noted: usize,
    /// Requests sent.
    pub requests: u32,
    /// Sections owed and left for a later run (the budget, a forge that did
    /// not answer).
    pub left: usize,
    /// Sections owed past the store's share, not read.
    pub past_share: usize,
    /// Speculative rows dropped: their record closed, or past the share.
    pub dropped: u64,
    /// Bytes of answers written.
    pub bytes: u64,
    /// What stopped the pass short of every owed section: `requests`, `time`,
    /// `forge` or `share`; `None` where nothing did.
    pub stopped_by: Option<&'static str>,
}

/// A note's size where no row can be held at its commits: the store refused
/// the answer, or the source answered about the record (a 404). Read again
/// only when its commits or count move.
const UNHOLDABLE: i32 = -1;

/// A note's size where the source failed as a whole on this record (a 5xx, a
/// refused credential, no answer, or a read that had the whole budget and did
/// not end): the run stopped there, and the runs after it read this record
/// last, so one that always fails holds up no other.
const FAILED: i32 = -2;

/// A note's size where the answer fits its ask's cap but the large rows
/// already take their half of the share: not held, and read again only when
/// its commits or count move.
const PAST_LARGE: i32 = -3;

/// One record the mirror holds open, with what the store holds of it.
#[derive(Debug, Clone, sqlx::FromRow)]
struct Open {
    entity_id: String,
    base: Option<String>,
    head: Option<String>,
    /// The record's count of what the section's threads are made of, where
    /// the shape names one.
    count: Option<String>,
    /// The commits and count the pass last held its section at, under this
    /// recipe, and its answer's size (or a sentinel above).
    held_base: Option<String>,
    held_head: Option<String>,
    held_count: Option<String>,
    held_bytes: Option<i32>,
    /// The held row's size and whether it is speculative, where one is held
    /// under this recipe.
    row_bytes: Option<i32>,
    speculative: Option<bool>,
    /// Where the pass's note does not stand at the record's commits but a row
    /// is held: whether the row's own diff does (a person read it).
    row_stands: Option<bool>,
}

/// What the store holds of one open record's section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stand {
    /// Held at the record's commits and count, or known to be unholdable.
    Held,
    /// Held at the record's commits; the note names no count and the record
    /// does: the note takes it, with no read.
    Adopt,
    /// A person's read holds it at the record's commits since the pass last
    /// read it: noted, with no read.
    PersonRead,
    /// Read it.
    Owed,
}

impl Open {
    /// Both commits, each a whole sha.
    fn commits(&self) -> Option<(&str, &str)> {
        let whole =
            |s: &str| (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit());
        match (self.base.as_deref(), self.head.as_deref()) {
            (Some(base), Some(head)) if whole(base) && whole(head) => Some((base, head)),
            _ => None,
        }
    }

    fn stand(&self, ask: &str) -> Stand {
        let commits = self.held_base == self.base && self.held_head == self.head;
        // A row, or a note that none can be.
        let row = self.row_bytes.is_some()
            || self.held_bytes.is_some_and(|b| {
                b == UNHOLDABLE
                    || b == PAST_LARGE
                    || usize::try_from(b).is_ok_and(|b| b > held_read::row_cap(ask))
            });
        if commits && row {
            return match (self.held_count.as_deref(), self.count.as_deref()) {
                (Some(held), Some(now)) if held != now => Stand::Owed,
                (None, Some(_)) => Stand::Adopt,
                _ => Stand::Held,
            };
        }
        if self.row_stands == Some(true) {
            return Stand::PersonRead;
        }
        Stand::Owed
    }

    /// The bytes this record's speculative row takes in the store.
    fn speculative_bytes(&self) -> i64 {
        match (self.speculative, self.row_bytes) {
            (Some(true), Some(bytes)) => i64::from(bytes),
            _ => 0,
        }
    }

    /// The bytes of its speculative row where it is over [`held_read::ROW_CAP`].
    fn large_bytes(&self) -> i64 {
        let bytes = self.speculative_bytes();
        if usize::try_from(bytes).is_ok_and(|b| b > held_read::ROW_CAP) {
            bytes
        } else {
            0
        }
    }
}

/// One pass at a time a source.
fn passing() -> &'static Mutex<HashSet<String>> {
    static PASSING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    PASSING.get_or_init(Default::default)
}

struct Passing(String);

impl Drop for Passing {
    fn drop(&mut self) {
        passing()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.0);
    }
}

/// Read and hold the section of each of `source_id`'s open records whose
/// commits or count moved since it was held, most recently updated first,
/// inside the run's budget and the store's share; drop the speculative rows of
/// records no longer open or past the share.
///
/// # Errors
///
/// A database failure. A section the source fails to answer is left for the
/// next run, not an error.
pub async fn refresh(
    pool: &PgPool,
    source_id: &str,
    source: &dyn Source,
) -> Result<Held, sqlx::Error> {
    refresh_within(pool, source_id, source, REQUESTS_PER_RUN, BUDGET, SHARE).await
}

/// [`refresh`] with its budget given: at most `requests` requests inside
/// `budget`, and the pass's rows at most `share` bytes of the store.
///
/// # Errors
///
/// As [`refresh`].
pub async fn refresh_within(
    pool: &PgPool,
    source_id: &str,
    source: &dyn Source,
    requests: u32,
    budget: Duration,
    share: i64,
) -> Result<Held, sqlx::Error> {
    let mut done = Held::default();
    let Some(shape) = source.held_shape() else {
        return Ok(done);
    };
    let Some(cfg) = crate::config::get(pool, source_id).await? else {
        return Ok(done);
    };
    {
        let mut busy = passing().lock().unwrap_or_else(PoisonError::into_inner);
        if !busy.insert(source_id.to_owned()) {
            return Ok(done);
        }
    }
    let _passing = Passing(source_id.to_owned());
    let recipe = held_read::recipe(
        &cfg.adapter_kind,
        &cfg.base_url,
        cfg.auth_kind.method(),
        &cfg.config,
    );
    let ask = held_read::view_ask(Some(shape.section));
    let open = plan(pool, source_id, &shape, &ask, &recipe).await?;
    // What the pass may fill: its share, and no more than a person's reads
    // leave of the store; the rows over the ordinary cap, half of that.
    let people: i64 = sqlx::query_scalar(
        "select coalesce(sum(bytes), 0)::bigint from sync.held_read where not speculative",
    )
    .fetch_one(pool)
    .await?;
    let allowance = share.min(held_read::BOUND - people);
    let large_allowance = share / 2;
    let (mut filled, mut large): (i64, i64) = (0, 0);
    let mut past: Vec<String> = Vec::new();
    let deadline = tokio::time::Instant::now() + budget;
    for record in &open {
        let Some((base, head)) = record.commits() else {
            continue;
        };
        let count = record.count.as_deref();
        let stand = record.stand(&ask);
        if filled >= allowance {
            if record.speculative == Some(true) || record.row_bytes.is_none() {
                past.push(record.entity_id.clone());
            }
            if stand == Stand::Owed {
                done.past_share += 1;
                done.stopped_by.get_or_insert("share");
            }
            continue;
        }
        match stand {
            Stand::Held => {
                filled += record.speculative_bytes();
                large += record.large_bytes();
                continue;
            }
            Stand::Adopt => {
                sqlx::query(
                    "update sync.held_section set count = $4
                      where source_id = $1 and entity_id = $2 and ask = $3",
                )
                .bind(source_id)
                .bind(&record.entity_id)
                .bind(&ask)
                .bind(count)
                .execute(pool)
                .await?;
                done.noted += 1;
                filled += record.speculative_bytes();
                large += record.large_bytes();
                continue;
            }
            Stand::PersonRead => {
                let at = Note {
                    base,
                    head,
                    count,
                    bytes: record.row_bytes.unwrap_or(0),
                    read_at: Utc::now(),
                };
                note(pool, source_id, &record.entity_id, &ask, &recipe, &at).await?;
                done.noted += 1;
                filled += record.speculative_bytes();
                large += record.large_bytes();
                continue;
            }
            Stand::Owed => {}
        }
        if done.stopped_by.is_some() {
            done.left += 1;
            continue;
        }
        if done.requests >= requests {
            done.left += 1;
            done.stopped_by = Some("requests");
            continue;
        }
        if tokio::time::Instant::now() >= deadline {
            done.left += 1;
            done.stopped_by = Some("time");
            continue;
        }
        let read_at = Utc::now();
        let whole_budget = done.requests == 0;
        let (read, sent) = knobas_http::counted::counted(tokio::time::timeout_at(
            deadline,
            source.view(&record.entity_id, Some(shape.section)),
        ))
        .await;
        done.requests += u32::try_from(sent).unwrap_or(u32::MAX);
        let mut at = Note {
            base,
            head,
            count,
            bytes: FAILED,
            read_at,
        };
        let view = match read {
            // The budget ran out under this read. One that had the whole
            // budget would take it again every run: it goes last, as a
            // failure does.
            Err(_) => {
                done.left += 1;
                done.stopped_by = Some("time");
                if whole_budget {
                    note(pool, source_id, &record.entity_id, &ask, &recipe, &at).await?;
                }
                continue;
            }
            // A forge that is down, refuses the credential or is overloaded
            // fails every record alike: the run stops, and this record goes
            // last in the next. A failure about this record (a 404) is noted
            // at its commits and not asked again until they move.
            Ok(Err(error)) => {
                if crate::stored_trees::forge_failed(&error) {
                    tracing::debug!(source_id, entity = record.entity_id, %error, "the forge did not answer a held section; the pass stops");
                    done.left += 1;
                    done.stopped_by = Some("forge");
                } else {
                    tracing::debug!(source_id, entity = record.entity_id, %error, "a held section was not read");
                    done.refused += 1;
                    at.bytes = UNHOLDABLE;
                }
                note(pool, source_id, &record.entity_id, &ask, &recipe, &at).await?;
                continue;
            }
            Ok(Ok(view)) => view,
        };
        let Ok(answer) = serde_json::to_value(&view) else {
            continue;
        };
        // What the answer stands at, where it says: the forge may have moved
        // past the mirror's record since the run.
        let (base, head) = stands_at(&answer).unwrap_or((base.to_owned(), head.to_owned()));
        let size = |bytes: usize| i32::try_from(bytes).unwrap_or(i32::MAX);
        let bytes = serde_json::to_vec(&answer).map_or(0, |b| b.len());
        // A large row is held only inside the large rows' half of the share
        // and the share itself; past either it is noted, not held.
        let fits = bytes <= held_read::ROW_CAP || {
            let more = i64::try_from(bytes).unwrap_or(i64::MAX);
            large + more <= large_allowance && filled + more <= allowance
        };
        let put = if fits {
            held_read::put_as(
                pool,
                source_id,
                &record.entity_id,
                &ask,
                &recipe,
                read_at,
                &answer,
                true,
            )
            .await
        } else {
            held_read::Put::TooLarge(bytes)
        };
        // Noted whatever became of it, so a section the store cannot hold is
        // not read again every run.
        at.bytes = match put {
            held_read::Put::Held(bytes) => {
                done.read += 1;
                done.bytes += bytes as u64;
                // Speculative unless a person's row was there.
                if record.speculative != Some(false) {
                    let more = i64::from(size(bytes));
                    filled += more;
                    if bytes > held_read::ROW_CAP {
                        large += more;
                    }
                }
                size(bytes)
            }
            held_read::Put::TooLarge(bytes) => {
                done.too_large += 1;
                if bytes > held_read::row_cap(&ask) {
                    size(bytes)
                } else {
                    PAST_LARGE
                }
            }
            held_read::Put::Refused => {
                done.refused += 1;
                UNHOLDABLE
            }
        };
        let at = Note {
            base: &base,
            head: &head,
            ..at
        };
        note(pool, source_id, &record.entity_id, &ask, &recipe, &at).await?;
    }
    done.dropped = drop_unheld(pool, source_id, &ask, &open, &past).await?;
    if done.requests > 0 || done.stopped_by.is_some() {
        tracing::info!(
            source_id,
            read = done.read,
            too_large = done.too_large,
            refused = done.refused,
            requests = done.requests,
            left = done.left,
            past_share = done.past_share,
            stopped_by = done.stopped_by.unwrap_or("nothing"),
            "held the open pull requests' files"
        );
    }
    Ok(done)
}

/// The commits a held answer's diff stands at, where it holds one diff with
/// a range.
fn stands_at(answer: &serde_json::Value) -> Option<(String, String)> {
    let mut ranges = answer
        .get("sections")?
        .as_array()?
        .iter()
        .flat_map(|s| {
            s.get("blocks")
                .and_then(|b| b.as_array())
                .into_iter()
                .flatten()
        })
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("diff"))
        .filter_map(|b| b.get("range"));
    let range = ranges.next()?;
    Some((
        range.get("base")?.as_str()?.to_owned(),
        range.get("head")?.as_str()?.to_owned(),
    ))
}

/// What the pass notes of a section: the commits and count it stands at, its
/// answer's size (or a sentinel), and when it was read.
#[derive(Debug, Clone, Copy)]
struct Note<'a> {
    base: &'a str,
    head: &'a str,
    count: Option<&'a str>,
    bytes: i32,
    read_at: DateTime<Utc>,
}

/// Note what the pass holds of the section of `entity_id`.
async fn note(
    pool: &PgPool,
    source_id: &str,
    entity_id: &str,
    ask: &str,
    recipe: &serde_json::Value,
    at: &Note<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into sync.held_section
                (source_id, entity_id, ask, recipe, base, head, count, bytes, read_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         on conflict (source_id, entity_id, ask) do update
           set recipe = excluded.recipe, base = excluded.base, head = excluded.head,
               count = excluded.count, bytes = excluded.bytes, read_at = excluded.read_at",
    )
    .bind(source_id)
    .bind(entity_id)
    .bind(ask)
    .bind(recipe)
    .bind(at.base)
    .bind(at.head)
    .bind(at.count)
    .bind(at.bytes)
    .bind(at.read_at)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Drop the speculative rows, and the pass's notes, of records the mirror no
/// longer holds open, and of `past` (past the share). How many rows went.
async fn drop_unheld(
    pool: &PgPool,
    source_id: &str,
    ask: &str,
    open: &[Open],
    past: &[String],
) -> Result<u64, sqlx::Error> {
    let kept: Vec<&str> = open
        .iter()
        .map(|o| o.entity_id.as_str())
        .filter(|id| !past.iter().any(|p| p == id))
        .collect();
    let dropped = sqlx::query(
        "delete from sync.held_read
          where source_id = $1 and ask = $2 and speculative and entity_id <> all($3)",
    )
    .bind(source_id)
    .bind(ask)
    .bind(&kept)
    .execute(pool)
    .await?
    .rows_affected();
    sqlx::query(
        "delete from sync.held_section s
          where s.source_id = $1 and s.ask = $2 and s.entity_id <> all($3)
            and not exists (select 1 from sync.held_read h
                             where h.source_id = s.source_id and h.entity_id = s.entity_id
                               and h.ask = s.ask)",
    )
    .bind(source_id)
    .bind(ask)
    .bind(&kept)
    .execute(pool)
    .await?;
    Ok(dropped)
}

/// Every record of the shape's kind the mirror holds open for `source_id`,
/// most recently updated first, with what the store holds of its section.
async fn plan(
    pool: &PgPool,
    source_id: &str,
    shape: &HeldShape,
    ask: &str,
    recipe: &serde_json::Value,
) -> Result<Vec<Open>, sqlx::Error> {
    let path = |p: &[&str]| p.iter().map(|s| (*s).to_owned()).collect::<Vec<String>>();
    sqlx::query_as::<_, Open>(PLAN)
        .bind(source_id)
        .bind(shape.kind)
        .bind(path(shape.state))
        .bind(shape.open)
        .bind(path(shape.base))
        .bind(path(shape.head))
        .bind(ask)
        .bind(recipe)
        .bind(shape.count.map(path))
        .fetch_all(pool)
        .await
}

/// The plan: the open records of kind `$2`, their commits (`$5`, `$6`) and
/// count (`$9`, none where the shape names none), the pass's note of the
/// section `$7` under recipe `$8`, and the held row's size; where the note
/// does not stand at the record's commits and a row is held, whether the
/// row's own diff does (a person read it since). Only then is a held answer
/// read, so an unchanged run reads no answer. The most recently updated
/// first, but a record the forge failed on at these commits last.
const PLAN: &str = r#"
with open as (
  select i.entity_id, i.item_updated_at,
         i.payload #>> $5::text[] as base, i.payload #>> $6::text[] as head,
         i.payload #>> $9::text[] as count
    from sync.item i
    join knobas.entity e on e.id = i.entity_id and e.deleted_at is null
   where i.source_id = $1 and i.kind = $2 and i.payload #>> $3::text[] = $4
)
select o.entity_id, o.base, o.head, o.count,
       s.base as held_base, s.head as held_head, s.count as held_count, s.bytes as held_bytes,
       h.bytes as row_bytes, h.speculative,
       case when h.entity_id is not null
             and (s.base is distinct from o.base or s.head is distinct from o.head)
            then jsonb_path_exists(h.answer,
                   '$.sections[*].blocks[*] ? (@.type == "diff" && @.range.base == $b && @.range.head == $h)',
                   jsonb_build_object('b', o.base, 'h', o.head))
       end as row_stands
  from open o
  left join sync.held_section s
         on s.source_id = $1 and s.entity_id = o.entity_id and s.ask = $7 and s.recipe = $8
  left join sync.held_read h
         on h.source_id = $1 and h.entity_id = o.entity_id and h.ask = $7 and h.recipe = $8
 order by (s.bytes = -2 and s.base = o.base and s.head = o.head) is true,
          o.item_updated_at desc nulls last, o.entity_id
"#;
