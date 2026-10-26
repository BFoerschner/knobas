/**
 * The Settings sections besides Sources (#831), in prototype-6's words.
 *
 * Pure, as `settings.ts` is: the section components hand in what the IPC
 * answered and draw the strings that come back.
 */
import type { BackupRecord, BackupSchedule, ShareParts } from "../../ipc/backup";
import type { InboxCategory } from "../../ipc/entity";
import type { DbStats } from "../../ipc/sources";
import { formatBytes } from "../../sources/diagnostics";
import { stampLabel } from "../../time/format";

/** One tick under *Share with a colleague*. */
export interface ShareTick {
  part: keyof ShareParts;
  label: string;
}

/**
 * The ticks, in prototype-6's order. Each is a part of the share export
 * (`ShareParts`). *Smart lists* is drawn only once a saved list exists,
 * *Ticket views* (#812) only once a view exists and *Link sets* (#870) only
 * once a set exists: each part is a box about nothing before that
 * (CONTEXT.md, *Share export*).
 */
export function shareTicks(savedLists: boolean, savedViews = false, savedSets = false): ShareTick[] {
  const all: ShareTick[] = [
    { part: "links", label: "Links" },
    { part: "link_sets", label: "Link sets" },
    { part: "ticket_views", label: "Ticket views" },
    { part: "contexts", label: "Contexts" },
    { part: "assets", label: "Assets and routes" },
    { part: "sources", label: "Sources, without passwords" },
    { part: "smart_lists", label: "Smart lists" },
    { part: "notes", label: "Notes" },
    { part: "time", label: "Time" },
  ];
  return all.filter(
    (tick) =>
      (savedLists || tick.part !== "smart_lists") &&
      (savedViews || tick.part !== "ticket_views") &&
      (savedSets || tick.part !== "link_sets"),
  );
}

const pad = (n: number) => String(n).padStart(2, "0");

/**
 * *Last backup*: *today 02:00, 48 MB, in ~/Library/…/backups*; an older one
 * keeps its clock, *2026-10-02 23:00, …* (`stampLabel`, #1986).
 */
export function lastBackupWords(last: BackupRecord | null, directory: string, now: Date = new Date()): string {
  if (last === null) return "None yet";
  return `${stampLabel(last.taken_at, now)}, ${formatBytes(last.bytes)}, in ${directory}`;
}

/**
 * The daily option of *Back up*, at the stored time. `BackupSchedule` has no
 * weekday, so prototype-6's *Every week* has no field to store and is not
 * offered.
 */
export function scheduleWords(schedule: BackupSchedule): string {
  return `Every day at ${pad(schedule.hour)}:${pad(schedule.minute)}`;
}

/**
 * A number field's value, never `NaN`: a cleared field reads `""`, which the
 * Rust `u32` refuses. Clamped, so `keep: 0` cannot delete the archive just
 * written (`BackupSchedule::clamped`).
 */
export function boundedNumber(value: string, min: number, max: number, fallback: number): number {
  const parsed = Number.parseInt(value, 10);
  if (Number.isNaN(parsed)) return fallback;
  return Math.min(max, Math.max(min, parsed));
}

/** Prototype-6 groups thousands with a space: *18 400*. */
const grouped = (n: number) => n.toLocaleString("en-GB").replace(/,/g, " ");

/** *Database*: *1.2 GB, 18 400 items*. */
export function databaseWords(stats: Pick<DbStats, "db_bytes" | "item_count">): string {
  return `${formatBytes(stats.db_bytes)}, ${grouped(stats.item_count)} ${stats.item_count === 1 ? "item" : "items"}`;
}

/**
 * The Notifications switches, in prototype-6's words and in
 * `knobas_core::inbox::Category::ALL`'s order, which
 * `every_inbox_category_has_a_toggle_in_the_interface` pins (#881).
 */
export const NOTIFY_KINDS: readonly { id: InboxCategory; label: string }[] = [
  { id: "review_request", label: "Review asked of you" },
  { id: "mention", label: "You are mentioned" },
  { id: "failed_build", label: "A build you started or care about failed" },
  { id: "new_assignment", label: "A ticket is assigned to you" },
  { id: "credential_expiry", label: "A source's credential is about to expire" },
  { id: "failed_sign_in", label: "A source cannot sign in" },
  { id: "alert", label: "A monitor goes down" },
];

/** Keyboard: requirements §14, in prototype-6's words. Fixed in this version. */
export const SHORTCUTS: readonly (readonly [keys: string, where: string, does: string])[] = [
  ["⌘K", "Anywhere", "Search everything"],
  ["⌘T", "Anywhere", "Stop or start the timer"],
  ["⌘,", "Anywhere", "Settings"],
  ["⌘[ ⌘] ⌘← ⌘→", "Anywhere", "Back and forward through what you opened; the mouse's buttons too"],
  ["⌘R", "A detail or a role view", "Refresh, or Sync now"],
  ["/", "A list", "Focus the filter"],
  ["Esc", "Anywhere", "Back one step: dialog, drawer, filter, view, job"],
  ["J K ↑ ↓", "Tickets", "Move the selection"],
  ["Enter", "Tickets", "Open the job"],
  ["L", "Job", "Connections"],
  ["Y N S", "Proposed links", "Link, not related, decide later"],
  ["Enter, Shift Enter", "Build log search", "Next and previous match"],
];
