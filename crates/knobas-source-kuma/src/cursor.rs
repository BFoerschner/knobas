//! What one run remembers for the next: a digest of every monitor it emitted.
//!
//! # Why a Kuma sync needs a cursor at all
//!
//! `/metrics` is a snapshot with no clock in it -- no timestamps, no
//! pagination, no "changed since". So there is nothing to resume *from*, and
//! spec #427 says as much: every run reads the whole document. What the SPI
//! still requires is that **an incremental sync over an unchanged source emits
//! nothing and hands its cursor straight back** (`knobas_source::contract`
//! clause 2), because the engine reads "same cursor, no items" as "nothing
//! happened" and writes no activity line for it. An adapter that re-emitted its
//! whole corpus on every poll would turn a one-minute schedule into 1,440
//! "synced everything" lines a day.
//!
//! So the cursor is not a position: it is **the previous corpus, digested**.
//! A run reads `/metrics`, folds it into monitors, and compares. Same corpus,
//! nothing emitted, same cursor back. Anything different -- one monitor's
//! response time, a rename, an addition, a removal -- and the run emits **every
//! monitor**, which is what "every run is full" means here and what keeps the
//! kind honestly `full_sync_exhaustive`.
//!
//! **How often that is actually quiet: not often.** The digest covers the
//! response time, and Kuma writes a new one on every heartbeat, so against a
//! live instance polled every minute most runs do find a difference and do
//! re-emit the roster. Spec #427 says so in as many words -- "Response time
//! changes every poll, so nearly every monitor upserts every run; *upserted*
//! stays honest and small only because the corpus is small". This is therefore
//! not a saving to lean on: what it is for is the contract's clause 2, which an
//! adapter must satisfy whatever its server happens to be doing, and the
//! genuinely quiet case (a Kuma whose monitors are all paused, or one polled
//! faster than it beats).
//!
//! # Why it also carries the names, and why a vanished monitor is tombstoned
//!
//! A monitor deleted in Kuma must leave the mirror on the next run (spec #427,
//! story 54: *the roster never shows ghosts*). The engine's own sweep cannot do
//! it: the sweep is gated on a **cursor-less** run (`knobas_sync`, limitation
//! 3), and a scheduled poll always resumes from the stored position, so a
//! deleted monitor would sit in the mirror until somebody cleared the cursor.
//! The adapter therefore reports the deletion itself, as `SyncItem::deleted` --
//! which needs the monitor's *name*, since a tombstone still has to render.
//! That is what the stored names are for, and they are the only reason this is
//! a map of entries rather than one hash of everything.
//!
//! Both mechanisms stay in place and agree: a cursor-less run emits the whole
//! corpus, so the engine's sweep retires whatever is missing, and every other
//! run tombstones it here.
//!
//! **A paused monitor is tombstoned too, and that is not a bug in this
//! module.** Measured on the pinned image: pausing a monitor removes every one
//! of its series from `/metrics`, and resuming puts them back. Through this
//! channel *paused* and *deleted* are one observation, so the mirror loses a
//! paused monitor and regains it on resume. Telling them apart needs the
//! socket.io channel, which is the write half's ticket and not this one's.

use std::collections::BTreeMap;

/// The version this module writes, and the only one it reads.
///
/// A cursor from a future knobas is not decoded and not guessed at: it falls
/// through to [`Position::of`]'s unreadable arm, which costs one full emission
/// and nothing else.
const VERSION: u8 = 1;

/// What the last run saw, per monitor.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Entry {
    /// The monitor's name as it was then -- the title a tombstone renders with
    /// when the monitor itself is gone.
    pub(crate) name: String,
    /// [`digest`] of the payload this adapter emitted for it.
    pub(crate) digest: String,
}

/// The whole of what one run tells the next.
///
/// A `BTreeMap` and a fixed field order, so that two runs over an unchanged
/// Kuma serialize to the **same bytes**: the engine compares cursors for
/// equality, and a map that serialized in hash order would report a change on
/// every poll while nothing had changed at all.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Position {
    pub(crate) version: u8,
    pub(crate) monitors: BTreeMap<String, Entry>,
}

impl Position {
    /// The position a corpus of `(id, name, payload)` triples leaves behind.
    pub(crate) fn of<'a>(
        corpus: impl IntoIterator<Item = (&'a str, &'a str, &'a serde_json::Value)>,
    ) -> Self {
        Self {
            version: VERSION,
            monitors: corpus
                .into_iter()
                .map(|(id, name, payload)| {
                    (
                        id.to_owned(),
                        Entry {
                            name: name.to_owned(),
                            digest: digest(payload),
                        },
                    )
                })
                .collect(),
        }
    }

    /// The stored cursor, or `None` for one this version cannot read.
    ///
    /// Unreadable is not an error: a cursor written by another version, or by
    /// something that is not this adapter at all, means only that this run has
    /// no idea what the last one saw. It emits everything and stores a cursor
    /// it *can* read.
    ///
    /// **What that costs is one poll for the upserts and, for deletions, more
    /// than that.** A run with no readable previous corpus has nothing to
    /// compare against, so it emits no tombstone -- and it is not a cursor-less
    /// run either, so the engine does not sweep for it. A monitor deleted in
    /// the window between the last readable cursor and this run is therefore
    /// **not retired at all**, and stays in the mirror until something runs
    /// that source cursor-less (a first sync, a backfill, or a cleared cursor).
    /// The window is one run per cursor-format change, and there is no cheaper
    /// honest answer: an adapter that tombstoned on an unreadable cursor would
    /// be tombstoning the whole mirror on the strength of not knowing anything.
    /// Recorded because "self-healing" is what this looked like before the
    /// deletion half was thought through.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let parsed: Self = serde_json::from_str(raw).ok()?;
        (parsed.version == VERSION).then_some(parsed)
    }

    /// This position as the cursor to store.
    pub(crate) fn encode(&self) -> String {
        serde_json::to_string(self).expect("a position is plain data and always serializes")
    }

    /// The monitors this position knows that `current` no longer holds, with
    /// the names they had -- the tombstones the next sync owes the mirror.
    pub(crate) fn vanished<'a>(
        &'a self,
        current: &'a Self,
    ) -> impl Iterator<Item = (&'a str, &'a str)> {
        self.monitors
            .iter()
            .filter(|(id, _)| !current.monitors.contains_key(*id))
            .map(|(id, entry)| (id.as_str(), entry.name.as_str()))
    }
}

/// FNV-1a over the payload's canonical JSON.
///
/// Hand-rolled and dependency-free on purpose. `std`'s `DefaultHasher` is
/// documented as unstable across Rust releases, which would silently re-emit
/// every monitor once per toolchain upgrade; a cryptographic hash would be a
/// dependency for a job with no adversary. FNV-1a is fixed by its constants,
/// so a cursor written today reads the same next year.
///
/// The input is the payload, which is everything the mirror stores about a
/// monitor and everything the title, the body text and the web URL are derived
/// from -- so a change this misses is a change the mirror would not have shown.
fn digest(payload: &serde_json::Value) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let canonical =
        serde_json::to_string(payload).expect("a monitor payload is plain data and serializes");
    let mut hash = OFFSET;
    for byte in canonical.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn position(rows: &[(&str, &str, serde_json::Value)]) -> Position {
        Position::of(rows.iter().map(|(id, name, payload)| (*id, *name, payload)))
    }

    /// The property the whole design rests on: two runs over an unchanged Kuma
    /// produce the **same bytes**, so the engine sees no change and the adapter
    /// hands its cursor back untouched.
    #[test]
    fn an_unchanged_corpus_encodes_to_the_same_bytes() {
        let rows = [
            (
                "7",
                "gitea",
                json!({ "state": "up", "response_time_ms": 35.0 }),
            ),
            ("1", "knobas-teamcity", json!({ "state": "down" })),
        ];
        assert_eq!(position(&rows).encode(), position(&rows).encode());
        // ... and the order Kuma happened to list them in is not part of it.
        let mut reversed = rows.clone();
        reversed.reverse();
        assert_eq!(position(&rows).encode(), position(&reversed).encode());
    }

    /// Every field the mirror stores is inside the digest, so a poll that found
    /// a monitor slower, renamed, or in another state is a poll that emits.
    #[test]
    fn a_changed_payload_changes_the_position() {
        let before = position(&[(
            "7",
            "gitea",
            json!({ "state": "up", "response_time_ms": 35.0 }),
        )]);
        for after in [
            position(&[(
                "7",
                "gitea",
                json!({ "state": "down", "response_time_ms": 35.0 }),
            )]),
            position(&[(
                "7",
                "gitea",
                json!({ "state": "up", "response_time_ms": 36.0 }),
            )]),
            position(&[(
                "7",
                "gitea-eu",
                json!({ "state": "up", "response_time_ms": 35.0 }),
            )]),
            position(&[(
                "8",
                "gitea",
                json!({ "state": "up", "response_time_ms": 35.0 }),
            )]),
        ] {
            assert_ne!(before, after, "{}", after.encode());
        }
    }

    /// The tombstone list, and the name a tombstone renders with.
    #[test]
    fn a_monitor_the_new_corpus_lacks_is_named_as_vanished() {
        let before = position(&[
            ("7", "gitea", json!({ "state": "up" })),
            ("9", "knobas-live-scratch", json!({ "state": "up" })),
        ]);
        let after = position(&[("7", "gitea", json!({ "state": "up" }))]);
        let gone: Vec<_> = before.vanished(&after).collect();
        assert_eq!(gone, vec![("9", "knobas-live-scratch")]);
        // ... and nothing is owed in the other direction: a monitor that
        // appeared is emitted, not tombstoned.
        assert_eq!(after.vanished(&before).count(), 0);
    }

    /// A cursor round-trips, and one this version cannot read is a miss rather
    /// than a failure -- the run emits everything and stores a readable one.
    #[test]
    fn an_unreadable_cursor_is_a_miss_and_not_a_failure() {
        let stored = position(&[("7", "gitea", json!({ "state": "up" }))]);
        assert_eq!(Position::parse(&stored.encode()), Some(stored));
        assert_eq!(Position::parse(""), None);
        assert_eq!(Position::parse("2026-09-06T12:00:00Z"), None);
        assert_eq!(
            Position::parse(&json!({ "version": 2, "monitors": {} }).to_string()),
            None,
            "a cursor from a later version is not guessed at"
        );
    }
}
