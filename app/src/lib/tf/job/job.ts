/**
 * The job's pure half (#816): what the job card and the Code section say,
 * computed from what the reads answered, so the rules are testable as values.
 *
 * Nothing here reads a source's payload. The card's type, priority and
 * estimate arrive as `get_entity` fields the backend read through the
 * adapter's declarations (ADR-0007); the blocked chip reads the confirmed
 * links the detail already carries.
 */
import type { EntityDetail, EntityRow, Tone } from "../../ipc/entity";
import type { Week } from "../../ipc/time";

/** The key half of an entity id: `jira:PAY-231` reads `PAY-231`. */
export function keyOf(entityId: string): string {
  return entityId.slice(entityId.indexOf(":") + 1);
}

/**
 * The ticket a job address names, given how to read one entity.
 *
 * `#/job/<key>` carries the key a person reads (prototype-6's address), and
 * an entity id is `<source>:<key>`, so a bare key is looked for under every
 * configured source, in the order the sources are listed, and the first
 * ticket wins. An address that already holds an id (`jira:PAY-231`) is read
 * as one. `null` when nothing in the mirror answers to it.
 */
export async function resolveTicket(
  segment: string,
  sources: readonly { id: string }[],
  read: (entityId: string) => Promise<EntityDetail>,
): Promise<EntityDetail | null> {
  const candidates = segment.includes(":") ? [segment] : sources.map((source) => `${source.id}:${segment}`);
  const answers = await Promise.allSettled(candidates.map((id) => read(id)));
  for (const answer of answers) {
    if (answer.status === "fulfilled" && answer.value.row.kind === "ticket") return answer.value;
  }
  return null;
}

/**
 * The tickets blocking this one: the other end of every confirmed `blocks`
 * link this ticket is the **`to`** end of (the standup digest's reading of
 * the same word), withdrawn ones left out.
 */
export function blockers(detail: Pick<EntityDetail, "row" | "links">): string[] {
  return detail.links
    .filter(
      (entry) =>
        entry.link.relation === "blocks" &&
        entry.link.to_id === detail.row.entity_id &&
        entry.other.deleted_at === null,
    )
    .map((entry) => keyOf(entry.other.entity_id));
}

/**
 * A duration as prototype-6 writes one: `7h 30m`, `45m`, `3h`.
 *
 * Minutes, never decimal hours, and never days: how long a working day is
 * belongs to the source's settings, which knobas does not read.
 */
export function hm(seconds: number): string {
  const minutes = Math.round(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours === 0) return `${rest}m`;
  return rest === 0 ? `${hours}h` : `${hours}h ${rest}m`;
}

/** The card's facts line: `Story, High, est. 3h`, each part only where known. */
export function factsLine(detail: Pick<EntityDetail, "issue_type" | "priority" | "estimate_seconds">): string {
  const parts: string[] = [];
  if (detail.issue_type) parts.push(detail.issue_type);
  if (detail.priority) parts.push(detail.priority);
  if (detail.estimate_seconds !== null && detail.estimate_seconds > 0) {
    parts.push(`est. ${hm(detail.estimate_seconds)}`);
  }
  return parts.join(", ");
}

/** One weekday of *Time this week*. */
export interface WeekDay {
  logged: number;
  unlogged: number;
  today: boolean;
}

/** *Time this week* for one ticket: Monday to Friday and the two totals. */
export interface WeekReading {
  days: WeekDay[];
  logged: number;
  unlogged: number;
}

/**
 * What *Time this week* draws for `entityId`, out of `week_timesheet`.
 *
 * Logged is worklog time sent or waiting to send; *to log* is the tracked
 * time no worklog covers (`unlogged_seconds`), the week's own split. Held
 * worklogs are neither, as on the timesheet.
 */
export function weekReading(week: Week, entityId: string, today: string): WeekReading {
  const row = week.rows.find(
    (candidate) => candidate.target?.kind === "entity" && candidate.target.entity_id === entityId,
  );
  const days = week.days.slice(0, 5).map((day, index) => {
    const cell = row?.cells[index];
    return { logged: cell?.logged_seconds ?? 0, unlogged: cell?.unlogged_seconds ?? 0, today: day === today };
  });
  const sum = (field: "logged_seconds" | "unlogged_seconds") =>
    (row?.cells ?? []).reduce((total, cell) => total + cell[field], 0);
  return { days, logged: sum("logged_seconds"), unlogged: sum("unlogged_seconds") };
}

/** The line under *Time this week*. */
export function weekLine(reading: WeekReading): string {
  if (reading.logged === 0 && reading.unlogged === 0) return "Nothing yet";
  const parts: string[] = [];
  if (reading.logged > 0) parts.push(`${hm(reading.logged)} logged`);
  if (reading.unlogged > 0) parts.push(`${hm(reading.unlogged)} to log`);
  return parts.join(", ");
}

/** The kinds the Code section draws, in the words of R-4.7. */
export const CODE_KINDS = ["pr", "branch", "commit", "repo"] as const;

/** How loud a row's state is: what asks for you first (R-4.6). */
const RANK: Record<Tone, number> = { err: 0, warn: 1, info: 2, ok: 3, muted: 3 };

/**
 * A section's rows, sorted by what asks for you, then newest first (R-4.6).
 * A row with no state sorts with the settled ones; a row the source never
 * dated sorts last among its rank.
 */
export function byAttention(rows: readonly EntityRow[]): EntityRow[] {
  const rank = (row: EntityRow) => (row.state ? RANK[row.state.tone] : 3);
  return [...rows].sort(
    (a, b) => rank(a) - rank(b) || (b.updated_at ?? "").localeCompare(a.updated_at ?? ""),
  );
}

/** The repositories a set of code rows belongs to, as the mirror names them. */
export function reposOf(rows: readonly EntityRow[]): string[] {
  const names = rows.map((row) => (row.kind === "repo" ? row.title : row.path)).filter((name): name is string => !!name);
  return [...new Set(names)];
}

/** How many open pull requests: a pull request whose state says *open*. */
export function openPrs(rows: readonly EntityRow[]): number {
  return rows.filter((row) => row.kind === "pr" && row.state?.text.toLowerCase() === "open").length;
}

/**
 * The full view a row opens (R-4.10): the item's own address in the new
 * shell, never a panel. The address carries the entity id, because a pull
 * request's number is unique only inside its repository. A kind with no view
 * of its own in prototype-6 opens Code.
 */
export function itemHash(row: Pick<EntityRow, "entity_id" | "kind">): string {
  const id = encodeURIComponent(row.entity_id);
  switch (row.kind) {
    case "pr":
      return `#/pr/${id}`;
    case "repo":
      return `#/repo/${id}`;
    case "branch":
      return `#/branch/${id}`;
    case "commit":
      return `#/commit/${id}`;
    case "ticket":
      return `#/job/${encodeURIComponent(keyOf(row.entity_id))}`;
    default:
      return "#/code";
  }
}

/** Whether `row` matches a section's filter text, ignoring case. */
export function matches(row: EntityRow, text: string): boolean {
  const needle = text.trim().toLowerCase();
  if (needle === "") return true;
  return [row.title, row.path ?? "", row.state?.text ?? ""].some((field) => field.toLowerCase().includes(needle));
}
