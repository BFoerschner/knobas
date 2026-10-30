/**
 * `?fake-ipc`: The Settings pages that read a stored preference (#831): Time (passive
 * attribution), Monitoring and Backup and export's status.
 *
 * Answers `pages_kept_open`, `set_pages_kept_open` (#2247), `passive_attribution`, `set_passive_attribution`,
 * `monitoring_settings`, `set_monitoring_settings`, `backup_status`,
 * `backup_now`, `set_backup_schedule` and `open_backup_folder` (which opens
 * nothing: a browser has no file manager to show the folder in).
 * Notifications are `inbox.ts`'s and Standup's publish target is
 * `standup.ts`'s, each beside the data it names.
 *
 * **Values are prototype-6's `STGV` pages**: passive blocks on; slow above
 * 2000 ms and samples kept 30 days; the last backup "today 02:00, 48 MB",
 * every day at 02:00, keep 14.
 *
 * **Where this diverges from prototype-6:**
 *
 * - `share_export` and `restore_backup` stay unanswered, with the census's
 *   reason (the deputy's ruling of 2026-09-08 on #507: a fake would certify
 *   nothing about an archive). Prototype-6's *Export...* and *Restore from a
 *   file...* buttons therefore reject under `?fake-ipc`.
 * - Prototype-6 draws the schedule as "Every day at 02:00 / Every week /
 *   Never"; `BackupSchedule` is `{enabled, hour, minute, keep}`, so a weekly
 *   choice has no field (listed in the PR, not invented here).
 * - Prototype-6's "Back up" label is the nightly export; `backup_now` answers
 *   a record and adds an archive to the list, nothing more.
 */
import type { BackupRecord, BackupSchedule, BackupStatus } from "../../../ipc/backup";
import type { MonitoringSettings } from "../../../ipc/assets";
import type { Handlers } from "../fake-tauri";

const DIRECTORY = "~/Library/Application Support/knobas/backups";

/** Today at `hour:minute` by the browser's clock, so "today 02:00" stays today. */
function todayAt(hour: number, minute: number): Date {
  const at = new Date();
  at.setHours(hour, minute, 0, 0);
  return at;
}

function archiveName(at: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `knobas-${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}-${pad(at.getHours())}${pad(at.getMinutes())}.knobas`;
}

function record(at: Date, bytes: number): BackupRecord {
  return { taken_at: at.toISOString(), file: `${DIRECTORY}/${archiveName(at)}`, bytes };
}

/** This feature's commands. */
export function handlers(): Handlers {
  let passive = true;
  // `?fake-pages-kept-open=<n>` starts *Pages kept open* at n (the keep-alive budget, #2250).
  const keptAsked = new URLSearchParams(globalThis.location?.search).get("fake-pages-kept-open");
  let keptOpen = keptAsked === null ? 5 : Math.min(15, Math.max(0, Math.round(Number(keptAsked)) || 0));
  let monitoring: MonitoringSettings = { sample_retention_days: 30, response_time_warn_ms: 2000 };
  let schedule: BackupSchedule = { enabled: true, hour: 2, minute: 0, keep: 14 };
  const taken = todayAt(2, 0);
  const MB = 1024 * 1024;
  const archives = [record(taken, 48 * MB)];
  for (const back of [1, 2, 3]) {
    const earlier = new Date(taken.getTime() - back * 86_400_000);
    archives.push(record(earlier, 47 * MB - back * 200_000));
  }

  const status = (): BackupStatus => {
    const next = todayAt(schedule.hour, schedule.minute);
    if (next.getTime() <= Date.now()) next.setDate(next.getDate() + 1);
    return {
      schedule,
      directory: DIRECTORY,
      last: archives[0] ?? null,
      next_due_at: schedule.enabled ? next.toISOString() : null,
      archives: archives.slice(0, Math.max(1, schedule.keep)).map((made) => ({
        file: made.file,
        bytes: made.bytes,
        share: false,
      })),
    };
  };

  return {
    pages_kept_open: () => keptOpen,
    set_pages_kept_open: (args) => {
      keptOpen = Math.min(15, Math.max(0, Math.round(Number(args["count"]))));
      return keptOpen;
    },
    passive_attribution: () => passive,
    set_passive_attribution: (args) => {
      passive = args["enabled"] === true;
      return passive;
    },
    monitoring_settings: () => monitoring,
    set_monitoring_settings: (args) => {
      const next = args["settings"] as MonitoringSettings;
      if (!(next.sample_retention_days >= 1) || !(next.response_time_warn_ms >= 1)) {
        throw { code: "invalid", message: "both numbers must be at least 1", source_id: null };
      }
      monitoring = next;
      return monitoring;
    },
    backup_status: () => status(),
    backup_now: () => {
      const made = record(new Date(), 48 * MB + 100_000);
      archives.unshift(made);
      return made;
    },
    set_backup_schedule: (args) => {
      schedule = args["schedule"] as BackupSchedule;
      return status();
    },
    open_backup_folder: () => null,
  };
}
