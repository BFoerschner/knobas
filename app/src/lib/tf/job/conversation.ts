/**
 * The job's conversation, pure (#819, R-5.1 to R-5.3): the card's summary line
 * and the Conversation tab's entries, computed from what `entity_thread` and
 * `visit_entity` answered, so the rules are testable as values.
 *
 * **New** is a comment somebody else wrote after the reader's last visit. A
 * comment the source did not date is never new, and with no earlier visit
 * nothing is: a first look has nothing to compare against. The conversation
 * is the ticket's comments and nothing else (#2163): no status-move or
 * pull-request lines.
 */
import type { Thread, ThreadComment } from "../../ipc/entity";
import { isFiltering, matchesAll, type FilterState } from "../filter/filter";
import { dayKey } from "../../time/day";
import { stampLabel } from "../../time/format";

/** How many entries an unfiltered thread keeps above its fold (R-5.3). */
export const KEEP = 6;

/** The *Show* chips' values. Their labels are the view's. */
export const MENTIONS = "mentions";
export const NEW = "new";

/** Whether `comment` is new to the reader: somebody else's, after `lastVisit`. */
export function isNew(comment: ThreadComment, lastVisit: string | null): boolean {
  if (lastVisit === null || comment.at === null || comment.mine) return false;
  return Date.parse(comment.at) > Date.parse(lastVisit);
}

/** The card's conversation summary (R-5.1). */
export interface Summary {
  comments: number;
  fresh: number;
  mentions: number;
  latest: ThreadComment | null;
}

export function summary(thread: Thread, lastVisit: string | null): Summary {
  return {
    comments: thread.comments.length,
    fresh: thread.comments.filter((comment) => isNew(comment, lastVisit)).length,
    mentions: thread.comments.filter((comment) => comment.mentions_me).length,
    latest: thread.comments[thread.comments.length - 1] ?? null,
  };
}

/** `"20 comments"`, `"1 comment"`. */
export function commentCount(n: number): string {
  return n === 1 ? "1 comment" : `${n} comments`;
}

/** `"2 mention you"`, `"1 mentions you"`. */
export function mentionCount(n: number): string {
  return n === 1 ? "1 mentions you" : `${n} mention you`;
}

/**
 * When an entry was written, as prototype-6 writes it: *today 09:35*,
 * *yesterday 17:41*, else *2026-08-12 09:14* (`stampLabel`). Empty for no time.
 */
export function stamp(iso: string | null, now: Date): string {
  return iso === null ? "" : stampLabel(iso, now);
}

/**
 * The last visit as the chip and the rule say it: *09:00* today, else
 * *yesterday 17:41* or *2026-08-12 09:14*.
 */
export function since(lastVisit: string, now: Date): string {
  return stampLabel(lastVisit, now, true);
}

/** The name a comment is signed with: the first word of a display name. */
export function firstName(author: string | null): string {
  if (author === null) return "Somebody";
  return author.trim().split(/\s+/)[0] ?? author;
}

/** One line of the thread. */
export interface Entry {
  comment: ThreadComment;
  fresh: boolean;
}

/** What the tab draws. */
export interface Drawn {
  entries: Entry[];
  /** How many old entries sit behind the fold, `0` for none. */
  folded: number;
  /** The date of the first folded entry, for "Show 17 earlier entries, from …". */
  foldedFrom: string | null;
  /** The index in `entries` the *New since your last visit* rule goes before, or `null`. */
  newAt: number | null;
  /** How many comments the filter keeps. */
  shown: number;
}

/**
 * The tab's entries under `filter` (R-5.2, R-5.3).
 *
 * Words must all appear in a comment's text or author; person chips keep any of
 * the people on; *Mentions me* and *New since…* keep only those comments.
 *
 * Unfiltered and not expanded, the old part folds: the last {@link KEEP}
 * entries and every new one stay.
 */
export function drawThread(
  thread: Thread,
  lastVisit: string | null,
  filter: FilterState,
  expanded: boolean,
): Drawn {
  const people = filter.chips["who"] ?? [];
  const only = filter.chips["only"] ?? [];
  const narrowing = isFiltering(filter);
  let entries = thread.comments.map((comment) => ({ comment, fresh: isNew(comment, lastVisit) })).filter((entry) => {
    const comment = entry.comment;
    if (!matchesAll(`${comment.body} ${comment.author ?? ""}`, filter.text)) return false;
    if (people.length > 0 && !people.includes(comment.author ?? "")) return false;
    if (only.includes(MENTIONS) && !comment.mentions_me) return false;
    if (only.includes(NEW) && !entry.fresh) return false;
    return true;
  });
  const shown = entries.length;

  let folded = 0;
  let foldedFrom: string | null = null;
  if (!narrowing && !expanded) {
    const keep = entries.filter((entry, index) => index >= entries.length - KEEP || entry.fresh);
    folded = entries.length - keep.length;
    if (folded > 0) {
      const first = entries.find((entry) => !keep.includes(entry));
      const at = first?.comment.at ?? null;
      foldedFrom = at === null ? null : dayKey(new Date(at));
    }
    entries = keep;
  }
  const firstNew = narrowing ? -1 : entries.findIndex((entry) => entry.fresh);
  return { entries, folded, foldedFrom, newAt: firstNew === -1 ? null : firstNew, shown };
}

/** A run of a comment's body, and whether it names the reader. */
export interface Run {
  text: string;
  at: boolean;
}

/**
 * `body` cut into runs, each `@name` or `[~name]` of an account in `me` marked
 * (R-5.3's `@name` in blue), by the backend's rule: not followed by another
 * name character (the inbox's ASCII `[A-Za-z0-9_]`), `.` or `-`.
 */
export function mentionRuns(body: string, me: readonly string[]): Run[] {
  const names = me.filter((name) => name !== "").map((name) => name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  if (names.length === 0) return [{ text: body, at: false }];
  const pattern = new RegExp(`(@|\\[~)(?:${names.join("|")})(?![A-Za-z0-9_.-])\\]?`, "g");
  const runs: Run[] = [];
  let last = 0;
  for (const match of body.matchAll(pattern)) {
    if (match.index > last) runs.push({ text: body.slice(last, match.index), at: false });
    runs.push({ text: match[0], at: true });
    last = match.index + match[0].length;
  }
  if (last < body.length) runs.push({ text: body.slice(last), at: false });
  return runs.length === 0 ? [{ text: body, at: false }] : runs;
}
