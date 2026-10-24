/**
 * The job's conversation, pure (#819, R-5.1 to R-5.3): the card's summary line
 * and the Conversation tab's entries, computed from what `entity_thread` and
 * `visit_entity` answered, so the rules are testable as values.
 *
 * **New** is a comment somebody else wrote after the reader's last visit. A
 * comment the source did not date is never new, and with no earlier visit
 * nothing is: a first look has nothing to compare against. **Status changes
 * are never comments** (R-5.1): they are entries of their own, counted
 * nowhere and drawn only while *Include changes* is on.
 */
import type { Thread, ThreadChange, ThreadComment } from "../../ipc/entity";
import { isFiltering, matchesAll, type FilterState } from "../filter/filter";
import { dayKey } from "../../time/day";
import { stampLabel } from "../../time/format";

/** How many entries an unfiltered thread keeps above its fold (R-5.3). */
export const KEEP = 6;

/** The *Show* chips' values. Their labels are the view's. */
export const MENTIONS = "mentions";
export const NEW = "new";
export const CHANGES = "changes";

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

/**
 * A change as its quiet line says it (#929): who moved the ticket, from what
 * to what, or a linked pull request's opening or merge, named by number and
 * repository as far as the record knows them.
 */
export function changeLine(change: ThreadChange): string {
  if (change.kind === "status") {
    const who = change.mine ? "You" : firstName(change.by);
    const from = change.from === null ? "" : ` from ${change.from}`;
    return `${who} moved this${from} to ${change.status ?? "another status"}`;
  }
  const pr = change.number === null ? "A pull request" : `PR #${change.number}`;
  const repo = change.repo === null ? "" : ` in ${change.repo}`;
  return change.kind === "opened" ? `${pr} opened${repo}` : `${pr}${repo} merged`;
}

/** One line of the thread. */
export type Entry =
  | { kind: "comment"; comment: ThreadComment; fresh: boolean }
  | { kind: "change"; change: ThreadChange };

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

/** Comments and changes in one time line; a comment with no time keeps its place. */
function interleave(thread: Thread, lastVisit: string | null): Entry[] {
  const entries: Entry[] = [];
  const changes = [...thread.changes];
  for (const comment of thread.comments) {
    while (comment.at !== null && changes.length > 0 && Date.parse(changes[0]!.at) <= Date.parse(comment.at)) {
      entries.push({ kind: "change", change: changes.shift()! });
    }
    entries.push({ kind: "comment", comment, fresh: isNew(comment, lastVisit) });
  }
  for (const change of changes) entries.push({ kind: "change", change });
  return entries;
}

/**
 * The tab's entries under `filter` (R-5.2, R-5.3).
 *
 * Words must all appear in a comment's text or author; person chips keep any of
 * the people on; *Mentions me* and *New since…* keep only those comments. A
 * change shows only with *Include changes* on and nothing else filtering, as
 * in prototype-6: a filtered thread is a search over what people wrote.
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
  const narrowing = narrows(filter);
  let entries = interleave(thread, lastVisit).filter((entry) => {
    if (entry.kind === "change") return only.includes(CHANGES) && !narrowing;
    const comment = entry.comment;
    if (!matchesAll(`${comment.body} ${comment.author ?? ""}`, filter.text)) return false;
    if (people.length > 0 && !people.includes(comment.author ?? "")) return false;
    if (only.includes(MENTIONS) && !comment.mentions_me) return false;
    if (only.includes(NEW) && !entry.fresh) return false;
    return true;
  });
  const shown = entries.filter((entry) => entry.kind === "comment").length;

  let folded = 0;
  let foldedFrom: string | null = null;
  if (!narrowing && !expanded) {
    const keep = entries.filter((entry, index) => index >= entries.length - KEEP || (entry.kind === "comment" && entry.fresh));
    folded = entries.length - keep.length;
    if (folded > 0) {
      const first = entries.find((entry) => !keep.includes(entry));
      const at = first?.kind === "comment" ? first.comment.at : (first?.change.at ?? null);
      foldedFrom = at === null ? null : dayKey(new Date(at));
    }
    entries = keep;
  }
  const firstNew = narrowing ? -1 : entries.findIndex((entry) => entry.kind === "comment" && entry.fresh);
  return { entries, folded, foldedFrom, newAt: firstNew === -1 ? null : firstNew, shown };
}

/** Whether the filter narrows the thread beyond *Include changes*. */
export function narrows(filter: FilterState): boolean {
  const only = filter.chips["only"] ?? [];
  return isFiltering({ ...filter, chips: { ...filter.chips, only: only.filter((value) => value !== CHANGES) } });
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
