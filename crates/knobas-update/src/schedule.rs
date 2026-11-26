//! When knobas checks on its own (decisions D6).
//!
//! 60 s after the app starts, unless the last finished check is less than an hour old; then
//! whenever 6 h have passed since the last finished check. After a failed check the next try is
//! in 1 h, or at GitHub's reset time if that is later. Never in a debug build, in the demo
//! profile, with *Check for updates* off or with `KNOBAS_UPDATE=off`.

use chrono::{DateTime, Duration, Utc};

/// After the start, before the first check.
pub const AFTER_START: Duration = Duration::seconds(60);
/// Between two checks.
pub const EVERY: Duration = Duration::hours(6);
/// After a failed check; also how old the last check may be for the start to skip its own.
pub const AFTER_FAILURE: Duration = Duration::hours(1);
/// How often the loop looks at the wall clock, so a laptop that slept catches up on waking.
pub const LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(600);

/// Whether this copy checks on its own at all.
#[must_use]
pub fn checks_itself(debug: bool, demo: bool, env: Option<&str>) -> bool {
    !debug && !demo && !env.is_some_and(|value| value.trim().eq_ignore_ascii_case("off"))
}

/// What the schedule reads.
#[derive(Debug, Clone, Copy)]
pub struct Input {
    pub started_at: DateTime<Utc>,
    /// The last check that finished, ok or not, and when.
    pub last: Option<DateTime<Utc>>,
    pub last_failed: bool,
    /// GitHub's `x-ratelimit-reset`, after a rate-limited check.
    pub retry_at: Option<DateTime<Utc>>,
    /// The *Check for updates* switch.
    pub check: bool,
    /// [`checks_itself`].
    pub checks_itself: bool,
}

/// When the next check on its own is due, or `None` for never.
#[must_use]
pub fn due_at(input: Input) -> Option<DateTime<Utc>> {
    if !(input.check && input.checks_itself) {
        return None;
    }
    let first = input.started_at + AFTER_START;
    let Some(last) = input.last else {
        return Some(first);
    };
    if input.last_failed {
        let after = (last + AFTER_FAILURE).max(input.retry_at.unwrap_or(last));
        return Some(after.max(first));
    }
    if last + AFTER_FAILURE <= first {
        Some(first.min(last + EVERY))
    } else {
        Some(last + EVERY)
    }
}

/// Whether a check on its own is due at `now`.
#[must_use]
pub fn is_due(input: Input, now: DateTime<Utc>) -> bool {
    due_at(input).is_some_and(|due| due <= now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap() + Duration::minutes(minutes)
    }

    fn input(last: Option<i64>, failed: bool) -> Input {
        Input {
            started_at: at(0),
            last: last.map(at),
            last_failed: failed,
            retry_at: None,
            check: true,
            checks_itself: true,
        }
    }

    /// The table of decisions D6. Mutant: drop the demo rule, and the last row fails.
    #[test]
    fn the_schedule() {
        // The first check of a run, a minute after the start.
        assert!(!is_due(input(None, false), at(0)));
        assert!(is_due(input(None, false), at(1)));
        // A check this run: due at 6 h, not at 5 h 59 min.
        assert!(!is_due(input(Some(10), false), at(10 + 359)));
        assert!(is_due(input(Some(10), false), at(10 + 360)));
        // The last check 30 minutes before the start: none at start, the next at its 6 h.
        assert!(!is_due(input(Some(-30), false), at(5)));
        assert!(is_due(input(Some(-30), false), at(330)));
        // Two hours old at the start: the start's own check.
        assert!(is_due(input(Some(-120), false), at(1)));
        // After a failure, an hour later.
        assert!(!is_due(input(Some(10), true), at(69)));
        assert!(is_due(input(Some(10), true), at(70)));
        // Or GitHub's reset, when that is later.
        let mut limited = input(Some(10), true);
        limited.retry_at = Some(at(100));
        assert!(!is_due(limited, at(99)));
        assert!(is_due(limited, at(100)));
        // Never with the switch off.
        let mut off = input(None, false);
        off.check = false;
        assert!(!is_due(off, at(10_000)));
        // Never in a debug build, the demo profile or under KNOBAS_UPDATE=off.
        assert!(!checks_itself(true, false, None));
        assert!(!checks_itself(false, true, None));
        assert!(!checks_itself(false, false, Some("off")));
        assert!(checks_itself(false, false, Some("install")));
        assert!(checks_itself(false, false, None));
    }
}
