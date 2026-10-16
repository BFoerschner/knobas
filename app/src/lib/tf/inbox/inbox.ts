/**
 * The Inbox's pure half (#834, R-12.1 to R-12.3): what a row says, where it
 * opens, what it offers, which job it belongs to, and which rows the filter
 * keeps.
 *
 * A row is an `inbox_items` entry. The view holds both shelves whole (the
 * inbox store reads them), so the filter runs in the browser (R-2.3's first
 * mode). Nothing here reads a payload: the sentence is the backend's own
 * `reason` and `title`.
 */
import type { InboxCategory, InboxEntry, InboxItem, Reached } from "../../ipc/entity";
import { matchesAll } from "../filter/filter";
import { itemHash, keyOf } from "../job/job";

/** The word each category reads as: the kind chips' and the second line's. */
const KIND: Record<InboxCategory, string> = {
  review_request: "Review",
  mention: "Mention",
  failed_build: "Build",
  new_assignment: "Assignment",
  alert: "Monitor",
  credential_expiry: "Credential",
};

/** The kind chips, in prototype-6's order. A credential expiry has none. */
export const KIND_CHIPS = ["Review", "Mention", "Build", "Assignment", "Monitor"] as const;

export function kindOf(category: InboxCategory): string {
  return KIND[category] ?? category;
}

/**
 * The item as one phrase, lower case as the backend writes it: the reason,
 * then the item after a colon, unless the reason already names it (a
 * credential expiry's reason names its source).
 */
export function phrase(item: Pick<InboxItem, "reason" | "title">): string {
  const reason = item.reason.trim();
  const title = item.title.trim();
  if (title === "" || reason.toLowerCase().includes(title.toLowerCase())) return reason;
  return `${reason}: ${title}`;
}

/**
 * Categories whose reason starts with a source's word: the reviewer's account
 * (`jonas asked for your review`) or the asset's path. Those are drawn as the
 * source spells them; only the backend's own opening words are capitalised.
 */
const SOURCE_FIRST: readonly InboxCategory[] = ["review_request", "alert"];

/** The row's sentence: {@link phrase}, capitalised where the first word is knobas' own. */
export function sentence(item: Pick<InboxItem, "category" | "reason" | "title">): string {
  const text = phrase(item);
  if (SOURCE_FIRST.includes(item.category)) return text;
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** A failure is drawn with an err dot (a failed build, a monitor down). */
export function failed(item: Pick<InboxItem, "category">): boolean {
  return item.category === "failed_build" || item.category === "alert";
}

const pad = (n: number) => String(n).padStart(2, "0");
const clock = (at: Date) => `${pad(at.getHours())}:${pad(at.getMinutes())}`;
const day = (at: Date) => `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;

/** The time column: the clock today, `yest. 16:30` yesterday, else the date. */
export function when(iso: string, now: Date): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "";
  if (day(at) === day(now)) return clock(at);
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
  if (day(at) === day(yesterday)) return `yest. ${clock(at)}`;
  return day(at);
}

/** Where a snoozed item comes back, for its toast: the weekday within a week, else the date. */
export function untilLabel(at: Date, now: Date): string {
  const days = (at.getTime() - now.getTime()) / 86_400_000;
  const name = days < 7 ? at.toLocaleDateString("en-GB", { weekday: "long" }) : day(at);
  return `${name} ${clock(at)}`;
}

/** The link a row opens with: its word and its address. */
export interface Opener {
  label: string;
  href: string;
}

/**
 * The row's first action, a link to the item's own view (R-4.10's
 * `itemHash`), worded by what it opens as prototype-6 words it.
 */
export function opener(item: Pick<InboxItem, "category" | "entity_id" | "kind" | "source_id">): Opener | null {
  if (item.category === "credential_expiry") {
    return { label: "Open source", href: `#/settings/source/${encodeURIComponent(item.source_id)}` };
  }
  if (item.category === "alert") return { label: "Open monitor", href: "#/monitors" };
  if (item.entity_id === null) return null;
  const kind = item.kind ?? "";
  const href = itemHash({ entity_id: item.entity_id, kind });
  switch (item.category) {
    case "review_request":
      return { label: "Review", href };
    case "failed_build":
      return { label: "Open log", href };
    case "new_assignment":
      return { label: "Open job", href };
    default:
      break;
  }
  if (kind === "ticket") return { label: "Open conversation", href: `${href}/conversation` };
  if (kind === "pr") return { label: "Open PR", href };
  if (kind === "page") return { label: "Open page", href };
  return { label: "Open", href };
}

/**
 * The source ops a row draws inline. A re-run is one press and harmless to
 * repeat; *Approve* and *Comment* want the pull request read first, so the
 * row offers *Review* instead, as prototype-6 does (a ruling of #834).
 */
const INLINE_OPS = ["rerun_build"];

/** A row's buttons after its link, in order: source ops, then knobas' own. */
export function rowActions(entry: InboxEntry): string[] {
  const ops = entry.actions.filter((op) => INLINE_OPS.includes(op));
  return [...ops, ...(entry.item.category === "alert" ? ["ack"] : []), "snooze", "done"];
}

/** One job and what it reaches: a ticket entity id and its `job_reach` read. */
export interface JobReach {
  id: string;
  reached: readonly Pick<Reached, "entity_id">[];
}

/** Which job an item belongs to, and whether a job holds it. */
export interface JobIndex {
  /** The job key the row's second line names, or `null` for *no job*. */
  jobOf(item: Pick<InboxItem, "entity_id" | "kind">): string | null;
  /** Whether the job with this key holds the item: its own ticket, or what it reaches. */
  holds(job: string, item: Pick<InboxItem, "entity_id">): boolean;
}

/**
 * The jobs' reaches, inverted. An item about a ticket belongs to that ticket;
 * anything else to the first job (in the order given) whose reach holds it.
 */
export function jobIndex(jobs: readonly JobReach[]): JobIndex {
  const owner = new Map<string, string>();
  const held = new Map<string, Set<string>>();
  for (const job of jobs) {
    const key = keyOf(job.id);
    const ids = new Set([job.id, ...job.reached.map((row) => row.entity_id)]);
    if (!held.has(key)) held.set(key, ids);
    for (const id of ids) if (!owner.has(id)) owner.set(id, key);
  }
  return {
    jobOf(item) {
      if (item.entity_id === null) return null;
      if (item.kind === "ticket") return keyOf(item.entity_id);
      return owner.get(item.entity_id) ?? null;
    },
    holds(job, item) {
      return item.entity_id !== null && (held.get(job)?.has(item.entity_id) ?? false);
    },
  };
}

/** What the filter bar has set. */
export interface InboxFilter {
  scope: "job" | "all";
  /** The job's ticket key; *This job* keeps nothing without one. */
  job: string | null;
  /** The kind chips on; none keeps every kind. */
  kinds: readonly string[];
  text: string;
}

/** The rows the filter keeps, in the stream's order (newest first). */
export function visible(entries: readonly InboxEntry[], filter: InboxFilter, index: JobIndex): InboxEntry[] {
  return entries.filter(({ item }) => {
    if (filter.scope === "job" && (filter.job === null || !index.holds(filter.job, item))) return false;
    if (filter.kinds.length > 0 && !filter.kinds.includes(kindOf(item.category))) return false;
    return matchesAll(`${sentence(item)} ${index.jobOf(item) ?? ""}`, filter.text);
  });
}

/** The snoozed shelf as one line (R-12.3), or `null` when nothing is snoozed. */
export function snoozedLine(entries: readonly InboxEntry[]): string | null {
  if (entries.length === 0) return null;
  const parts = entries.map(({ item }) => {
    const back = item.snoozed_until ? new Date(item.snoozed_until) : null;
    const date = back && !Number.isNaN(back.getTime()) ? ` (back ${day(back)})` : "";
    return `${phrase(item)}${date}`;
  });
  return `Snoozed: ${parts.join(", ")}.`;
}
