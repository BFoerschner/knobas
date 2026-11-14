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
 * (`ShareParts`). *Saved lists, favourites and keys* is drawn only once a
 * saved list, a favourite or a key exists (they ride in that part, #2428,
 * #2429),
 * *Ticket views* (#812) only once a view exists, *Link sets* (#870) only
 * once a set exists and *State colours* (#2469) only once a tone override
 * exists: each part is a box about nothing before that (CONTEXT.md, *Share
 * export*).
 */
export function shareTicks(savedLists: boolean, savedViews = false, savedSets = false, savedTones = false): ShareTick[] {
  const all: ShareTick[] = [
    { part: "links", label: "Links" },
    { part: "link_sets", label: "Link sets" },
    { part: "ticket_views", label: "Ticket views" },
    { part: "contexts", label: "Contexts" },
    { part: "assets", label: "Assets and routes" },
    { part: "sources", label: "Sources, without passwords" },
    { part: "smart_lists", label: "Saved lists, favourites and keys" },
    { part: "state_colours", label: "State colours" },
    { part: "notes", label: "Notes" },
    { part: "time", label: "Time" },
  ];
  return all.filter(
    (tick) =>
      (savedLists || tick.part !== "smart_lists") &&
      (savedViews || tick.part !== "ticket_views") &&
      (savedSets || tick.part !== "link_sets") &&
      (savedTones || tick.part !== "state_colours"),
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

/**
 * Why a number field holds something other than what was typed, naming both, since what stands in the field is a valid
 * value (F-613); `null` when it holds what was typed.
 */
export function clampNote(typed: string, min: number, max: number, value: number): string | null {
  const parsed = Number.parseInt(typed, 10);
  if (Number.isNaN(parsed)) return `Takes ${min} to ${max}, so this stays ${value}.`;
  return parsed >= min && parsed <= max ? null : `Takes ${min} to ${max}, so ${parsed} became ${value}.`;
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

/**
 * Keyboard: requirements §14, in prototype-6's words, and what has joined since (F-062, F-633). Written as the Mac
 * spells them, ⌘ for ⌘ or Ctrl; `keyHint` writes them as the platform does (the Windows pass, 2026-11-14).
 */
export const SHORTCUTS: readonly (readonly [keys: string, where: string, does: string])[] = [
  ["⌘K", "Anywhere", "Search, from an empty box"],
  ["⌘T", "Anywhere", "Stop or start the timer"],
  ["⌘,", "Anywhere", "Settings"],
  ["⌘[ ⌘] ⌘← ⌘→", "Anywhere", "Back and forward through what you opened; the mouse's buttons too"],
  ["⌘R", "A detail or a role view", "Refresh, or Sync now"],
  ["/ ⌘/", "Anywhere; / outside a field", "Search this section: the launcher with its filter"],
  ["⌘F", "Anywhere", "Find on this page: its find bar or filter box, else the section's search"],
  ["Esc", "Anywhere", "Back one step: dialog, drawer, filter, view, job"],
  ["⌘D", "Search box", "Titles only, or everything"],
  ["Tab", "Search box", "The actions on the selected result"],
  ["⌘S", "Search box", "Save the search as a list"],
  ["⌘?", "Search box", "The filters, and the keys it answers"],
  ["⌘B", "Search box, on an action", "Bind a key of your own to it"],
  ["⌘Enter", "A composer or a dialog", "Send, or press its primary button"],
  ["J K ↑ ↓", "Tickets and the Inbox", "Move the selection"],
  ["Enter", "Tickets", "Open the job"],
  ["L", "Job", "Connections"],
  ["J K", "A diff", "Next and previous file"],
  ["N P", "A diff", "Next and previous change"],
  ["V", "A diff", "Mark the file viewed"],
  ["C", "A diff", "Comment on the line"],
  ["S", "A diff", "Split or unified"],
  ["Enter, Shift Enter", "Build log search", "Next and previous match"],
];

/** Keys only the Mac has: history's arrows go with ⌘ alone (`historyStep`), which off the Mac is the Windows key's. */
export const MAC_ONLY: ReadonlySet<string> = new Set(["⌘←", "⌘→"]);
