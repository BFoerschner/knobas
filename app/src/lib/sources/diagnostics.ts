/**
 * The readings the status bar and the diagnostics view share.
 *
 * Functions rather than five `Math.round`s in five components, because the
 * status bar's line and the `.dbbar`'s line are the *same* numbers and a
 * reader who sees them disagree has no way to tell which one is lying.
 */

/**
 * `db_bytes` as the status bar reads it.
 *
 * **Binary steps with the short labels** — 1 MB is 1 048 576 bytes here. That
 * is not the SI reading, and it is the deliberate one: this number is compared
 * against what `du`, Finder's Get Info and `pg_database_size` report about the
 * same directory, all of which step by 1024, and a status bar that is 5% off
 * every one of them invites a bug report about the wrong thing.
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  // One decimal below ten, none above: "1.4 GB" carries information, "212.0 MB"
  // carries a decimal point.
  const rendered = value < 10 ? value.toFixed(1).replace(/\.0$/, "") : String(Math.round(value));
  return `${rendered} ${units[unit]}`;
}

/**
 * How long until the next scheduled run — the status bar's flap.
 *
 * `—` for a source that is not scheduled: `next_run_at` is null while a run is
 * in flight, and for one that is disabled or needs a human (contract §2.3).
 * **Never negative**: a countdown that has run out reads *due*, because a
 * scheduler that is a few seconds behind its own tick is ordinary and
 * "-0:04" is a fact about two clocks rather than about the sync.
 */
export function countdown(nextRunAt: string | null | undefined, now: Date = new Date()): string {
  if (!nextRunAt) return "—";
  const at = new Date(nextRunAt);
  if (Number.isNaN(at.getTime())) return "—";

  const seconds = Math.floor((at.getTime() - now.getTime()) / 1000);
  if (seconds <= 0) return "due";
  if (seconds < 3600) {
    const minutes = Math.floor(seconds / 60);
    return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
  }
  const hours = Math.floor(seconds / 3600);
  return `${hours}:${String(Math.floor((seconds % 3600) / 60)).padStart(2, "0")}:00`;
}

/**
 * How long a run took, or `null` while it is still going.
 *
 * `null` and not `0`: a run in flight has no duration yet, and rendering one as
 * `0 ms` makes an in-progress sync indistinguishable from an instant success —
 * which is exactly the row a person is looking at when they wonder whether the
 * sync is stuck.
 */
export function runDuration(startedAt: string, finishedAt: string | null): number | null {
  if (!finishedAt) return null;
  const from = new Date(startedAt).getTime();
  const to = new Date(finishedAt).getTime();
  if (Number.isNaN(from) || Number.isNaN(to)) return null;
  return Math.max(0, to - from);
}

/** A duration as the run log prints it: `840 ms`, `4.2 s`, `1:12`. */
export function formatDuration(ms: number | null): string {
  if (ms === null) return "—";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const seconds = Math.round(ms / 1000);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

/** How often a source syncs, as a sentence fragment: `every 15 min`. */
export function formatInterval(secs: number): string {
  if (!Number.isFinite(secs) || secs <= 0) return "—";
  if (secs < 3600) return `every ${Math.round(secs / 60)} min`;
  if (secs < 86_400) {
    const hours = secs / 3600;
    return hours === 1 ? "hourly" : `every ${Math.round(hours)} h`;
  }
  const days = Math.round(secs / 86_400);
  return days === 1 ? "daily" : `every ${days} days`;
}
