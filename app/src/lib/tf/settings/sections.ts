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

/** One tick under *Share with a colleague*. */
export interface ShareTick {
  part: keyof ShareParts;
  label: string;
}

/**
 * The ticks, in prototype-6's order. Each is a part of the share export
 * (`ShareParts`); prototype-6 also draws *Link sets* and *Ticket views*,
 * which no export carries yet, so they are not drawn (#831's criterion).
 * *Smart lists* is drawn only once a saved list exists: the part is a box
 * about nothing before that (CONTEXT.md, *Share export*).
 */
export function shareTicks(savedLists: boolean): ShareTick[] {
  const all: ShareTick[] = [
    { part: "links", label: "Links" },
    { part: "contexts", label: "Contexts" },
    { part: "assets", label: "Assets and routes" },
    { part: "sources", label: "Sources, without passwords" },
    { part: "smart_lists", label: "Smart lists" },
    { part: "notes", label: "Notes" },
    { part: "time", label: "Time" },
  ];
  return savedLists ? all : all.filter((tick) => tick.part !== "smart_lists");
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

const pad = (n: number) => String(n).padStart(2, "0");

/** *today 02:00*, *yesterday 02:00*, *2 Oct 02:00*. */
function dayAndClock(at: Date, now: Date): string {
  const clock = `${pad(at.getHours())}:${pad(at.getMinutes())}`;
  if (at.toDateString() === now.toDateString()) return `today ${clock}`;
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (at.toDateString() === yesterday.toDateString()) return `yesterday ${clock}`;
  return `${at.getDate()} ${MONTHS[at.getMonth()]} ${clock}`;
}

/** *Last backup*: *today 02:00, 48 MB, in ~/Library/…/backups*. */
export function lastBackupWords(last: BackupRecord | null, directory: string, now: Date = new Date()): string {
  if (last === null) return "None yet";
  return `${dayAndClock(new Date(last.taken_at), now)}, ${formatBytes(last.bytes)}, in ${directory}`;
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
 * `knobas_core::inbox::Category::ALL`'s order (the rooms section's list, which
 * `every_inbox_category_has_a_toggle_in_the_interface` pins).
 */
export const NOTIFY_KINDS: readonly { id: InboxCategory; label: string }[] = [
  { id: "review_request", label: "Review asked of you" },
  { id: "mention", label: "You are mentioned" },
  { id: "failed_build", label: "A build you started or care about failed" },
  { id: "new_assignment", label: "A ticket is assigned to you" },
  { id: "credential_expiry", label: "A source cannot sign in" },
  { id: "alert", label: "A monitor goes down" },
];

/** Keyboard: requirements §14, in prototype-6's words. Fixed in this version. */
export const SHORTCUTS: readonly (readonly [keys: string, where: string, does: string])[] = [
  ["⌘K", "Anywhere", "Search everything"],
  ["⌘T", "Anywhere", "Stop or start the timer"],
  ["⌘,", "Anywhere", "Settings"],
  ["/", "A list", "Focus the filter"],
  ["Esc", "Anywhere", "Back one step: dialog, drawer, filter, view, job"],
  ["J K ↑ ↓", "Tickets", "Move the selection"],
  ["Enter", "Tickets", "Open the job"],
  ["L", "Job", "Connections"],
  ["Y N S", "Proposed links", "Link, not related, decide later"],
  ["Enter, Shift Enter", "Build log search", "Next and previous match"],
];
