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
import type { InboxCategory, InboxEntry, InboxItem } from "../../ipc/entity";
import { matchesAll } from "../filter/filter";
import { itemHash } from "../job/job";
import type { JobIndex } from "../job/open-jobs";
import { ticketHash } from "../route";

/** The word each category reads as: the kind chips' and the second line's. */
const KIND: Record<InboxCategory, string> = {
  review_request: "Review",
  mention: "Mention",
  failed_build: "Build",
  new_assignment: "Assignment",
  alert: "Monitor",
  credential_expiry: "Credential",
  failed_sign_in: "Sign-in",
};

/**
 * The kind chips, in prototype-6's order. A credential expiry has none, and
 * neither has a failed sign-in (#1571): both are about a source, not work.
 */
export const KIND_CHIPS = ["Review", "Mention", "Build", "Assignment", "Monitor"] as const;

/**
 * How many days before a credential expires its item arrives:
 * `knobas_core::inbox::WINDOW_DAYS`, which a Rust test in
 * `crates/knobas-app/src/inbox.rs` holds this line to.
 */
export const WINDOW_DAYS = 14;

/**
 * When a credential expiry's credential expires, for the *After it expires*
 * snooze (#1086); `null` for every other item. The date is not on the wire:
 * the backend dates the item at `secret_expires_at` minus [`WINDOW_DAYS`], so
 * it is re-derived as `occurred_at` plus the same window.
 */
export function expiryOf(item: Pick<InboxItem, "category" | "occurred_at">): Date | null {
  if (item.category !== "credential_expiry") return null;
  return new Date(new Date(item.occurred_at).getTime() + WINDOW_DAYS * 86_400_000);
}

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

/** The backend's stand-in for a review request with no author (`knobas_core::inbox`): knobas' own word. */
const NO_AUTHOR = "somebody ";

/** The row's sentence: {@link phrase}, capitalised where the first word is knobas' own. */
export function sentence(item: Pick<InboxItem, "category" | "reason" | "title">): string {
  const text = phrase(item);
  if (SOURCE_FIRST.includes(item.category) && !text.startsWith(NO_AUTHOR)) return text;
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** A failure is drawn with an err dot (a failed build, a monitor down, a refused sign-in). */
export function failed(item: Pick<InboxItem, "category">): boolean {
  return item.category === "failed_build" || item.category === "alert" || item.category === "failed_sign_in";
}

const pad = (n: number) => String(n).padStart(2, "0");
const clock = (at: Date) => `${pad(at.getHours())}:${pad(at.getMinutes())}`;
const day = (at: Date) => `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;

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
 * `itemHash`), worded by what it opens as prototype-6 words it. A ticket
 * assigned to the reader opens its ticket page (spec #2227), to read before
 * taking it on; a mention on a ticket opens the job's Conversation, the one
 * place the thread is drawn, so it builds the job's address on purpose.
 */
export function opener(item: Pick<InboxItem, "category" | "entity_id" | "kind" | "source_id">): Opener | null {
  // Both are about the source itself, whose page holds *Re-enter* (#1571).
  if (item.category === "credential_expiry" || item.category === "failed_sign_in") {
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
      return { label: kind === "ticket" ? "Open ticket" : "Open", href };
    default:
      break;
  }
  if (kind === "ticket") return { label: "Open conversation", href: ticketHash(item.entity_id) };
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
