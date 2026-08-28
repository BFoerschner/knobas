//! When a nightly backup is due, and which archives have aged out.
//!
//! Pure, and separated from everything that touches a disk or a database for
//! one reason: "nightly" is the only interesting decision in this feature, and
//! a decision buried in a task loop can only be tested by waiting for it.
//!
//! # What "nightly" means here
//!
//! *A backup is due when the last one is older than the most recent occurrence
//! of the scheduled local time.* Ratified 2026-08-28 (issue #38), against the
//! obvious alternative. Not "a timer that fires at 03:00": a desktop
//! is asleep at 03:00 more often than it is awake, and a schedule expressed as
//! a moment simply does not happen on those days. Expressed as a **boundary**,
//! the same rule backs up on the next wake instead, and asks nothing of the
//! machine having been on.
//!
//! It also makes the decision idempotent. Nothing records "the 03:00 run
//! happened"; the last archive's own timestamp is the whole state, so a
//! restart, a retry and a second window all reach the same answer.
//!
//! # Why the comparison is in local time
//!
//! Because the user picked a local hour, and 03:00 has to keep meaning 03:00
//! across a daylight-saving change. Both sides are converted to the caller's
//! zone and compared as naive local timestamps, which is also what keeps the
//! rule total: the hour that does not exist on a spring-forward night and the
//! hour that happens twice on a fall-back one are ordinary comparisons here,
//! where converting a boundary *back* to UTC would have to answer "which one".

use chrono::{DateTime, Datelike, Duration, NaiveDateTime, TimeZone, Timelike, Utc};

/// The nightly export schedule (spec §14: "scheduled automatic exports as
/// backups", ratified nightly in #38).
///
/// `Deserialize` because [`set_backup_schedule`](crate::commands::backup) takes
/// one from the settings dialog (issue #69); `Serialize` because it is stored as JSON in
/// `knobas.setting` and read back by [`backup_status`](crate::commands::backup).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BackupSchedule {
    /// Whether the nightly export runs at all. *Export now* works either way.
    pub enabled: bool,
    /// Local hour, 0-23.
    pub hour: u32,
    /// Local minute, 0-59.
    pub minute: u32,
    /// How many archives to keep (ratified 2026-08-28, default seven). The
    /// oldest beyond this are deleted after a successful export.
    pub keep: u32,
}

impl Default for BackupSchedule {
    /// Enabled, 03:00 local, seven kept.
    ///
    /// All three ratified 2026-08-28 on issue #38, along with the boundary
    /// reading of "nightly" below. 03:00 is the usual answer for a machine
    /// that is either asleep (in which case the boundary rule catches it on
    /// waking) or idle. Seven is a week -- enough to notice a mistake and go
    /// back past it, at a few megabytes each.
    fn default() -> Self {
        Self {
            enabled: true,
            hour: 3,
            minute: 0,
            keep: 7,
        }
    }
}

impl BackupSchedule {
    /// The same schedule with impossible values brought back into range.
    ///
    /// A schedule arrives from two places that can both be wrong: a settings
    /// dialog, and a `knobas.setting` row written by an older or newer knobas.
    /// Neither is worth refusing a backup over -- an out-of-range hour means
    /// the *scheduler* would otherwise be the thing that breaks, and a
    /// scheduler that silently stops is the failure this feature exists to
    /// avoid.
    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            enabled: self.enabled,
            hour: self.hour.min(23),
            minute: self.minute.min(59),
            // At least one: `keep: 0` would delete the archive it just wrote.
            keep: self.keep.clamp(1, 365),
        }
    }
}

/// The most recent occurrence of the scheduled local time, at or before
/// `local_now`.
fn boundary(schedule: BackupSchedule, local_now: NaiveDateTime) -> NaiveDateTime {
    let schedule = schedule.clamped();
    let today = local_now
        .date()
        .and_hms_opt(schedule.hour, schedule.minute, 0)
        .unwrap_or(local_now);
    if today <= local_now {
        today
    } else {
        today - Duration::days(1)
    }
}

/// Whether a nightly backup should run now.
///
/// `last` is when the last successful export was taken; `None` means never,
/// and a machine that has never backed up is due immediately rather than at
/// the next boundary.
pub fn is_due<Tz: TimeZone>(
    schedule: BackupSchedule,
    last: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    zone: &Tz,
) -> bool {
    if !schedule.enabled {
        return false;
    }
    let local_now = now.with_timezone(zone).naive_local();
    match last {
        None => true,
        Some(last) => last.with_timezone(zone).naive_local() < boundary(schedule, local_now),
    }
}

/// When the next nightly export is expected, for the settings dialog to show.
///
/// `None` when the schedule is off. Otherwise the boundary already passed
/// (i.e. *now*, because a due backup is due now) or the next one.
pub fn next_due<Tz: TimeZone>(
    schedule: BackupSchedule,
    last: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    zone: &Tz,
) -> Option<DateTime<Utc>> {
    if !schedule.enabled {
        return None;
    }
    if is_due(schedule, last, now, zone) {
        return Some(now);
    }
    let local_now = now.with_timezone(zone).naive_local();
    let next = boundary(schedule, local_now) + Duration::days(1);
    // A local time can be ambiguous (the hour that repeats on a fall-back
    // night) or absent (the hour skipped on a spring-forward one). `earliest`
    // answers the first; for the second there is no such instant at all, so
    // the schedule is reported a minute later rather than not at all -- this
    // is a label on a settings screen, and `is_due` above is what actually
    // decides.
    zone.from_local_datetime(&next)
        .earliest()
        .or_else(|| {
            zone.from_local_datetime(&(next + Duration::hours(1)))
                .earliest()
        })
        .map(|dt| dt.with_timezone(&Utc))
}

/// The archive file name for an export taken at `local` (spec §14: `*.knobas`).
///
/// Local rather than UTC, and sortable rather than pretty: the user reads
/// these in a file manager, in their own clock, and a lexicographic sort of
/// the names is a chronological one -- which is what [`expired`] rests on.
pub fn archive_name<Tz: TimeZone>(local: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    format!(
        "knobas-{:04}{:02}{:02}-{:02}{:02}{:02}.{}",
        local.year(),
        local.month(),
        local.day(),
        local.hour(),
        local.minute(),
        local.second(),
        knobas_db::backup::ARCHIVE_EXTENSION
    )
}

/// Whether a file name is one of ours.
///
/// A **file name**, and the check is structural rather than textual, because
/// [`restore`](crate::backup::restore) joins this onto the backup directory: a
/// prefix-and-suffix test alone waves through `knobas-x/../../elsewhere.knobas`,
/// which carries our prefix and our extension and still names a file two
/// directories up. So the name has to be one ordinary path component before
/// it is anything else -- no separator, no `..`, no root.
pub fn is_archive_name(name: &str) -> bool {
    let mut components = std::path::Path::new(name).components();
    let (Some(std::path::Component::Normal(only)), None) = (components.next(), components.next())
    else {
        return false;
    };
    only.to_str().is_some_and(|name| {
        name.starts_with("knobas-")
            && name.ends_with(&format!(".{}", knobas_db::backup::ARCHIVE_EXTENSION))
    })
}

/// Which of `names` have aged out, newest-first retention of `keep`.
///
/// Takes names rather than paths, and returns names, so the retention rule is
/// decided without a directory in sight. The caller does the deleting.
pub fn expired(names: &[String], keep: u32) -> Vec<String> {
    let mut ours: Vec<&String> = names.iter().filter(|n| is_archive_name(n)).collect();
    // The timestamp is fixed-width and zero-padded, so byte order is time
    // order. Reversed: newest first, and everything past `keep` goes.
    ours.sort_unstable();
    ours.reverse();
    ours.into_iter()
        .skip(keep.max(1) as usize)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    /// A zone two hours east, so "local" is demonstrably not UTC in these
    /// tests and an implementation that compared UTC timestamps would pass by
    /// accident.
    fn zone() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).unwrap()
    }

    /// A UTC instant from a local wall clock in [`zone`].
    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        zone()
            .with_ymd_and_hms(y, m, d, h, min, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn nightly() -> BackupSchedule {
        BackupSchedule {
            enabled: true,
            hour: 3,
            minute: 0,
            keep: 7,
        }
    }

    /// The whole point of the boundary rule: a machine that was asleep at
    /// 03:00 backs up when it wakes, rather than skipping the night.
    #[test]
    fn a_machine_asleep_at_the_scheduled_hour_is_due_when_it_wakes() {
        let last = local(2026, 8, 27, 3, 0);
        // 09:14 local the next morning: 03:00 came and went while it slept.
        assert!(is_due(
            nightly(),
            Some(last),
            local(2026, 8, 28, 9, 14),
            &zone()
        ));
    }

    /// ...and having backed up after the boundary, it is not due again until
    /// the next one.
    #[test]
    fn a_backup_taken_after_the_boundary_settles_the_night() {
        let last = local(2026, 8, 28, 9, 14);
        assert!(!is_due(
            nightly(),
            Some(last),
            local(2026, 8, 28, 23, 59),
            &zone()
        ));
        // Past the next 03:00, it is due again.
        assert!(is_due(
            nightly(),
            Some(last),
            local(2026, 8, 29, 3, 0),
            &zone()
        ));
    }

    /// Before the first boundary of the day, last night's backup still counts.
    #[test]
    fn the_small_hours_before_the_boundary_belong_to_the_previous_night() {
        let last = local(2026, 8, 28, 3, 0);
        assert!(!is_due(
            nightly(),
            Some(last),
            local(2026, 8, 29, 2, 59),
            &zone()
        ));
        assert!(is_due(
            nightly(),
            Some(last),
            local(2026, 8, 29, 3, 1),
            &zone()
        ));
    }

    /// A machine that has never backed up does not wait for 03:00.
    #[test]
    fn a_database_that_has_never_been_backed_up_is_due_at_once() {
        assert!(is_due(nightly(), None, local(2026, 8, 28, 12, 0), &zone()));
    }

    /// Turning the schedule off stops the nightly run and nothing else.
    #[test]
    fn a_disabled_schedule_is_never_due() {
        let off = BackupSchedule {
            enabled: false,
            ..nightly()
        };
        assert!(!is_due(off, None, local(2026, 8, 28, 12, 0), &zone()));
        assert_eq!(
            next_due(off, None, local(2026, 8, 28, 12, 0), &zone()),
            None
        );
    }

    /// The label the settings dialog shows: due now while it is due, and the
    /// next boundary once it is not.
    #[test]
    fn the_next_run_is_now_while_due_and_the_next_boundary_once_not() {
        let now = local(2026, 8, 28, 9, 14);
        assert_eq!(next_due(nightly(), None, now, &zone()), Some(now));

        let last = local(2026, 8, 28, 9, 0);
        assert_eq!(
            next_due(nightly(), Some(last), now, &zone()),
            Some(local(2026, 8, 29, 3, 0)),
            "the next 03:00 local, expressed in UTC"
        );
    }

    /// An hour out of range is clamped rather than refused: a scheduler that
    /// stops because a stored value is odd is worse than one that runs an hour
    /// early.
    #[test]
    fn an_impossible_schedule_is_clamped_into_range() {
        let broken = BackupSchedule {
            enabled: true,
            hour: 99,
            minute: 99,
            keep: 0,
        }
        .clamped();
        assert_eq!(broken.hour, 23);
        assert_eq!(broken.minute, 59);
        assert_eq!(broken.keep, 1, "keeping zero would delete the new archive");
    }

    /// The name is sortable, so retention needs no `stat`.
    #[test]
    fn an_archive_name_sorts_chronologically() {
        let earlier = archive_name(&zone().with_ymd_and_hms(2026, 8, 28, 3, 0, 0).unwrap());
        let later = archive_name(&zone().with_ymd_and_hms(2026, 9, 1, 3, 0, 0).unwrap());
        assert_eq!(earlier, "knobas-20260828-030000.knobas");
        assert!(earlier < later);
        assert!(is_archive_name(&earlier));
        assert!(!is_archive_name("notes.txt"));
        assert!(!is_archive_name("knobas-20260828-030000.tar"));
    }

    /// An archive name is a *file* name.
    ///
    /// `restore` joins it onto the backup directory, so a name carrying a path
    /// resolves somewhere the directory does not reach. The dangerous shape is
    /// not `../outside.knobas` -- that fails the prefix on its own -- but one
    /// wearing both our prefix and our extension with a path in the middle.
    #[test]
    fn a_name_carrying_a_path_is_not_an_archive_name() {
        assert!(!is_archive_name("knobas-x/../../outside.knobas"));
        assert!(!is_archive_name("knobas-/etc/passwd.knobas"));
        assert!(!is_archive_name("subdir/knobas-20260828-030000.knobas"));
        assert!(!is_archive_name("/knobas-20260828-030000.knobas"));
        assert!(
            is_archive_name("knobas-20260828-030000.knobas"),
            "and the ordinary name is still one"
        );
    }

    /// Retention keeps the newest and names the rest, and it never touches a
    /// file that is not ours.
    #[test]
    fn retention_keeps_the_newest_and_spares_what_is_not_ours() {
        let names: Vec<String> = [
            "knobas-20260826-030000.knobas",
            "knobas-20260827-030000.knobas",
            "knobas-20260828-030000.knobas",
            "the-users-own-export.knobas.bak",
            "readme.txt",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();

        assert_eq!(
            expired(&names, 2),
            vec!["knobas-20260826-030000.knobas".to_owned()]
        );
        assert!(expired(&names, 3).is_empty());
        assert!(
            expired(&names, 10).is_empty(),
            "keeping more than there are deletes nothing"
        );
        assert_eq!(
            expired(&names, 1),
            vec![
                "knobas-20260827-030000.knobas".to_owned(),
                "knobas-20260826-030000.knobas".to_owned(),
            ],
            "the oldest go first"
        );
    }
}
