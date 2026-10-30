/**
 * The reader's pending line comments (#709): kept per entity, in the detail
 * and in this viewer's browser storage, so a reopen -- or a restart -- does
 * not lose an unsent comment. Storage that is missing or throws costs the
 * memory across restarts, never the comments in hand.
 */
import { expect, test } from "vitest";

import { asReview, createPendingComments } from "./pending-comments.svelte";

const PR = "gitea:tidewater/payout-service#142";
const OTHER = "gitea:tidewater/payout-service#7";

/** A working `Storage`, whose contents the test can read. */
function memoryStorage(): Storage & { map: Map<string, string> } {
  const map = new Map<string, string>();
  return {
    map,
    get length() {
      return map.size;
    },
    clear: () => map.clear(),
    getItem: (key) => map.get(key) ?? null,
    key: (index) => [...map.keys()][index] ?? null,
    removeItem: (key) => void map.delete(key),
    setItem: (key, value) => void map.set(key, value),
  };
}

test("a comment is written, edited and discarded, each entity its own", () => {
  const drafts = createPendingComments(() => null);
  const first = drafts.add(PR, { path: "src/retry.rs", side: "new", line: 2, body: "Bound the jitter." })!;
  drafts.add(PR, { path: "src/retry.rs", side: "old", line: 2, body: "Why drop it?" });
  drafts.add(OTHER, { path: "README.md", side: "new", line: 1, body: "typo" });
  expect(drafts.of(PR).map((d) => d.body)).toEqual(["Bound the jitter.", "Why drop it?"]);

  drafts.edit(PR, first, "Bound the jitter to ±10 %.");
  expect(drafts.of(PR)[0]).toMatchObject({ side: "new", line: 2, body: "Bound the jitter to ±10 %." });

  drafts.discard(PR, first);
  expect(drafts.of(PR).map((d) => d.body)).toEqual(["Why drop it?"]);
  drafts.clear(PR);
  expect(drafts.of(PR)).toEqual([]);
  expect(drafts.of(OTHER)).toHaveLength(1);
});

test("the comments survive a new instance through storage, as the review sends them", () => {
  const storage = memoryStorage();
  const before = createPendingComments(() => storage);
  before.add(PR, { path: "src/retry.rs", side: "old", line: 4, body: "why?" });
  // The control: the comment is in storage, so the read below is a read of it.
  expect([...storage.map.values()].join()).toContain("why?");

  const after = createPendingComments(() => storage);
  expect(asReview(after.of(PR))).toEqual([{ path: "src/retry.rs", side: "old", line: 4, body: "why?" }]);

  after.clear(PR);
  expect(createPendingComments(() => storage).of(PR)).toEqual([]);
});

test("storage that throws or holds rubbish costs only the memory across restarts", () => {
  const throwing = {
    getItem: () => {
      throw new Error("blocked");
    },
    setItem: () => {
      throw new Error("blocked");
    },
  } as unknown as Storage;
  const drafts = createPendingComments(() => throwing);
  drafts.add(PR, { path: "a", side: "new", line: 1, body: "kept in hand" });
  expect(drafts.of(PR)).toHaveLength(1);

  const storage = memoryStorage();
  storage.setItem("knobas.pending-comments", "{not json");
  expect(createPendingComments(() => storage).of(PR)).toEqual([]);
  storage.setItem("knobas.pending-comments", JSON.stringify({ [PR]: [{ path: 3 }, "x"] }));
  expect(createPendingComments(() => storage).of(PR)).toEqual([]);
});

test("a blank comment is not one", () => {
  const drafts = createPendingComments(() => null);
  const id = drafts.add(PR, { path: "a", side: "new", line: 1, body: "   " });
  expect(id).toBeNull();
  expect(drafts.of(PR)).toEqual([]);
});

test("a review settles only what it sent as sent: one edited since stays, with its new words", () => {
  const drafts = createPendingComments(() => null);
  drafts.add(PR, { path: "a", side: "new", line: 1, body: "sent as is" });
  const edited = drafts.add(PR, { path: "a", side: "new", line: 2, body: "first words" })!;
  const sent = drafts.of(PR);
  drafts.edit(PR, edited, "second words");
  drafts.add(PR, { path: "a", side: "old", line: 3, body: "written meanwhile" });

  drafts.settle(PR, sent);
  expect(drafts.of(PR).map((d) => d.body)).toEqual(["second words", "written meanwhile"]);
});

test("#2276: two comments and the review's words survive a restart, and submitting clears them", () => {
  const storage = memoryStorage();
  const before = createPendingComments(() => storage);
  before.seen(PR, "c90d11");
  before.add(PR, { path: "src/retry.rs", side: "new", line: 27, body: "Is a sub-second retry intended?" });
  before.add(PR, { path: "src/retry.rs", side: "old", line: 28, body: "Why 3?" });
  before.setBody(PR, "Close, two questions.");

  // The app again: a new store over the same storage.
  const after = createPendingComments(() => storage);
  expect(after.of(PR).map((d) => d.body)).toEqual(["Is a sub-second retry intended?", "Why 3?"]);
  expect(after.body(PR)).toBe("Close, two questions.");

  // Submitted: what it sent goes, the words with it.
  after.settle(PR, after.of(PR), "Close, two questions.");
  expect(after.of(PR)).toEqual([]);
  expect(after.body(PR)).toBe("");
  const again = createPendingComments(() => storage);
  expect([again.of(PR), again.body(PR)]).toEqual([[], ""]);
});

test("#2276: comments written on an earlier head are kept apart, and are not the review's", () => {
  const drafts = createPendingComments(() => null);
  drafts.seen(PR, "c90d11");
  drafts.add(PR, { path: "a", side: "new", line: 1, body: "on the first head" });
  drafts.seen(PR, "c90d11");
  expect(drafts.of(PR)).toHaveLength(1);

  // The head moved: the comment is no longer on the lines drawn.
  drafts.seen(PR, "e41a07");
  expect(drafts.of(PR)).toEqual([]);
  expect(drafts.stale(PR).map((d) => [d.body, d.head])).toEqual([["on the first head", "c90d11"]]);
  drafts.add(PR, { path: "a", side: "new", line: 2, body: "on the second" });
  expect(drafts.of(PR).map((d) => d.body)).toEqual(["on the second"]);
});

test("#2276: words edited after the review was sent stay as the next review's", () => {
  const drafts = createPendingComments(() => null);
  drafts.setBody(PR, "sent");
  drafts.setBody(PR, "changed since");
  drafts.settle(PR, [], "sent");
  expect(drafts.body(PR)).toBe("changed since");
});
