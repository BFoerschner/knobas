//! Carry-over (M0 → M1, stream F): "a full sync cannot express items the source
//! stopped returning; rows stay live forever."
//!
//! The reconciliation is the one interfaces §1 specifies: after a `cursor:
//! None` run, tombstone every entity whose mirror row was not touched by this
//! run. No `last_seen_at` column is needed -- `sync.item.synced_at` is already
//! the run's transaction timestamp, identical for every row the run wrote, so
//! `synced_at < now()` inside that same transaction means exactly "this run did
//! not see it".
//!
//! **Gated on `KindInfo::full_sync_exhaustive`, per kind** (ADR-0003). The
//! inference "this full sync did not return it, therefore it is gone" only
//! holds where the full sync really does return everything -- and that is a
//! property of each *kind*, not of a source. TeamCity's builds are the newest
//! N per configuration, so both its kinds declare `false` and are never swept.
//! Gitea enumerates every repository and every branch but budgets commits and
//! pull requests, so it declares two of each. Jira's one kind and every one of
//! the mock's declare `true`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_source::{
    Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, WriteOp,
};
use sqlx::PgPool;

/// An adapter emitting whichever `(kind, key)` pairs it is currently told to,
/// over a fixed set of declared kinds.
struct Shrinking {
    id: String,
    keys: Arc<Mutex<Vec<(&'static str, &'static str)>>>,
    /// The kinds this source declares, each with the gate the sweep obeys:
    /// `true` means "a full sync returns every item of this kind I have",
    /// which is what makes absence proof of deletion -- **for that kind
    /// alone**.
    kinds: Vec<(&'static str, bool)>,
}

#[async_trait]
impl Source for Shrinking {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "shrinking".into(),
            name: "Shrinking".into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: self
                .kinds
                .iter()
                .map(|(id, exhaustive)| KindInfo {
                    id: (*id).to_owned(),
                    label: (*id).to_owned(),
                    plural: (*id).to_owned(),
                    monogram: "SH".into(),
                    full_sync_exhaustive: *exhaustive,
                })
                .collect(),
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            // Nothing declared: this stand-in has no payload shapes to
            // read, so every path-driven read misses on it (#277).
            payload_paths: Vec::new(),
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let keys = self.keys.lock().unwrap().clone();
        for (kind, key) in &keys {
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new(&self.id, key),
                kind: (*kind).to_owned(),
                title: format!("{key} title"),
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload: serde_json::json!({}),
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        // Battery clause 2: a run that emitted nothing returns the cursor it
        // was handed, byte-identical.
        if keys.is_empty() {
            return Ok(cursor.unwrap_or_default());
        }
        Ok(r#"{"v":1,"n":1}"#.to_owned())
    }

    async fn write(&self, _op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }
}

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

async fn deleted_at(pool: &PgPool, id: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let (d,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("select deleted_at from knobas.entity where id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    d
}

async fn live_count(pool: &PgPool, source_id: &str) -> i64 {
    let (n,): (i64,) = sqlx::query_as("select count(*) from sync.live_item where source_id = $1")
        .bind(source_id)
        .fetch_one(pool)
        .await
        .unwrap();
    n
}

fn unique() -> String {
    format!("swp-{}", uuid::Uuid::new_v4().simple())
}

/// What the shared handle to a source's current corpus is spelled as.
type Corpus = Arc<Mutex<Vec<(&'static str, &'static str)>>>;

/// Keys of the single `"ticket"` kind, for the tests that are not about the
/// per-kind gate.
fn tickets(keys: &[&'static str]) -> Vec<(&'static str, &'static str)> {
    keys.iter().map(|key| ("ticket", *key)).collect()
}

/// A source declaring `kinds` and currently emitting `keys`.
fn source_of(
    id: &str,
    kinds: &[(&'static str, bool)],
    keys: &[(&'static str, &'static str)],
) -> (Shrinking, Corpus) {
    let corpus: Corpus = Arc::new(Mutex::new(keys.to_vec()));
    (
        Shrinking {
            id: id.to_owned(),
            keys: Arc::clone(&corpus),
            kinds: kinds.to_vec(),
        },
        corpus,
    )
}

/// An adapter whose full sync returns everything it has -- the Jira/mock shape,
/// where absence really does mean deletion.
fn source(id: &str, keys: &[&'static str]) -> (Shrinking, Corpus) {
    source_of(id, &[("ticket", true)], &tickets(keys))
}

/// An adapter whose full sync is **bounded** -- the TeamCity shape: "the newest
/// N builds per configuration", so an item this run did not return may simply
/// have fallen off the window.
fn bounded_source(id: &str, keys: &[&'static str]) -> (Shrinking, Corpus) {
    source_of(id, &[("ticket", false)], &tickets(keys))
}

/// A second of daylight, so `synced_at` is unmistakably older than the next
/// run's transaction timestamp even at coarse clock resolution.
async fn a_moment_passes() {
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
}

#[tokio::test]
async fn a_full_sync_tombstones_what_the_source_stopped_returning_and_keeps_its_mirror_row() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source(&id, &["A-1", "A-2", "A-3"]);

    let first = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(first.upserted, 3);
    assert_eq!(first.swept, 0, "nothing is stale on the first full sync");

    // Upstream hard-deletes A-2: it simply stops appearing.
    *keys.lock().unwrap() = tickets(&["A-1", "A-3"]);
    a_moment_passes().await;

    let second = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(second.upserted, 2);
    assert_eq!(second.swept, 1, "the vanished item is reconciled");

    assert!(deleted_at(&pool, &format!("{id}:A-2")).await.is_some());
    assert!(deleted_at(&pool, &format!("{id}:A-1")).await.is_none());
    assert!(deleted_at(&pool, &format!("{id}:A-3")).await.is_none());

    // The mirror row survives, so the UI can still render the last-known title
    // of something that vanished upstream.
    let (title,): (String,) = sqlx::query_as("select title from sync.item where entity_id = $1")
        .bind(format!("{id}:A-2"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "A-2 title");

    // And `sync.live_item` -- the structural tombstone filter every reader gets
    // for free (interfaces §1, point 1) -- no longer offers it.
    assert_eq!(live_count(&pool, &id).await, 2);
}

/// **The gate.** Not every adapter's full sync is exhaustive: TeamCity's is
/// "the newest N builds per configuration", so an old build that this run did
/// not return has not been deleted -- it has fallen off the end of a bounded
/// window. Sweeping there would tombstone a source's entire history one page at
/// a time. The adapter declares which it is, and the engine believes it.
#[tokio::test]
async fn a_source_whose_full_sync_is_bounded_is_never_swept() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = bounded_source(&id, &["T-1", "T-2", "T-3"]);

    let first = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(first.upserted, 3);

    // The window slid: T-1 is simply older than the newest two builds.
    *keys.lock().unwrap() = tickets(&["T-2", "T-3"]);
    a_moment_passes().await;
    let second = knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(second.upserted, 2);
    assert_eq!(
        second.swept, 0,
        "a bounded full sync proves nothing about absence"
    );
    assert!(
        deleted_at(&pool, &format!("{id}:T-1")).await.is_none(),
        "an old build that fell out of the window is still a real build"
    );
    assert_eq!(live_count(&pool, &id).await, 3, "all three are still live");
}

#[tokio::test]
async fn an_incremental_run_never_sweeps() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source(&id, &["B-1", "B-2"]);
    let first = knobas_sync::run_once(&pool, &src, None).await.unwrap();

    *keys.lock().unwrap() = tickets(&["B-1"]);
    a_moment_passes().await;
    let inc = knobas_sync::run_once(&pool, &src, Some(first.cursor))
        .await
        .unwrap();

    assert_eq!(
        inc.swept, 0,
        "an incremental run has not seen the whole source"
    );
    assert!(
        deleted_at(&pool, &format!("{id}:B-2")).await.is_none(),
        "only a full sync knows that something is gone"
    );
}

/// A full sync that emitted **nothing** is indistinguishable from an adapter
/// that silently failed -- an expired token accepted with an empty 200, a
/// misconfigured project filter. Sweeping there would tombstone the entire
/// source. One stale row is cheap; wiping a corpus is not.
#[tokio::test]
async fn a_full_sync_that_emitted_nothing_sweeps_nothing() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source(&id, &["C-1", "C-2"]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    *keys.lock().unwrap() = tickets(&[]);
    a_moment_passes().await;
    let empty = knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!((empty.upserted, empty.swept), (0, 0));
    assert!(deleted_at(&pool, &format!("{id}:C-1")).await.is_none());
    assert!(deleted_at(&pool, &format!("{id}:C-2")).await.is_none());
    assert_eq!(live_count(&pool, &id).await, 2);
}

/// The sweep must keep the *first* deletion's timestamp, exactly as
/// `ENTITY_UPSERT` does -- a tombstone restamped on every run makes "deleted 3
/// days ago" say "deleted just now" for ever.
#[tokio::test]
async fn the_sweep_does_not_restamp_an_existing_tombstone() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source(&id, &["D-1", "D-2"]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    *keys.lock().unwrap() = tickets(&["D-1"]);
    a_moment_passes().await;
    let swept = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(swept.swept, 1);
    let first = deleted_at(&pool, &format!("{id}:D-2")).await.unwrap();

    a_moment_passes().await;
    let again = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(
        again.swept, 0,
        "an already-tombstoned row is not swept twice"
    );
    assert_eq!(
        deleted_at(&pool, &format!("{id}:D-2")).await.unwrap(),
        first
    );
}

/// The sweep is scoped to one source. Two sources sharing a database must not
/// tombstone each other's world.
#[tokio::test]
async fn the_sweep_only_touches_its_own_source() {
    let pool = pool().await;
    let mine = unique();
    let theirs = unique();
    let (a, keys) = source(&mine, &["E-1", "E-2"]);
    let (b, _) = source(&theirs, &["E-1"]);
    knobas_sync::run_once(&pool, &a, None).await.unwrap();
    knobas_sync::run_once(&pool, &b, None).await.unwrap();

    *keys.lock().unwrap() = tickets(&["E-1"]);
    a_moment_passes().await;
    let swept = knobas_sync::run_once(&pool, &a, None).await.unwrap();

    assert_eq!(swept.swept, 1, "its own vanished item is still swept");
    assert!(
        deleted_at(&pool, &format!("{theirs}:E-1")).await.is_none(),
        "another source's items are not this run's to tombstone"
    );
    assert_eq!(live_count(&pool, &theirs).await, 1);
}

/// Sources do resurrect things -- an issue is un-deleted, a repo restored --
/// and a tombstone the sweep wrote must come off again when the item comes
/// back, or a stale one hides a live entity for ever.
///
/// This asserts **resurrection and nothing else**. An earlier version of this
/// comment claimed it also proved the sweep runs inside the run's own
/// transaction; it does not, and that property is not reachable from a test at
/// this level: the sweep executes only after `Source::sync` has returned `Ok`,
/// and the only statements after it are the cursor update and the commit,
/// neither of which a test can force to fail without reaching inside the
/// engine. The property is held by construction instead -- `SWEEP` executes on
/// `&mut *tx`, the same transaction as the upserts -- and that is what the
/// reader should check, rather than trusting this test to have checked it.
#[tokio::test]
async fn a_resurrected_item_loses_its_tombstone_again() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source(&id, &["F-1", "F-2"]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    *keys.lock().unwrap() = tickets(&["F-1"]);
    a_moment_passes().await;
    assert_eq!(
        knobas_sync::run_once(&pool, &src, None)
            .await
            .unwrap()
            .swept,
        1
    );
    assert!(deleted_at(&pool, &format!("{id}:F-2")).await.is_some());

    // Sources do resurrect things -- an issue is un-deleted, a repo restored.
    *keys.lock().unwrap() = tickets(&["F-1", "F-2"]);
    a_moment_passes().await;
    let back = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(back.swept, 0);
    assert!(
        deleted_at(&pool, &format!("{id}:F-2")).await.is_none(),
        "a stale tombstone would hide a live entity"
    );
    assert_eq!(live_count(&pool, &id).await, 2);
}

/// **The per-kind gate (ADR-0003).** Exhaustiveness belongs to a *kind*, not to
/// a source: Gitea enumerates every repository and every branch, and budgets
/// commits and pull requests with `commits_per_repo` / `prs_per_repo`. One
/// per-source flag could only answer "sweep everything" or "sweep nothing", and
/// both are wrong for such a source -- `true` tombstones every commit past the
/// cap on every full sync, `false` leaves a vanished repository live for ever.
///
/// So the run sweeps the exhaustive kinds and leaves the budgeted ones exactly
/// where they were, in the same transaction.
#[tokio::test]
async fn a_run_sweeps_its_exhaustive_kinds_and_spares_its_budgeted_ones() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source_of(
        &id,
        &[("repo", true), ("commit", false)],
        &[
            ("repo", "G-repo-1"),
            ("repo", "G-repo-2"),
            ("commit", "G-sha-1"),
            ("commit", "G-sha-2"),
        ],
    );

    let first = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(first.upserted, 4);
    assert_eq!(first.swept, 0, "nothing is stale on the first full sync");

    // One repository is deleted upstream; one commit merely falls off the
    // per-repository budget. Both simply stop appearing, and the engine has
    // only the declaration to tell the two apart.
    *keys.lock().unwrap() = vec![("repo", "G-repo-1"), ("commit", "G-sha-1")];
    a_moment_passes().await;

    let second = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(second.upserted, 2);
    assert_eq!(
        second.swept, 1,
        "exactly the vanished repo -- not the commit that fell off the budget"
    );
    assert!(
        deleted_at(&pool, &format!("{id}:G-repo-2")).await.is_some(),
        "a repository that vanished from an exhaustive listing is retired"
    );
    assert!(
        deleted_at(&pool, &format!("{id}:G-sha-2")).await.is_none(),
        "a commit past a budget was never claimed to be the whole corpus"
    );
    assert!(deleted_at(&pool, &format!("{id}:G-repo-1")).await.is_none());
    assert!(deleted_at(&pool, &format!("{id}:G-sha-1")).await.is_none());
    assert_eq!(live_count(&pool, &id).await, 3);
}

/// The `upserted > 0` guard -- "a full sync that emitted nothing is
/// indistinguishable from an adapter that silently failed" -- has to hold **per
/// kind** too, for the same reason the gate does.
///
/// A Gitea token that loses repository scope returns an empty listing with a
/// 200 while the branch walk of the repositories already mirrored keeps
/// working. Judged source-wide, that run emitted plenty and would sweep every
/// repo row knobas holds. Judged per kind, the `repo` walk emitted nothing and
/// proves nothing, so nothing of that kind is swept.
#[tokio::test]
async fn a_kind_that_emitted_nothing_is_not_swept_even_when_another_kind_did() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source_of(
        &id,
        &[("repo", true), ("branch", true)],
        &[
            ("repo", "H-repo-1"),
            ("repo", "H-repo-2"),
            ("branch", "H-main"),
        ],
    );

    let first = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(first.upserted, 3);

    // The repository listing comes back empty; the branch walk is unaffected.
    *keys.lock().unwrap() = vec![("branch", "H-main")];
    a_moment_passes().await;
    let second = knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(second.upserted, 1);
    assert_eq!(
        second.swept, 0,
        "a kind that emitted nothing has not proved that its corpus is empty"
    );
    assert!(deleted_at(&pool, &format!("{id}:H-repo-1")).await.is_none());
    assert!(deleted_at(&pool, &format!("{id}:H-repo-2")).await.is_none());
    assert_eq!(live_count(&pool, &id).await, 3);

    // And it does not resolve itself on the next run, or the one after. The
    // guard is stateless -- it asks only what *this* run emitted -- so a kind
    // whose corpus has genuinely gone to zero keeps every row live for as long
    // as it stays empty. That is the documented residual on `run_once`
    // (*Limitations*, case 2).
    //
    // **Read this loop for exactly what it is.** It does not detect anything
    // the assertions above it miss. Because the guard holds no cross-run state
    // -- `emitted` is a fresh set per `PgSink`, per run -- every mutation that
    // would close the gap fires on the *first* empty run, at the `swept == 0`
    // assertion above; this loop then never executes. It is a doc-pin: it
    // costs a couple of seconds to say "indefinitely" out loud, and it would
    // catch a future *design* change that made the guard stateful (say,
    // "sweep after N consecutive empty runs"), which nothing above would. Do
    // not count it as independent evidence that the guard works.
    for round in 3..=4 {
        a_moment_passes().await;
        let again = knobas_sync::run_once(&pool, &src, None).await.unwrap();
        assert_eq!(again.swept, 0, "round {round}");
        assert!(
            deleted_at(&pool, &format!("{id}:H-repo-2")).await.is_none(),
            "round {round}: a repo row of an empty-listing kind is never retired"
        );
        assert_eq!(live_count(&pool, &id).await, 3, "round {round}");
    }
}

/// **A note is not swept, and this test is here to fail loudly if it ever is.**
///
/// The sweep is the one way notes could be destroyed wholesale, and they are
/// the only content knobas holds that no source can hand back. ADR-0003's
/// tombstone trap is the precedent: absence proves deletion only where the run
/// really returns everything, and a kind that *no run emits at all* is absent
/// from every one of them.
///
/// So "notes are not in the sweep's kind list" is not the guarantee. The
/// guarantee is that the sweep cannot reach a note **at all**: its statement is
/// driven from `sync.item`, and migration `0006`'s `item_entity_reserved_chk`
/// forbids any `sync.item` row from naming an entity in the `note:` namespace.
/// What a descriptor declares, what the kind list holds and what an adapter
/// emits are all beside the point.
///
/// Non-vacuous by construction: the same run is asserted to have swept
/// something. A sweep that did not fire would prove nothing about a note that
/// survived it.
#[tokio::test]
async fn a_full_sync_that_sweeps_cannot_reach_a_note() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source(&id, &["A-1", "A-2"]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    // A note about the item that is *about to be* tombstoned, so this covers
    // the case that matters most: the note is not merely elsewhere, it is
    // attached to the very row the sweep is reconciling.
    let doomed = format!("{id}:A-2");
    let note = knobas_core::note::create(
        &pool,
        "Retry runbook",
        &format!("what to do about [[{doomed}]]"),
        "user",
    )
    .await
    .unwrap();
    let note_id = knobas_core::entity::EntityRef::parse(&note.id).unwrap();
    assert_eq!(
        knobas_core::link::entries_of(&pool, &note_id)
            .await
            .unwrap()
            .len(),
        1
    );

    // Upstream hard-deletes A-2.
    *keys.lock().unwrap() = tickets(&["A-1"]);
    a_moment_passes().await;
    let second = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(
        second.swept, 1,
        "the sweep must actually have fired, or this test asserts nothing"
    );
    assert!(deleted_at(&pool, &doomed).await.is_some());

    // The note itself: live address, body intact.
    assert!(
        deleted_at(&pool, &note.id).await.is_none(),
        "a note is not the sweep's to tombstone"
    );
    assert_eq!(
        knobas_core::note::get(&pool, &note_id)
            .await
            .unwrap()
            .map(|row| row.body_md),
        Some(format!("what to do about [[{doomed}]]"))
    );

    // And its `[[ref]]` still resolves -- to a target now marked withdrawn,
    // which is the whole reason the resolution reads `knobas.entity` (§5a).
    let refs = knobas_core::note::refs_of(&pool, &note_id).await.unwrap();
    assert_eq!(refs.len(), 1);
    let target = refs[0]
        .target
        .as_ref()
        .expect("a swept target still resolves");
    assert!(target.deleted_at.is_some(), "and is marked withdrawn");
}

/// A kind knobas owns is not a source's to mirror, whatever its descriptor
/// claims.
///
/// The refusal is the sibling of the namespace guard beside it, and it is a
/// *second* check rather than a widening of that one: the namespace guard can
/// only compare an item against the source id it was handed, so a source
/// legitimately called `shrinking` passes it while emitting `shrinking:x` of
/// kind `note`. Such a row is not a sweep hazard -- `0006` closes that on the
/// id -- but it is a mirrored row sitting in the launcher's note group and in
/// `type:note` that no note command can read, edit or delete.
#[tokio::test]
async fn a_source_may_not_mirror_a_kind_knobas_owns() {
    let pool = pool().await;
    let id = unique();
    let (src, _keys) = source_of(&id, &[("note", true)], &[("note", "N-1")]);

    let refused = knobas_sync::run_once(&pool, &src, None).await.unwrap_err();
    let said = refused.to_string();
    assert!(
        said.contains("knobas owns"),
        "the refusal has to say why: {said}"
    );

    // The run is rolled back whole, so nothing of it reached either table.
    let (rows,): (i64,) = sqlx::query_as("select count(*) from knobas.entity where id = $1")
        .bind(format!("{id}:N-1"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}
