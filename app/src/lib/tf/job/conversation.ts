/**
 * The ticket page's conversation, pure (#819, R-5.1 to R-5.3, #2399): the
 * whole thread the reading column draws, oldest first and with no fold,
 * computed from what `entity_thread` and `visit_entity` answered, so the
 * rules are testable as values.
 *
 * **New** is a comment somebody else wrote after the reader's last visit. A
 * comment the source did not date is never new, and with no earlier visit
 * nothing is: a first look has nothing to compare against. The conversation
 * is the ticket's comments and nothing else (#2163): no status-move or
 * pull-request lines.
 */
import type { Thread, ThreadComment } from "../../ipc/entity";
import { stampLabel } from "../../time/format";

/** Whether `comment` is new to the reader: somebody else's, after `lastVisit`. */
export function isNew(comment: ThreadComment, lastVisit: string | null): boolean {
  if (lastVisit === null || comment.at === null || comment.mine) return false;
  return Date.parse(comment.at) > Date.parse(lastVisit);
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

/** What the reading column draws. */
export interface Drawn {
  entries: Entry[];
  /** The index in `entries` the *New since your last visit* rule goes before, or `null`. */
  newAt: number | null;
}

/**
 * The whole thread, oldest first, with no fold and no filter (#2399): the
 * rule goes before the first comment new to the reader, wherever it falls.
 */
export function wholeThread(thread: Thread, lastVisit: string | null): Drawn {
  const entries: Entry[] = thread.comments.map((comment) => ({ comment, fresh: isNew(comment, lastVisit) }));
  const firstNew = entries.findIndex((entry) => entry.fresh);
  return { entries, newAt: firstNew === -1 ? null : firstNew };
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
