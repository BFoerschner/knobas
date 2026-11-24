/** A folder's generated compare (ux26, rulings 7 and 8): a lane a side, only a press runs, one compare per pair of tokens. */
import { flushSync } from "svelte";
import { expect, test, vi } from "vitest";

import type { FolderRenderer } from "../ipc/app";
import { FolderCompare, laneOf, type FolderCompareAt } from "./folder-compare.svelte";
import { fakeCompareBackend } from "./folder-compare.test-support";

const SHA = "4f2a91c0d2b9e8a7f6c5d4b3a2918f7e6d5c4b3a";
const WT = "~working-tree";
const SITE: FolderRenderer = { id: "site", name: "Low-code site", command: "lowcode {path} --out {outdir}", holds: [], folders: ["docs"], output: "folder", timeout_secs: 30 };

async function settle() {
  for (let i = 0; i < 10; i++) await Promise.resolve();
  flushSync();
}

function harness(more: Partial<FolderCompareAt> = {}, kept: (fake: ReturnType<typeof fakeCompareBackend>) => void = () => {}) {
  const fake = fakeCompareBackend();
  kept(fake);
  const at: FolderCompareAt = $state({ entityId: "repo", renderer: SITE, folder: "docs", before: SHA, after: WT, ...more });
  let compare!: FolderCompare;
  const stop = $effect.root(() => {
    compare = new FolderCompare(() => at, fake.ports);
  });
  flushSync();
  return { fake, at, compare: () => compare, stop };
}

test("nothing but a look until a press, each side in its own lane", async () => {
  const h = harness();
  await settle();
  expect(h.fake.asked.map((a) => [a.ask, a.lane])).toEqual([
    ["look", laneOf(SHA)],
    ["look", laneOf(WT)],
  ]);
  expect(h.compare().state).toBe("idle");
  h.compare().press();
  await settle();
  expect(h.fake.asked.slice(2).map((a) => [a.ask, a.lane])).toEqual([
    ["again", laneOf(SHA)],
    ["again", laneOf(WT)],
  ]);
  expect(h.compare().state).toBe("running");
  h.stop();
});

test("a press runs only the side that is not rendered, and stops both while they run", async () => {
  vi.useFakeTimers();
  const h = harness({}, (fake) => fake.keep(laneOf(SHA), "base"));
  await settle();
  expect(h.compare().sideOf("old")).toBe("kept");
  h.compare().press();
  await settle();
  expect(h.fake.asked.filter((a) => a.ask === "again").map((a) => a.lane)).toEqual([laneOf(WT)]);
  // A press holds for its two seconds (the press rule): *Stop* after them.
  vi.advanceTimersByTime(2100);
  h.compare().press();
  await settle();
  expect(h.fake.stopped).toEqual([laneOf(WT)]);
  h.stop();
  vi.useRealTimers();
});

test("the compare is asked once both are drawn, once per pair of tokens, line endings aside at the working tree", async () => {
  const h = harness();
  await settle();
  h.compare().press();
  await settle();
  h.fake.land(laneOf(SHA), "a");
  await settle();
  expect(h.fake.compared, "not before both sides are drawn").toEqual([]);
  expect(h.compare().state).toBe("running");
  h.fake.land(laneOf(WT), "b");
  await settle();
  expect(h.fake.compared).toEqual([["a", "b", true]]);
  expect(h.compare().state).toBe("same");
  // A look that answers the same tokens asks nothing again.
  h.compare().before.look();
  await settle();
  expect(h.fake.compared.length).toBe(1);
  h.stop();
});

test("what differs is made; a side with no folder lists every file of the other as added", async () => {
  const h = harness({ before: null, after: SHA }, (fake) => {
    fake.answers.set(`null\nz`, { files: [{ path: "index.md", change: "added", before_size: null, after_size: 10, binary: false }], same: 0, eol_only: 0, before_files: 0, after_files: 1 });
    fake.keep(laneOf(SHA), "z");
  });
  await settle();
  expect(h.compare().sideOf("old")).toBe("absent");
  expect(h.fake.compared).toEqual([[null, "z", false]]);
  expect(h.compare().state).toBe("made");
  expect(h.compare().lists).toBe(true);
  h.stop();
});

test("a side made from an earlier state is never compared: it is held, and Run runs it alone; a failed side fails the compare", async () => {
  // Björn, 2026-11-24: one side made by an older version of the tool was compared with one made by the new version.
  const h = harness({}, (fake) => {
    fake.keep(laneOf(SHA), "a");
    fake.keep(laneOf(WT), "old-wt", false);
  });
  await settle();
  expect(h.compare().state).toBe("stale");
  expect(h.compare().runnable).toBe(true);
  expect(h.compare().wants).toBe(true);
  expect(h.fake.compared, "what two tools made is no diff").toEqual([]);
  expect(h.compare().made).toBeNull();
  h.compare().press();
  await settle();
  expect(h.fake.asked.filter((a) => a.ask === "again").map((a) => a.lane), "the stale side alone").toEqual([laneOf(WT)]);
  h.fake.fail(laneOf(WT));
  await settle();
  expect(h.compare().state).toBe("failed");
  expect(h.compare().runnable, "a batch does not run a failing tool again").toBe(false);
  h.stop();
});

test("Run again with both sides kept runs both anew with the tool as it is now (2026-11-24)", async () => {
  // Björn: "in comparisons it doesn't rerun the tool for the folders when I click run again".
  const h = harness({}, (fake) => {
    fake.keep(laneOf(SHA), "a");
    fake.keep(laneOf(WT), "b");
  });
  await settle();
  expect(h.compare().state).toBe("same");
  expect(h.compare().wants).toBe(false);
  h.compare().press();
  await settle();
  expect(h.fake.asked.filter((a) => a.ask === "again").map((a) => a.lane)).toEqual([laneOf(SHA), laneOf(WT)]);
  expect(h.compare().state).toBe("running");
  h.stop();
});

test("a side turning stale under a drawn compare keeps it drawn and says so; other ends drop it", async () => {
  const h = harness({}, (fake) => {
    fake.keep(laneOf(SHA), "a");
    fake.keep(laneOf(WT), "b");
  });
  await settle();
  const made = h.compare().made;
  expect(made).not.toBeNull();
  // The working tree moved on: its side shows its last drawing, made from an earlier state.
  h.fake.keep(laneOf(WT), "b", false);
  h.compare().after.look();
  await settle();
  expect(h.compare().state).toBe("stale");
  expect(h.compare().made, "nothing drawn moves").toBe(made);
  expect(h.fake.compared).toEqual([["a", "b", true]]);
  // Another start (Local changes keeps its compare when a commit moves it): what was compared is not this folder's.
  h.at.before = "9".repeat(40);
  await settle();
  expect(h.compare().made).toBeNull();
  h.stop();
});

test("a single output is compared by its bytes, in the window", async () => {
  const h = harness({ renderer: { ...SITE, output: "image" } }, (fake) => {
    fake.keep(laneOf(SHA), "same-1");
    fake.keep(laneOf(WT), "same-2");
  });
  await settle();
  expect(h.fake.compared, "no folder compare for one output").toEqual([]);
  expect(h.compare().made).toMatchObject({ kind: "single", same: true });
  expect(h.compare().state).toBe("same");
  h.stop();
});
