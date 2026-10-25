//! The refresh (ADR-0031, #2100): one monitor, read as the sync reads it.
//!
//! # One document, and what it cannot say
//!
//! There is no read of one monitor on `/metrics`: the refresh reads the
//! whole document, as a run does, and emits the one monitor out of it
//! through the run's own mapping ([`crate::map::to_sync_item`]). A monitor in
//! the document is the item; that half needs nothing else.
//!
//! A monitor **missing** from it is the half that does. Through `/metrics` a
//! paused monitor and a deleted one are one observation ([`crate::cursor`]),
//! and the sync tombstones both, so both are *gone*, which is how the roster
//! draws *Paused*. But the document also omits a monitor Kuma is running and
//! has not checked yet: a monitor publishes no series until its first
//! heartbeat lands. Measured on the pinned 2.5.3 on 2026-10-25: a created
//! monitor is absent when `add` is acknowledged and published 0.27 s later;
//! a resumed one 0.1 s after `resumeMonitor`'s ack. And a refresh follows a
//! landed write at once, so a refresh of what a create or a resume made reads
//! the document in exactly that window. Read as *gone*, the resume would
//! leave the monitor *Paused* until the next run, and the created monitor
//! out of the mirror.
//!
//! So a source with an account, the only one that writes, asks Kuma over
//! socket.io whether a monitor the document omits is still running
//! (`getMonitor`, its `active`). Not there, or not active, is *gone*; active
//! is a monitor whose first heartbeat is on its way, and the document is read
//! again until it is published, within [`FIRST_BEAT`]. A source with only an
//! API key has the document and nothing else, and reads an omitted monitor as
//! *gone*, as its sync does.
//!
//! The document is read first, and a monitor in it is the item whatever the
//! socket would say: the refresh answers what the sync would emit, and a
//! monitor in the document is one the sync emits.

use std::time::Duration;

use knobas_source::{RefreshAnswer, SourceError, SyncItem};

/// How long an active monitor may stay unpublished before the refresh gives
/// up on it: one check of a monitor the create op makes against a host that
/// accepts and never answers (`create`'s sixteen-second timeout; its first
/// beat lands when the check times out), with room for a loaded machine. A
/// monitor not published by then is read by the next run.
///
/// The refresh holds the source's run lock while it waits, so this is also
/// how long it can hold off the run after the write. Against a host that
/// answers, it waits a third of a second or nothing at all.
pub(crate) const FIRST_BEAT: Duration = Duration::from_secs(30);

/// How often the document is read again while an active monitor is waited
/// for.
const REREAD: Duration = Duration::from_millis(250);

/// What the refresh reads a monitor through: the document, and the socket.io
/// channel when the source has one.
#[async_trait::async_trait]
pub(crate) trait Roster: Sync {
    /// The monitor as the document publishes it, mapped as the run maps it,
    /// or `None` when the document omits it.
    async fn published(&self, id: i64) -> Result<Option<SyncItem>, SourceError>;

    /// Whether Kuma is running the monitor: `Some(true)` active,
    /// `Some(false)` paused, not there or not the account's, and `None` for a
    /// source with no channel to ask on.
    async fn active(&self, id: i64) -> Result<Option<bool>, SourceError>;
}

/// Monitor `id`, read through `roster` (the module note).
///
/// # Errors
///
/// The document's and the socket's own, and [`SourceError::Unreachable`] for
/// an active monitor still unpublished after `first_beat`: the read can be
/// asked again, and the next run reads it.
pub(crate) async fn read(
    roster: &dyn Roster,
    id: i64,
    first_beat: Duration,
) -> Result<RefreshAnswer, SourceError> {
    if let Some(item) = roster.published(id).await? {
        return Ok(RefreshAnswer::Item(item));
    }
    if roster.active(id).await? != Some(true) {
        return Ok(RefreshAnswer::Gone);
    }
    let deadline = tokio::time::Instant::now() + first_beat;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(REREAD).await;
        if let Some(item) = roster.published(id).await? {
            return Ok(RefreshAnswer::Item(item));
        }
    }
    Err(SourceError::Unreachable(format!(
        "Uptime Kuma is running monitor {id} but did not publish it in /metrics within \
         {first_beat:?}: its first check has not landed, and the next run reads it"
    )))
}

/// `active` out of `getMonitor`'s answer: `{"ok":true,"monitor":{…,"active":
/// true}}` on the pinned 2.5.3, a boolean. A number is read as one too, the
/// column's own type.
///
/// # Errors
///
/// [`SourceError::Protocol`] for an answer with no `monitor.active`: a Kuma
/// whose record moved, which must not read as a pause.
pub(crate) fn active_of(answered: &serde_json::Value) -> Result<bool, SourceError> {
    let active = &answered["monitor"]["active"];
    active
        .as_bool()
        .or_else(|| active.as_i64().map(|n| n != 0))
        .ok_or_else(|| {
            SourceError::protocol(format!(
                "Uptime Kuma answered getMonitor with no monitor.active: {}",
                answered.to_string().chars().take(120).collect::<String>()
            ))
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A roster that publishes the monitor from its `published_after`-th read
    /// of the document on, and answers `active` as told.
    struct Script {
        published_after: Option<usize>,
        active: Option<bool>,
        reads: Mutex<usize>,
        asked: Mutex<usize>,
    }

    impl Script {
        fn new(published_after: Option<usize>, active: Option<bool>) -> Self {
            Self {
                published_after,
                active,
                reads: Mutex::new(0),
                asked: Mutex::new(0),
            }
        }
    }

    fn monitor(id: i64) -> SyncItem {
        SyncItem {
            entity: knobas_core::entity::EntityRef::new("kuma", &id.to_string()),
            kind: crate::KIND_MONITOR.to_owned(),
            title: "scratch".to_owned(),
            body_text: String::new(),
            author: None,
            updated_at: None,
            payload: serde_json::json!({ "state": "up" }),
            web_url: None,
            deleted: false,
        }
    }

    #[async_trait::async_trait]
    impl Roster for Script {
        async fn published(&self, id: i64) -> Result<Option<SyncItem>, SourceError> {
            let mut reads = self.reads.lock().unwrap();
            *reads += 1;
            Ok(self
                .published_after
                .is_some_and(|after| *reads > after)
                .then(|| monitor(id)))
        }

        async fn active(&self, _id: i64) -> Result<Option<bool>, SourceError> {
            *self.asked.lock().unwrap() += 1;
            Ok(self.active)
        }
    }

    fn item(answer: RefreshAnswer) -> SyncItem {
        match answer {
            RefreshAnswer::Item(item) => item,
            other => panic!("expected the monitor, got {other:?}"),
        }
    }

    /// A published monitor is the item, and the channel is not asked.
    #[tokio::test]
    async fn a_published_monitor_is_the_item_without_asking_the_socket() {
        let roster = Script::new(Some(0), Some(false));
        let read = item(read(&roster, 8, FIRST_BEAT).await.unwrap());
        assert_eq!(read.entity.key, "8");
        assert_eq!(*roster.asked.lock().unwrap(), 0);
    }

    /// Paused, or not there, is gone at once: no wait for a beat that is not
    /// coming.
    #[tokio::test(start_paused = true)]
    async fn an_omitted_monitor_kuma_is_not_running_is_gone_at_once() {
        let started = tokio::time::Instant::now();
        let roster = Script::new(None, Some(false));
        let answer = read(&roster, 8, FIRST_BEAT).await.unwrap();
        assert!(matches!(answer, RefreshAnswer::Gone), "{answer:?}");
        assert_eq!(*roster.reads.lock().unwrap(), 1, "the document read once");
        assert_eq!(started.elapsed(), Duration::ZERO);
    }

    /// A source with no channel has the document alone: gone, as its sync
    /// reads it.
    #[tokio::test(start_paused = true)]
    async fn an_omitted_monitor_is_gone_to_a_source_with_no_channel() {
        let roster = Script::new(Some(2), None);
        let answer = read(&roster, 8, FIRST_BEAT).await.unwrap();
        assert!(matches!(answer, RefreshAnswer::Gone), "{answer:?}");
        assert_eq!(*roster.reads.lock().unwrap(), 1);
    }

    /// **The resume and the create (#2100)**: an active monitor the document
    /// omits is read again until its first heartbeat publishes it.
    #[tokio::test(start_paused = true)]
    async fn an_active_monitor_is_waited_for_until_it_is_published() {
        let roster = Script::new(Some(3), Some(true));
        let read = item(read(&roster, 521, FIRST_BEAT).await.unwrap());
        assert_eq!(read.entity.key, "521");
        assert_eq!(*roster.reads.lock().unwrap(), 4);
    }

    /// An active monitor never published is an error the run after it
    /// outlives, never gone: gone would tombstone a running monitor.
    #[tokio::test(start_paused = true)]
    async fn an_active_monitor_never_published_is_an_error_not_gone() {
        let started = tokio::time::Instant::now();
        let roster = Script::new(None, Some(true));
        let error = read(&roster, 8, FIRST_BEAT).await.expect_err("never published");
        assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
        assert!(started.elapsed() >= FIRST_BEAT);
        assert!(started.elapsed() < FIRST_BEAT + REREAD * 2);
    }

    #[test]
    fn active_is_read_off_the_monitor_record() {
        let answered = |active| serde_json::json!({ "ok": true, "monitor": { "id": 8, "active": active } });
        assert!(active_of(&answered(serde_json::json!(true))).unwrap());
        assert!(!active_of(&answered(serde_json::json!(false))).unwrap());
        assert!(active_of(&answered(serde_json::json!(1))).unwrap());
        assert!(!active_of(&answered(serde_json::json!(0))).unwrap());
        let moved = active_of(&serde_json::json!({ "ok": true, "monitor": { "id": 8 } }));
        assert!(matches!(moved, Err(SourceError::Protocol { .. })), "{moved:?}");
    }
}
