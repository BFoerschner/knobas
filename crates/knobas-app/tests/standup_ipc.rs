//! The standup digest as the interface sees it (issue #288, spec #272 stories
//! 58-63).
//!
//! # Why this seam and not a store battery
//!
//! The digest is a **join**: three producers that reach this database by three
//! unrelated routes (the mirror, the activity stream, knobas' own worklog
//! copy), an identity read, and a descriptor's declared payload paths. Every
//! one of those can be green on its own while the digest is wrong -- and the
//! two ways it can be wrong are the two the spec names in as many words: a
//! *yesterday* that is not the newest day with activity, and a blockers list
//! that reads a word instead of a declaration.
//!
//! Spec #272's Testing Decisions name this seam for exactly this battery: *"the
//! digest's yesterday rule at a Monday, its seven-day cap, and blockers from
//! both the declared set and links"*, at the command seam, against a scratch
//! database and a trait-level mock source.
//!
//! # The one thing the fixtures are arranged around
//!
//! **The declared blocked-like set here is `Waiting for support`, and nothing
//! spells `Blocked`.** A blockers read that carried a list of English status
//! words -- the failure #277 exists to end -- would pass a battery whose
//! fixtures say `Blocked`, because that is the word such a list would contain.
//! So the source that has a blocked ticket declares a set no hardcoded list
//! would guess, and a *second* ticket sits in the status `Blocked`, which this
//! source declares nothing about and which therefore must **not** be a
//! blocker. The two together are what make the assertion about the
//! declaration rather than about the word.
//!
//! Every test gets a database of its own, for `inbox_ipc.rs`'s reason: the
//! read is a pass over the whole mirror, so a shared database would put every
//! other test's fixtures into this one's digest.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use knobas_app::commands::entity::standup_digest_inner;
use knobas_app::standup::{DigestLine, StandupDigest};
use knobas_app::time::week::DayWindow;
use knobas_source::instance::SourceInstance;
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, KindInfo, KindPaths, PayloadPath, Sink, Source,
    SourceDescriptor, SourceError,
};
use knobas_sync::config::{self, AuthKind, InsertConfig};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

/// The configured username -- who the digest is about.
const ME: &str = "mara.lindqvist";
/// A colleague, configured nowhere. Story 63's negative control.
const THEM: &str = "jonas.becker";

/// The tracker source id, and the entity namespace its items are in.
const TRACKER: &str = "tracker";
/// The forge source id.
const FORGE: &str = "forge";

/// The blocked-like status this fixture's tracker declares.
///
/// Deliberately not a word an English-speaking implementer would put in a
/// hardcoded list. See the module header.
const DECLARED_BLOCKED: &str = "Waiting for support";

/// A status the fixture's tracker declares **nothing** about, and which a
/// hardcoded list would almost certainly contain.
const UNDECLARED_BLOCKED: &str = "Blocked";

/// The digest is read for this day; every window below is relative to it.
fn today() -> NaiveDate {
    // A Monday. Story 60's whole point is that this day reads Friday.
    NaiveDate::from_ymd_opt(2026, 8, 31).expect("2026-08-31 is a Monday")
}

/// Midday on the day being asked about -- the `now` every test passes unless
/// it is testing what `now` decides.
fn noon() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap()
}

/// The window one date spans, in UTC -- this battery's reader lives at UTC, so
/// the webview's arithmetic is a `NaiveDate` and two midnights.
///
/// The real one is `app/src/lib/time/day.ts`'s `dayBounds`, which asks `Date`
/// for the local midnights; the shape it produces is this one.
fn window(day: NaiveDate) -> DayWindow {
    DayWindow {
        day,
        from: day.and_hms_opt(0, 0, 0).expect("midnight").and_utc(),
        to: day
            .succ_opt()
            .expect("the next day")
            .and_hms_opt(0, 0, 0)
            .expect("midnight")
            .and_utc(),
    }
}

/// The `count` days before `today()`, oldest first -- what the webview sends
/// as `earlier`.
fn earlier(count: i64) -> Vec<DayWindow> {
    (1..=count)
        .rev()
        .map(|back| window(today() - Duration::days(back)))
        .collect()
}

/// An instant on a day some number of days before the digest's own.
fn days_before(back: i64, hour: u32, minute: u32) -> DateTime<Utc> {
    (today() - Duration::days(back))
        .and_hms_opt(hour, minute, 0)
        .expect("a time of day")
        .and_utc()
}

// -- the sources this battery configures ------------------------------------

/// A `Source` that syncs nothing: the digest reads the mirror, never a source.
struct Inert(SourceDescriptor);

#[async_trait]
impl Source for Inert {
    fn descriptor(&self) -> SourceDescriptor {
        self.0.clone()
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo {
            account: None,
            server_version: None,
            secret_expires_at: None,
            detail: None,
            discovered: std::collections::BTreeMap::new(),
        })
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        _sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }

    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        // Nothing in this battery sends a write: the digest reads the queue's
        // *record* of one, which the fixtures write directly.
        Ok(knobas_source::WriteReceipt::none())
    }
}

fn kind(id: &str) -> KindInfo {
    KindInfo {
        id: id.to_owned(),
        label: id.to_owned(),
        plural: id.to_owned(),
        monogram: "T".to_owned(),
        full_sync_exhaustive: true,
    }
}

/// The tracker's descriptor: a ticket's status and assignee at declared paths,
/// and one declared blocked-like name.
fn tracker_descriptor() -> SourceDescriptor {
    SourceDescriptor {
        id: TRACKER.to_owned(),
        adapter_kind: TRACKER.to_owned(),
        name: TRACKER.to_owned(),
        capabilities: Vec::new(),
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        write_ops: vec!["comment".to_owned(), "transition".to_owned()],
        entity_kinds: vec![kind("ticket")],
        config_schema: serde_json::json!({"type": "object", "properties": {}}),
        payload_paths: vec![KindPaths {
            kind: "ticket".to_owned(),
            status_name: vec![PayloadPath::of(["fields", "status", "name"])],
            assignee: vec![PayloadPath::of(["fields", "assignee", "name"])],
            blocked_statuses: vec![DECLARED_BLOCKED.to_owned()],
            ..KindPaths::default()
        }],
    }
}

/// The forge's descriptor: commits, and **no declaration at all**.
///
/// A source with nothing to say about status or assignee contributes no
/// blocker, which is ADR-0007's miss. It is here so that "the tracker's
/// declaration is what produced the blockers" is a statement about one source
/// out of two rather than about the only source there is.
fn forge_descriptor() -> SourceDescriptor {
    SourceDescriptor {
        id: FORGE.to_owned(),
        adapter_kind: FORGE.to_owned(),
        name: FORGE.to_owned(),
        capabilities: Vec::new(),
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        write_ops: Vec::new(),
        entity_kinds: vec![kind("commit")],
        config_schema: serde_json::json!({"type": "object", "properties": {}}),
        payload_paths: Vec::new(),
    }
}

struct Registry;

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![tracker_descriptor(), forge_descriptor()]
    }

    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        let descriptor = if instance.id == FORGE {
            forge_descriptor()
        } else {
            tracker_descriptor()
        };
        Ok(Box::new(Inert(descriptor)))
    }
}

// -- the harness ------------------------------------------------------------

struct Harness {
    pool: PgPool,
    registry: Arc<Registry>,
}

impl Harness {
    async fn digest(&self) -> StandupDigest {
        self.digest_at(noon(), &earlier(7)).await
    }

    async fn digest_at(&self, now: DateTime<Utc>, earlier: &[DayWindow]) -> StandupDigest {
        standup_digest_inner(
            &self.pool,
            self.registry.as_ref(),
            now,
            window(today()),
            earlier,
        )
        .await
        .expect("the digest reads")
    }

    /// One mirrored item, authored by `author` and last moved at `at`.
    async fn item(
        &self,
        source: &str,
        kind: &str,
        key: &str,
        author: &str,
        at: DateTime<Utc>,
        payload: serde_json::Value,
    ) -> String {
        let id = format!("{source}:{key}");
        self.entity(&id, kind, key).await;
        sqlx::query(
            "insert into sync.item
                 (entity_id, source_id, kind, title, body_text, author, item_updated_at, payload)
             values ($1,$2,$3,$4,'',$5,$6,$7)",
        )
        .bind(&id)
        .bind(source)
        .bind(kind)
        .bind(key)
        .bind(author)
        .bind(at)
        .bind(payload)
        .execute(&self.pool)
        .await
        .expect("the mirror row is written");
        id
    }

    async fn entity(&self, id: &str, kind: &str, title: &str) {
        sqlx::query(
            "insert into knobas.entity (id, kind, title) values ($1,$2,$3)
             on conflict (id) do nothing",
        )
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(&self.pool)
        .await
        .expect("the entity row is written");
    }

    /// A ticket of `assignee`'s, standing in `status`.
    async fn ticket(&self, key: &str, assignee: &str, status: &str) -> String {
        self.ticket_at(key, assignee, status, days_before(4, 9, 0))
            .await
    }

    /// The same, with the instant the mirror says it last moved chosen -- what
    /// the blockers list is ordered by, and the only way to tell a merged
    /// list from two concatenated ones.
    async fn ticket_at(
        &self,
        key: &str,
        assignee: &str,
        status: &str,
        moved: DateTime<Utc>,
    ) -> String {
        self.item(
            TRACKER,
            "ticket",
            key,
            "somebody.else",
            moved,
            serde_json::json!({
                "fields": {
                    "status": { "name": status },
                    "assignee": { "name": assignee },
                }
            }),
        )
        .await
    }

    /// The activity line the write queue writes when a person queues a write.
    async fn queued(&self, entity_id: &str, op: &str, at: DateTime<Utc>) {
        self.activity(
            "user",
            "queued",
            entity_id,
            at,
            serde_json::json!({ "op": op, "source_id": TRACKER, "write_id": 7 }),
        )
        .await;
    }

    /// One activity line, stamped at a chosen instant.
    ///
    /// `activity::record` stamps `now()`, which no test of a *day* can use, so
    /// the line is written and then dated -- the shape `time_ipc.rs` records
    /// for the same problem.
    async fn activity(
        &self,
        actor: &str,
        verb: &str,
        entity_id: &str,
        at: DateTime<Utc>,
        detail: serde_json::Value,
    ) {
        sqlx::query(
            "insert into knobas.activity (at, actor, verb, entity_id, detail)
             values ($1,$2,$3,$4,$5)",
        )
        .bind(at)
        .bind(actor)
        .bind(verb)
        .bind(entity_id)
        .bind(detail)
        .execute(&self.pool)
        .await
        .expect("the activity line is written");
    }

    /// One local worklog copy, for work that began at `at`.
    async fn worklog(&self, entity_id: &str, at: DateTime<Utc>) {
        sqlx::query(
            "insert into knobas.worklog (entity_id, started_at, seconds, comment, block_ids)
             values ($1,$2,5400,'','{}')",
        )
        .bind(entity_id)
        .bind(at)
        .execute(&self.pool)
        .await
        .expect("the worklog copy is written");
    }

    /// A confirmed link saying `blocker` blocks `blocked`.
    async fn blocks(&self, blocker: &str, blocked: &str) {
        knobas_core::link::create(
            &self.pool,
            &knobas_core::entity::EntityRef::parse(blocker).expect("an entity id"),
            &knobas_core::entity::EntityRef::parse(blocked).expect("an entity id"),
            "blocks",
            knobas_core::link::Origin::Manual,
            None,
            "user",
        )
        .await
        .expect("the link is drawn");
        sqlx::query("update knobas.link set confirmed_at = now() where confirmed_at is null")
            .execute(&self.pool)
            .await
            .expect("the link is confirmed");
    }

    /// The running timer, on an entity or on an ad-hoc label.
    async fn timer(&self, entity_id: Option<&str>, label: Option<&str>, started: DateTime<Utc>) {
        sqlx::query(
            "insert into knobas.timer (entity_id, label, started_at, last_heartbeat)
             values ($1,$2,$3,$3)",
        )
        .bind(entity_id)
        .bind(label)
        .bind(started)
        .execute(&self.pool)
        .await
        .expect("the timer row is written");
    }
}

async fn harness(name: &str) -> Harness {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector.pool(4).await.expect("a pool onto the scratch db");
    for (id, kind) in [(TRACKER, TRACKER), (FORGE, FORGE)] {
        config::insert(
            &pool,
            &InsertConfig {
                id: id.to_owned(),
                adapter_kind: kind.to_owned(),
                display_name: id.to_owned(),
                base_url: String::new(),
                auth_kind: AuthKind::Method(AuthMethod::Pat),
                config: serde_json::json!({ "username": ME }),
                sync_interval_secs: 300,
                enabled: true,
            },
        )
        .await
        .expect("the source row is written");
    }
    Harness {
        pool,
        registry: Arc::new(Registry),
    }
}

/// The entity ids one list names, in order.
fn refs(lines: &[DigestLine]) -> Vec<Option<&str>> {
    lines.iter().map(|l| l.entity_id.as_deref()).collect()
}

/// `(entity id, source, verb)` for one list -- what "every line names its
/// item, which source and which verb" is, as data.
fn traces(lines: &[DigestLine]) -> Vec<(Option<&str>, &str, &str)> {
    lines
        .iter()
        .map(|l| (l.entity_id.as_deref(), l.source.as_str(), l.verb.as_str()))
        .collect()
}

// -- yesterday --------------------------------------------------------------

/// **Monday reads Friday** (story 60).
///
/// The digest is asked for a Monday. Friday has one of the user's commits and
/// the weekend has nothing, so *yesterday* is Friday and says so -- and the
/// heading has a date to draw, which is what `yesterday_day` is for.
///
/// The Thursday line is the control that matters: without it the test could
/// not tell "the newest day with activity" from "the oldest day the window
/// reaches", and both would answer Friday on a fixture with one day in it.
#[tokio::test]
async fn a_monday_digest_reads_fridays_work_across_the_weekend() {
    let h = harness("standup-monday").await;
    let friday = h
        .item(
            FORGE,
            "commit",
            "9f21ac",
            ME,
            days_before(3, 17, 30),
            serde_json::json!({}),
        )
        .await;
    let thursday = h
        .item(
            FORGE,
            "commit",
            "1b0e77",
            ME,
            days_before(4, 11, 0),
            serde_json::json!({}),
        )
        .await;

    let digest = h.digest().await;

    assert_eq!(
        digest.yesterday_day,
        Some(today() - Duration::days(3)),
        "the Friday before a Monday is what yesterday means"
    );
    assert_eq!(refs(&digest.yesterday), vec![Some(friday.as_str())]);
    assert!(
        !refs(&digest.yesterday).contains(&Some(thursday.as_str())),
        "yesterday is one day, not everything since the last one"
    );
}

/// **A week of silence finds nothing, and the cap is seven days** (story 60).
///
/// Two assertions in one fixture, because they are the same number seen from
/// its two sides. The user's only activity is **eight** days back:
///
/// * the digest finds no yesterday at all -- an empty list and a `None` date,
///   which is a week off and not an error;
/// * and moving that same activity to **seven** days back finds it, so the
///   cap is not merely "some number smaller than eight".
///
/// A test with only the first half passes just as well against a cap of one.
#[tokio::test]
async fn a_week_of_silence_has_no_yesterday_and_the_seventh_day_is_still_in_reach() {
    let h = harness("standup-silence").await;
    h.item(
        FORGE,
        "commit",
        "8daysago",
        ME,
        days_before(8, 10, 0),
        serde_json::json!({}),
    )
    .await;

    let digest = h.digest_at(noon(), &earlier(8)).await;
    assert_eq!(
        digest.yesterday_day, None,
        "eight days back is outside the seven-day cap, however many windows \
         the caller sent"
    );
    assert!(digest.yesterday.is_empty());

    // The same activity one day nearer is inside it.
    let seventh = h
        .item(
            FORGE,
            "commit",
            "7daysago",
            ME,
            days_before(7, 10, 0),
            serde_json::json!({}),
        )
        .await;
    let digest = h.digest_at(noon(), &earlier(8)).await;
    assert_eq!(digest.yesterday_day, Some(today() - Duration::days(7)));
    assert_eq!(refs(&digest.yesterday), vec![Some(seventh.as_str())]);
}

/// The three producers each put a line on the list, and each line says which
/// source and which verb it came from (stories 58 and 59).
///
/// One day, three facts about it: a pull request the user authored, a comment
/// they queued through knobas, and an afternoon they logged. A digest that
/// read only the mirror would show one of the three and look perfectly
/// healthy.
#[tokio::test]
async fn yesterdays_line_per_producer_names_its_item_its_source_and_its_verb() {
    let h = harness("standup-producers").await;
    let pr = h
        .item(
            FORGE,
            "commit",
            "c0ffee",
            ME,
            days_before(1, 9, 0),
            serde_json::json!({}),
        )
        .await;
    let commented = h.ticket("PAY-231", ME, "In Progress").await;
    h.queued(&commented, "comment", days_before(1, 11, 0)).await;
    let logged = h.ticket("PAY-240", ME, "In Progress").await;
    h.worklog(&logged, days_before(1, 14, 0)).await;

    let digest = h.digest().await;

    assert_eq!(
        traces(&digest.yesterday),
        vec![
            (Some(logged.as_str()), TRACKER, "log_work"),
            (Some(commented.as_str()), TRACKER, "comment"),
            (Some(pr.as_str()), FORGE, "attributed"),
        ],
        "newest first, one line per producer, each naming its source and verb"
    );
    for line in &digest.yesterday {
        assert!(
            line.reason.contains(&line.source),
            "a reason names the source it came from: {:?}",
            line.reason
        );
    }
}

/// A logged afternoon is **one** line, not two (the `log_work` skip).
///
/// A worklog travels through the write queue like every other write, so it
/// leaves a `queued` activity line *as well as* the local copy. Reading both
/// halves would put every logged afternoon on the standup twice.
#[tokio::test]
async fn a_logged_afternoon_is_one_line_and_not_the_queue_line_as_well() {
    let h = harness("standup-worklog-once").await;
    let ticket = h.ticket("PAY-231", ME, "In Progress").await;
    h.worklog(&ticket, days_before(1, 14, 0)).await;
    h.queued(&ticket, "log_work", days_before(1, 17, 0)).await;

    let digest = h.digest().await;

    assert_eq!(
        traces(&digest.yesterday),
        vec![(Some(ticket.as_str()), TRACKER, "log_work")],
        "the local copy is the worklog's one line; its queue line is the same \
         fact a second time"
    );
}

/// The queue narrates its own progress; the digest listens once (story 59).
///
/// One comment produces `queued`, then `sent`. Both are the user's actor and
/// both name the ticket, so a read that took every line would draw the comment
/// twice -- and three times for a write that was held first.
#[tokio::test]
async fn a_write_is_one_line_however_many_states_the_queue_narrates() {
    let h = harness("standup-queue-states").await;
    let ticket = h.ticket("PAY-231", ME, "In Progress").await;
    let detail = serde_json::json!({ "op": "comment", "source_id": TRACKER, "write_id": 7 });
    for verb in ["queued", "held", "sent"] {
        h.activity("user", verb, &ticket, days_before(1, 11, 0), detail.clone())
            .await;
    }

    let digest = h.digest().await;

    assert_eq!(traces(&digest.yesterday).len(), 1, "{:?}", digest.yesterday);
    assert_eq!(digest.yesterday[0].verb, "comment");
}

// -- today ------------------------------------------------------------------

/// **Today starts at the day's own midnight** (story 61's window).
///
/// The unwitnessed direction, and the one a `>=` written as a `>` or a window
/// widened by an hour would break silently: an item touched at 23:59 the night
/// before belongs to yesterday's list and to nothing else, and one touched at
/// 00:00 belongs to today's.
#[tokio::test]
async fn today_begins_at_midnight_and_last_nights_last_minute_is_not_on_it() {
    let h = harness("standup-midnight").await;
    let last_night = h
        .item(
            FORGE,
            "commit",
            "2359",
            ME,
            days_before(1, 23, 59),
            serde_json::json!({}),
        )
        .await;
    let this_morning = h
        .item(
            FORGE,
            "commit",
            "0000",
            ME,
            window(today()).from,
            serde_json::json!({}),
        )
        .await;

    let digest = h.digest().await;

    assert_eq!(
        refs(&digest.today),
        vec![Some(this_morning.as_str())],
        "the day's first instant is on today's list and last night's last \
         minute is not"
    );
    assert_eq!(
        refs(&digest.yesterday),
        vec![Some(last_night.as_str())],
        "and last night's last minute is on yesterday's"
    );
}

/// **Today includes what the timer is on right now** (story 61).
#[tokio::test]
async fn the_running_timers_target_is_on_todays_list() {
    let h = harness("standup-timer").await;
    let ticket = h.ticket("PAY-231", ME, "In Progress").await;
    h.timer(Some(&ticket), None, noon() - Duration::hours(1))
        .await;

    let digest = h.digest().await;

    assert_eq!(
        traces(&digest.today),
        vec![(Some(ticket.as_str()), TRACKER, "timer")]
    );
}

/// A timer on an **ad-hoc label** is still on the list, and it is the one line
/// with nowhere to click.
///
/// `CONTEXT.md`'s timer target is an entity *or* a label, and the digest has
/// to be able to say "the clock is on DB config for the migration". A read
/// that dropped it -- because every other line has an entity -- would leave
/// the reader's own afternoon out of their standup.
#[tokio::test]
async fn a_timer_on_an_ad_hoc_label_is_a_line_with_no_item_behind_it() {
    let h = harness("standup-timer-label").await;
    h.timer(
        None,
        Some("DB config for the migration"),
        noon() - Duration::hours(1),
    )
    .await;

    let digest = h.digest().await;

    assert_eq!(refs(&digest.today), vec![None]);
    assert_eq!(digest.today[0].title, "DB config for the migration");
    assert_eq!(digest.today[0].verb, "timer");
}

/// A digest read for a **past** day does not claim this afternoon's timer.
///
/// The rule is `now`, not "there is a timer": the view can be opened on any
/// date, and a timer running while somebody reads back last Tuesday is not
/// something that happened on last Tuesday.
#[tokio::test]
async fn a_timer_running_outside_the_day_being_asked_about_is_not_on_its_list() {
    let h = harness("standup-timer-elsewhere").await;
    let ticket = h.ticket("PAY-231", ME, "In Progress").await;
    h.timer(Some(&ticket), None, noon()).await;

    let digest = h.digest_at(noon() + Duration::days(2), &earlier(7)).await;

    assert!(
        digest.today.is_empty(),
        "the reader is two days past the day they asked about: {:?}",
        digest.today
    );
}

// -- blockers ---------------------------------------------------------------

/// **Both halves, and nothing else** (story 62, criterion 2).
///
/// Four of the user's tickets:
///
/// * one in the status its own source **declares** blocked-like, which is on
///   the list because of the declaration;
/// * one a confirmed link marks **blocked by** another, which is on the list
///   because of the link;
/// * one in `Blocked` -- a status this source declares nothing about, and the
///   word a hardcoded list would carry -- which must be **absent**;
/// * one in `In Progress` with no link, which must be absent.
///
/// The third is the assertion that distinguishes a declared read from a word
/// list. The reasons are checked as well as the membership: a line whose
/// provenance cannot be shown is not shippable, and here the two halves are
/// the two things a reader has to be able to tell apart.
///
/// **The order is asserted, not sorted away.** The two halves are two reads,
/// each already ordered by `at`, and concatenating them gives two descending
/// runs rather than one list -- which is what `DigestLine::at` promises on
/// both sides of the wire and what the view's *ago* column is drawn from. The
/// link blocker is given the newer instant, so it has to come **first**, which
/// is the opposite of the order the two reads happen in.
#[tokio::test]
async fn blockers_come_from_the_declared_set_and_from_the_link_graph_and_nowhere_else() {
    let h = harness("standup-blockers").await;
    let by_status = h
        .ticket_at("PAY-231", ME, DECLARED_BLOCKED, days_before(4, 9, 0))
        .await;
    let by_link = h
        .ticket_at("PAY-240", ME, "In Progress", days_before(2, 9, 0))
        .await;
    let undeclared = h.ticket("PAY-250", ME, UNDECLARED_BLOCKED).await;
    let neither = h.ticket("PAY-260", ME, "In Progress").await;
    let blocker = h.ticket("PAY-9", THEM, "In Progress").await;
    h.blocks(&blocker, &by_link).await;

    let digest = h.digest().await;

    let listed: Vec<(Option<&str>, &str)> = digest
        .blockers
        .iter()
        .map(|l| (l.entity_id.as_deref(), l.verb.as_str()))
        .collect();
    assert_eq!(
        listed,
        vec![
            (Some(by_link.as_str()), "blocked_by"),
            (Some(by_status.as_str()), "blocked_status"),
        ],
        "the declared status and the link, newest first across both halves, \
         and neither {undeclared} -- whose status this source declares nothing \
         about -- nor {neither}"
    );

    let status_line = digest
        .blockers
        .iter()
        .find(|l| l.verb == "blocked_status")
        .expect("the declared-status blocker");
    assert!(
        status_line.reason.contains(DECLARED_BLOCKED),
        "the reason names the status its source called blocked-like: {:?}",
        status_line.reason
    );
    let link_line = digest
        .blockers
        .iter()
        .find(|l| l.verb == "blocked_by")
        .expect("the link blocker");
    assert!(
        link_line.reason.contains("PAY-9"),
        "the reason names what is in the way: {:?}",
        link_line.reason
    );
}

/// The **direction** of a `blocks` link is load-bearing.
///
/// `A blocks B` is one row read from two ends. The blocked ticket is `B`, and
/// a read that joined `from_id` instead would list the tickets that are in
/// somebody else's way -- which is a standup reporting the opposite of the
/// truth on every row.
#[tokio::test]
async fn the_blocking_end_of_a_link_is_not_itself_a_blocker() {
    let h = harness("standup-link-direction").await;
    let blocker = h.ticket("PAY-9", ME, "In Progress").await;
    let blocked = h.ticket("PAY-231", ME, "In Progress").await;
    h.blocks(&blocker, &blocked).await;

    let digest = h.digest().await;

    assert_eq!(
        refs(&digest.blockers),
        vec![Some(blocked.as_str())],
        "both tickets are the user's; only the one at the `to` end is stuck"
    );
}

/// An **unconfirmed** link is a suggestion, and a suggestion is not a blocker.
///
/// `knobas.link` holds both populations; `knobas.confirmed_link` holds one of
/// them. A standup that listed a proposal knobas made and nobody accepted
/// would be reporting a guess as a fact.
#[tokio::test]
async fn a_link_nobody_confirmed_is_not_a_blocker() {
    let h = harness("standup-unconfirmed").await;
    let blocker = h.ticket("PAY-9", THEM, "In Progress").await;
    let blocked = h.ticket("PAY-231", ME, "In Progress").await;
    // Written as the tray's own row rather than through `link::create`, whose
    // `confirmed_at` defaults to `now()`: a proposal is the row with that
    // column null, and `link_proposal_chk` is what makes the rule, the class
    // and the reason mandatory on one.
    sqlx::query(
        "insert into knobas.link
             (from_id, to_id, relation, origin, created_by,
              confirmed_at, rule, rule_class, reason)
         values ($1,$2,'blocks','suggested','knobas',
                 null,'branch-name','exact_key','the branch names PAY-231')",
    )
    .bind(&blocker)
    .bind(&blocked)
    .execute(&h.pool)
    .await
    .expect("the proposal is written");

    let digest = h.digest().await;

    assert!(
        digest.blockers.is_empty(),
        "an unconfirmed row is a proposal: {:?}",
        digest.blockers
    );
}

// -- mine only --------------------------------------------------------------

/// **A colleague is in no list** (story 63).
///
/// The one negative control the whole feature rests on, and it has to be
/// asked of every list at once, because the three are three different reads
/// with three different ideas of "mine": the mirror's `author`, the activity
/// stream's `actor`, and the declared assignee path.
///
/// So the colleague gets one of each: a commit in yesterday's window, one this
/// morning, and a ticket of theirs standing in the declared blocked-like
/// status. Every one of them would be on a list if its read had forgotten who
/// the digest is about, and the user's own line on each list is what shows the
/// read was working at all.
#[tokio::test]
async fn a_colleagues_day_is_in_no_list() {
    let h = harness("standup-colleague").await;
    let mine_yesterday = h
        .item(
            FORGE,
            "commit",
            "mine-y",
            ME,
            days_before(1, 9, 0),
            serde_json::json!({}),
        )
        .await;
    let mine_today = h
        .item(
            FORGE,
            "commit",
            "mine-t",
            ME,
            noon() - Duration::hours(2),
            serde_json::json!({}),
        )
        .await;
    let theirs_yesterday = h
        .item(
            FORGE,
            "commit",
            "theirs-y",
            THEM,
            days_before(1, 9, 30),
            serde_json::json!({}),
        )
        .await;
    let theirs_today = h
        .item(
            FORGE,
            "commit",
            "theirs-t",
            THEM,
            noon() - Duration::hours(1),
            serde_json::json!({}),
        )
        .await;
    let theirs_blocked = h.ticket("PAY-999", THEM, DECLARED_BLOCKED).await;
    let mine_blocked = h.ticket("PAY-231", ME, DECLARED_BLOCKED).await;

    let digest = h.digest().await;

    assert_eq!(refs(&digest.yesterday), vec![Some(mine_yesterday.as_str())]);
    assert_eq!(refs(&digest.today), vec![Some(mine_today.as_str())]);
    assert_eq!(refs(&digest.blockers), vec![Some(mine_blocked.as_str())]);
    for theirs in [&theirs_yesterday, &theirs_today, &theirs_blocked] {
        for list in [&digest.yesterday, &digest.today, &digest.blockers] {
            assert!(
                !refs(list).contains(&Some(theirs.as_str())),
                "{theirs} is a colleague's and the digest describes me only"
            );
        }
    }
}
