//! The incremental position: `{"v":1,"since_build_id":N}` (interfaces §4.2).
//!
//! Opaque to everything outside this crate, versioned so a later shape change
//! is detectable, and byte-stable so an idle run can hand back exactly what it
//! was given (contract battery clause 2).

/// Bump when the envelope's meaning changes; every stored cursor then reads as
/// "unknown" and the next run is a full sync.
const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct CursorState {
    /// Field order is the serialized order: `{"v":…,"since_build_id":…}`.
    pub v: u32,
    pub since_build_id: i64,
}

pub(crate) fn new(since_build_id: i64) -> CursorState {
    CursorState {
        v: VERSION,
        since_build_id,
    }
}

pub(crate) fn render(state: CursorState) -> String {
    serde_json::to_string(&state).expect("a two-field struct of numbers always serializes")
}

/// `None` means "this position is not one this adapter wrote" -- an older
/// envelope, a different adapter's cursor, or garbage. The caller does a full
/// sync, which is the only safe reading.
pub(crate) fn parse(raw: &str) -> Option<CursorState> {
    let state: CursorState = serde_json::from_str(raw).ok()?;
    (state.v == VERSION).then_some(state)
}

/// Where the watermark stands after a run.
///
/// * `previous` -- where it stood before.
/// * `max_finished` -- the newest **finished** build this run observed.
/// * `min_unfinished` -- the oldest **in-scope** build this run saw queued or
///   running.
/// * `ceiling` -- the highest build id that existed when the run **started**.
///
/// TeamCity assigns a build's id when it is **queued**, so ids are monotonic
/// in queue order, not in finish order. A build queued at id 1150 that is
/// still running while 1200 finishes would be invisible to
/// `sinceBuild:(id:1200)` forever once it finished -- so the watermark is held
/// one below the oldest build still in flight. The cost is re-fetching a
/// handful of already-mirrored builds, and upserts are idempotent.
///
/// The asymmetry between the first two of those is deliberate and is the run's
/// job to honour: `max_finished` counts **every** finished build the run saw,
/// in scope or not, because a build outside the scope will never be emitted
/// and so never needs to be reachable again; `min_unfinished` counts only
/// in-scope builds, because clamping for a build that will never be emitted is
/// a watermark held back for nothing. See `sync::execute`.
///
/// # The ceiling
///
/// `min_unfinished` protects builds the run *saw* in flight. It cannot protect
/// a build that did not exist when the run looked: one queued after the
/// opening poll, still running when the finished query goes out, is in neither
/// set -- nothing emits it and nothing clamps below it -- and a build queued
/// later still that finished inside the same run pushes `max_finished` above
/// it. `sinceBuild` would then never offer it again, and a full sync is a
/// window rather than the corpus, so not even a cursor reset recovers it.
///
/// Ids are assigned at queue time and are monotonic, so **every** build queued
/// after the run started has an id above every build that existed when it
/// started. One number therefore covers all of them at once: the highest id in
/// existence at that instant, which the watermark may not pass. The cost is
/// re-fetching the builds that finished inside this run on the next one, and
/// upserts are idempotent -- the same trade the clamp above already makes.
///
/// `None` means the run could not name one (a server with no builds at all),
/// which is the same server on which `max_finished` is `None` too.
pub(crate) fn advance(
    previous: i64,
    max_finished: Option<i64>,
    min_unfinished: Option<i64>,
    ceiling: Option<i64>,
) -> i64 {
    let mut next = max_finished.map_or(previous, |newest| previous.max(newest));
    if let Some(highest_at_start) = ceiling {
        next = next.min(highest_at_start);
    }
    if let Some(oldest_in_flight) = min_unfinished {
        next = next.min(oldest_in_flight - 1);
    }
    // Never backwards: a watermark that regresses re-emits the same builds on
    // every run, and the engine then writes an activity line for every idle
    // poll.
    next.max(previous)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The envelope shape is fixed by interfaces §4.2 and must stay
    /// byte-stable: an idle run hands back exactly what it was given, and
    /// "exactly" is compared as a string by the contract battery.
    #[test]
    fn the_envelope_is_the_documented_shape() {
        assert_eq!(render(new(412)), r#"{"v":1,"since_build_id":412}"#);
        assert_eq!(render(new(0)), r#"{"v":1,"since_build_id":0}"#);
        assert_eq!(parse(&render(new(412))), Some(new(412)));
    }

    /// An unrecognised version means "full sync" (interfaces §4.1), not a
    /// failed run: the shape changed under a stored cursor, and re-reading
    /// everything is the recovery.
    #[test]
    fn an_unknown_cursor_asks_for_a_full_sync() {
        assert_eq!(parse(r#"{"v":2,"since_build_id":412}"#), None);
        assert_eq!(parse("tidewater-v1"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse(r#"{"since_build_id":412}"#), None);
        // Another adapter's envelope: same `v`, different payload.
        assert_eq!(
            parse(r#"{"v":1,"updated_at":"2026-08-22T10:00:00Z"}"#),
            None
        );
    }

    #[test]
    fn the_watermark_advances_to_the_newest_finished_build() {
        assert_eq!(advance(0, Some(412), None, Some(412)), 412);
        assert_eq!(advance(412, Some(1187), None, Some(1187)), 1187);
    }

    /// Nothing finished: the position is where it was. This is the idle poll,
    /// and moving here would make every five-minute tick look like a change.
    #[test]
    fn the_watermark_stands_still_when_nothing_finished() {
        assert_eq!(advance(1187, None, None, Some(1187)), 1187);
        assert_eq!(advance(1187, None, Some(1188), Some(1188)), 1187);
    }

    /// A build still queued or running keeps the watermark below it: its id
    /// was assigned when it was queued, so advancing past it would put it
    /// permanently out of reach of `sinceBuild` once it finishes.
    #[test]
    fn the_watermark_stays_below_the_oldest_build_still_in_flight() {
        assert_eq!(advance(0, Some(1187), Some(1188), Some(1188)), 1187);
        assert_eq!(advance(0, Some(1200), Some(1150), Some(1200)), 1149);
        // ...but never backwards: re-emitting forever is not a recovery.
        assert_eq!(advance(1187, Some(1200), Some(1150), Some(1200)), 1187);
    }

    #[test]
    fn the_watermark_never_regresses() {
        assert_eq!(advance(1187, Some(400), None, Some(400)), 1187);
        assert_eq!(advance(1187, Some(400), Some(2), Some(400)), 1187);
    }

    /// The clamp is strictly below the in-flight build, not equal to it:
    /// `sinceBuild:(id:N)` is exclusive, so a watermark *at* 1188 would never
    /// return 1188 again.
    #[test]
    fn the_clamp_is_one_below_not_equal() {
        assert_eq!(
            advance(0, Some(5000), Some(1188), Some(5000)),
            1187,
            "sinceBuild is exclusive, so the watermark must sit below the build it protects"
        );
    }

    /// The ceiling: the watermark may not pass the highest build id that
    /// existed when the run **started**.
    ///
    /// 1200 finished inside the run, but neither it nor 1100 existed when the
    /// run opened -- and 1100 is still running, in no set the run can see. A
    /// watermark at 1200 would put 1100 out of `sinceBuild`'s reach for good.
    #[test]
    fn the_watermark_never_passes_the_highest_build_id_at_run_start() {
        assert_eq!(advance(900, Some(1200), None, Some(1000)), 1000);
        // It is a ceiling and not a floor: nothing is dragged *up* to it.
        assert_eq!(advance(900, Some(950), None, Some(1000)), 950);
        // ...and the in-flight clamp still wins wherever it is lower.
        assert_eq!(advance(0, Some(1200), Some(500), Some(1000)), 499);
    }

    /// A ceiling below where the watermark already stands does not drag it
    /// back. A regressing watermark re-emits the same builds on every run for
    /// ever, which is not a recovery from anything.
    #[test]
    fn the_ceiling_never_pushes_the_watermark_backwards() {
        assert_eq!(advance(1187, Some(1200), None, Some(500)), 1187);
    }

    /// No ceiling is the server with no builds on it at all -- which is the
    /// same server on which nothing has finished either.
    #[test]
    fn an_unknown_ceiling_clamps_nothing() {
        assert_eq!(advance(412, Some(1200), None, None), 1200);
    }

    /// A build queued at id 1 while nothing has ever finished pins the
    /// watermark at 0 -- the same position a source that has never synced
    /// holds, which is exactly right: nothing is safe to skip yet.
    #[test]
    fn an_in_flight_build_at_the_very_first_id_pins_the_watermark_at_zero() {
        assert_eq!(advance(0, Some(9), Some(1), Some(9)), 0);
    }
}
