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
///
/// TeamCity assigns a build's id when it is **queued**, so ids are monotonic
/// in queue order, not in finish order. A build queued at id 1150 that is
/// still running while 1200 finishes would be invisible to
/// `sinceBuild:(id:1200)` forever once it finished -- so the watermark is held
/// one below the oldest build still in flight. The cost is re-fetching a
/// handful of already-mirrored builds, and upserts are idempotent.
///
/// The asymmetry between the two arguments is deliberate and is the run's job
/// to honour: `max_finished` counts **every** finished build the run saw, in
/// scope or not, because a build outside the scope will never be emitted and
/// so never needs to be reachable again; `min_unfinished` counts only in-scope
/// builds, because clamping for a build that will never be emitted is a
/// watermark held back for nothing. See `sync::execute`.
pub(crate) fn advance(
    previous: i64,
    max_finished: Option<i64>,
    min_unfinished: Option<i64>,
) -> i64 {
    let mut next = max_finished.map_or(previous, |newest| previous.max(newest));
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
        assert_eq!(advance(0, Some(412), None), 412);
        assert_eq!(advance(412, Some(1187), None), 1187);
    }

    /// Nothing finished: the position is where it was. This is the idle poll,
    /// and moving here would make every five-minute tick look like a change.
    #[test]
    fn the_watermark_stands_still_when_nothing_finished() {
        assert_eq!(advance(1187, None, None), 1187);
        assert_eq!(advance(1187, None, Some(1188)), 1187);
    }

    /// A build still queued or running keeps the watermark below it: its id
    /// was assigned when it was queued, so advancing past it would put it
    /// permanently out of reach of `sinceBuild` once it finishes.
    #[test]
    fn the_watermark_stays_below_the_oldest_build_still_in_flight() {
        assert_eq!(advance(0, Some(1187), Some(1188)), 1187);
        assert_eq!(advance(0, Some(1200), Some(1150)), 1149);
        // ...but never backwards: re-emitting forever is not a recovery.
        assert_eq!(advance(1187, Some(1200), Some(1150)), 1187);
    }

    #[test]
    fn the_watermark_never_regresses() {
        assert_eq!(advance(1187, Some(400), None), 1187);
        assert_eq!(advance(1187, Some(400), Some(2)), 1187);
    }

    /// The clamp is strictly below the in-flight build, not equal to it:
    /// `sinceBuild:(id:N)` is exclusive, so a watermark *at* 1188 would never
    /// return 1188 again.
    #[test]
    fn the_clamp_is_one_below_not_equal() {
        assert_eq!(
            advance(0, Some(5000), Some(1188)),
            1187,
            "sinceBuild is exclusive, so the watermark must sit below the build it protects"
        );
    }

    /// A build queued at id 1 while nothing has ever finished pins the
    /// watermark at 0 -- the same position a source that has never synced
    /// holds, which is exactly right: nothing is safe to skip yet.
    #[test]
    fn an_in_flight_build_at_the_very_first_id_pins_the_watermark_at_zero() {
        assert_eq!(advance(0, Some(9), Some(1)), 0);
    }
}
