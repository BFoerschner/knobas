/**
 * The conversation's rules as values (#819, R-5.1 to R-5.3). The suite runs in
 * Europe/Berlin (`vite.config.ts`), so 07:00Z is 09:00 on the reader's clock.
 */
import { expect, test } from "vitest";

import type { Thread, ThreadComment } from "../../ipc/entity";
import { emptyFilter } from "../filter/filter";
import {
  MENTIONS,
  NEW,
  drawThread,
  isNew,
  mentionRuns,
  since,
  stamp,
  summary,
} from "./conversation";

const NOW = new Date(2026, 9, 15, 14, 0);
const VISIT = "2026-10-15T07:00:00Z";

function said(author: string, at: string | null, body: string, over: Partial<ThreadComment> = {}): ThreadComment {
  return { author, at, body, mine: false, mentions_me: false, markup: "text", ...over };
}

/** Eight old comments, then the reader's own and two new ones, one naming the reader. */
function thread(): Thread {
  const old = Array.from({ length: 8 }, (_, n) =>
    said(n % 2 ? "Priya Nair" : "Lena Hoffmann", `2026-08-${12 + n}T08:00:00Z`, `old ${n}`),
  );
  return {
    comments: [
      ...old,
      said("Mara Lindqvist", "2026-10-15T07:30:00Z", "mine, after the visit", { mine: true }),
      said("Jonas Becker", "2026-10-15T08:00:00Z", "left a note"),
      said("Lena Hoffmann", "2026-10-15T09:00:00Z", "[~mara] will it get an SLA?", { mentions_me: true, markup: "text" }),
    ],
    me: ["mara"],
  };
}

test("new is somebody else's comment after the last visit, and nothing is new on a first look", () => {
  const [mine, theirs] = thread().comments.slice(8);
  expect(isNew(theirs!, VISIT)).toBe(true);
  expect(isNew(mine!, VISIT)).toBe(false);
  expect(isNew(said("x", "2026-10-15T08:00:00Z", "y"), VISIT)).toBe(true);
  expect(isNew(said("x", "2026-10-15T06:00:00Z", "y"), VISIT)).toBe(false);
  expect(isNew(said("x", null, "y"), VISIT)).toBe(false);
  expect(isNew(said("x", "2026-10-15T08:00:00Z", "y"), null)).toBe(false);
});

test("the summary counts the comments", () => {
  const read = summary(thread(), VISIT);
  expect(read).toMatchObject({ comments: 11, fresh: 2, mentions: 1 });
  expect(read.latest?.body).toBe("[~mara] will it get an SLA?");
  expect(summary({ comments: [], me: [] }, VISIT)).toMatchObject({ comments: 0, fresh: 0, mentions: 0, latest: null });
});

test("unfiltered, the old part folds: the last six entries and every new comment stay", () => {
  const filter = emptyFilter();
  const drawn = drawThread(thread(), VISIT, filter, false);
  // 11 comments; the last six are kept.
  expect(drawn.folded).toBe(5);
  expect(drawn.foldedFrom).toBe("2026-08-12");
  expect(drawn.entries).toHaveLength(6);
  // The rule goes before the first new comment, not before the reader's own.
  const at = drawn.newAt!;
  expect(drawn.entries[at]).toMatchObject({ comment: { body: "left a note" } });

  const all = drawThread(thread(), VISIT, filter, true);
  expect(all.folded).toBe(0);
  expect(all.entries.map((entry) => entry.comment.body)).toEqual(thread().comments.map((comment) => comment.body));
});

test("a new comment older than the last six still stays above the fold", () => {
  const read = thread();
  read.comments.splice(1, 0, said("Tomasz", "2026-10-15T08:30:00Z", "late but new"));
  const drawn = drawThread(read, VISIT, emptyFilter(), false);
  expect(drawn.entries.map((entry) => entry.comment.body)).toContain("late but new");
});

test("a filter keeps the comments it matches, and folds nothing", () => {
  const mentions = emptyFilter();
  mentions.chips["only"] = [MENTIONS];
  const drawn = drawThread(thread(), VISIT, mentions, false);
  expect(drawn.entries).toHaveLength(1);
  expect(drawn.shown).toBe(1);
  expect(drawn.folded).toBe(0);
  expect(drawn.newAt).toBeNull();

  const fresh = emptyFilter();
  fresh.chips["only"] = [NEW];
  expect(drawThread(thread(), VISIT, fresh, false).shown).toBe(2);

  const words = emptyFilter();
  words.text = "old priya";
  expect(drawThread(thread(), VISIT, words, false).shown).toBe(4);

  const person = emptyFilter();
  person.chips["who"] = ["Jonas Becker"];
  expect(drawThread(thread(), VISIT, person, false).shown).toBe(1);
});

test("a mention is marked in both spellings, and only for the reader's account", () => {
  expect(mentionRuns("[~mara] will it get an SLA?", ["mara"])).toEqual([
    { text: "[~mara]", at: true },
    { text: " will it get an SLA?", at: false },
  ]);
  expect(mentionRuns("ask @mara, not @mara.lindqvist", ["mara"]).filter((run) => run.at)).toEqual([
    { text: "@mara", at: true },
  ]);
  expect(mentionRuns("no one", [])).toEqual([{ text: "no one", at: false }]);
});

test("times read as prototype-6 writes them", () => {
  expect(stamp("2026-10-15T07:35:00Z", NOW)).toBe("today 09:35");
  expect(stamp("2026-10-14T15:41:00Z", NOW)).toBe("yesterday 17:41");
  expect(stamp("2026-08-12T07:14:00Z", NOW)).toBe("2026-08-12 09:14");
  expect(stamp(null, NOW)).toBe("");
  expect(since(VISIT, NOW)).toBe("09:00");
  expect(since("2026-10-14T15:41:00Z", NOW)).toBe("yesterday 17:41");
});
