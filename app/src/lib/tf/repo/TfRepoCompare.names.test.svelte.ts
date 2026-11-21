/**
 * Compare's first open of a branch with a clone (the stored-compare pass, 2026-11-14): the clone's names of the changed
 * files (`entity_range_names`, one `git diff --raw` that reads no file) are drawn under the clone's line while the list with
 * counts is read beside them, and that list fills them in place, keeping the rows, the line and the lines read for them.
 * A list with counts that lands first wins; one that fails leaves the names.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile, RangeFilePage, RangeFiles, RangeNames } from "../../ipc/entity";
import TfRepoCompare from "./TfRepoCompare.svelte";
import { forgetEarlyCompares } from "./compare-early";

const REPO = "gitea:tidewater/payout-service";
const BASE = "ba5e000000000000000000000000000000000000";
const HEAD = "4ead000000000000000000000000000000000000";

const listed = (path: string, additions = 2, deletions = 1): DiffFile => ({ path, old_path: null, change: "modified", additions, deletions, hunks: [], truncated: false, threads: [] });
const bare = (path: string): DiffFile => listed(path, 0, 0);
const range = { repository: REPO, base: BASE, head: HEAD, commits: [] };
const SYNCED = "2026-11-12T08:00:00Z";
const names = (files: DiffFile[]): RangeNames => ({ paths: files.map((f) => f.path), changes: "M".repeat(files.length), renamed: [], modes: [], truncated: false, range, predicted_at: SYNCED });
const counted = (files: DiffFile[], predicted?: string): RangeFiles => ({
  files,
  truncated: false,
  range: { ...range, commits: [{ sha: "c".repeat(40), title: "Retry SEPA payouts", meta: null }] },
  read_at: predicted === undefined ? "source" : "checkout",
  ...(predicted === undefined ? {} : { predicted_at: predicted }),
});

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  location.hash = "";
  forgetEarlyCompares();
});

function draw(head: string) {
  const lists: { head: string; read: string; answer: (files: RangeFiles) => void; fail: (why: unknown) => void }[] = [];
  const named: { head: string; answer: (files: RangeNames | null) => void }[] = [];
  const pages: { path: string; base: string; head: string }[] = [];
  const checks: { answer: (files: RangeFiles) => void }[] = [];
  const ports = {
    entityView: () => new Promise<never>(() => {}),
    getEntity: () => new Promise<never>(() => {}),
    listSources: async () => [],
    submitWrite: async () => ({ id: 7, state: "queued" }) as never,
    rangeFiles: (_repo: string, _base: string, h: string, _ws: boolean, read: string) =>
      new Promise<RangeFiles>((answer, fail) => lists.push({ head: h, read, answer, fail })),
    rangeNames: (_repo: string, _base: string, h: string) => new Promise<RangeNames | null>((answer) => named.push({ head: h, answer })),
    rangeFile: async (_repo: string, base: string, h: string, path: string, _old: string | null, _ws: boolean, from: number): Promise<RangeFilePage> => {
      pages.push({ path, base, head: h });
      const file = { ...listed(path), hunks: [{ header: "@@ -1,1 +1,2 @@", lines: [{ kind: "add" as const, old: null, new: 1, text: `new in ${path}` }] }] };
      return { file, from, continues: false, next: null };
    },
    sourceRangeFiles: () => new Promise<RangeFiles>((answer) => checks.push({ answer })),
    cancelRead: async () => true,
    listen: async () => () => {},
  };
  const props = $state({
    ports,
    repoId: REPO,
    branches: [
      { name: "main", entityId: `${REPO}@refs/heads/main`, isDefault: true },
      { name: "feature/a", entityId: `${REPO}@refs/heads/feature/a`, isDefault: false },
    ],
    head,
    defaultBranch: "main",
  });
  view = mount(TfRepoCompare, { target, props });
  flushSync();
  return { lists, named, pages, checks };
}

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const railNames = () => [...target.querySelectorAll(".tf-cdiff .tf-flist button")].map((b) => b.getAttribute("title"));
const railCounts = () => [...target.querySelectorAll(".tf-cdiff .tf-flist button .tf-n")].map((n) => text(n));
const note = () => text(target.querySelector(".tf-cdiff p.tf-cnote"));

test("the clone's names draw at once under the clone's line, and the counted list fills them in place", async () => {
  const { lists, named, pages, checks } = draw("feature/a");
  expect(named.map((s) => s.head)).toEqual(["feature/a"]);
  expect(lists.map((l) => l.head), "the list with counts is read beside it").toEqual(["feature/a"]);
  named[0]!.answer(names([bare("pay.py"), bare("retry.py")]));
  await settle();
  expect(railNames()).toEqual(["pay.py", "retry.py"]);
  expect(railCounts(), "no counts yet: the slot waits empty").toEqual(["", ""]);
  expect(note()).toBe("Read from your clone as of the last sync; checking the code host…");
  expect(text(target.querySelector(".tf-cfiles"))).toBe("2 files changed against main");
  expect(target.querySelector(".tf-cdiff .tf-wait-dim"), "nothing dimmed").toBeNull();
  // Each file's lines are read between the two commits the names were read between, before any count is known.
  expect(pages.map((p) => [p.path, p.base, p.head])).toEqual([
    ["pay.py", BASE, HEAD],
    ["retry.py", BASE, HEAD],
  ]);
  const rows = [...target.querySelectorAll(".tf-cdiff .tf-flist button")];

  lists[0]!.answer(counted([listed("pay.py", 4, 2), listed("retry.py", 1, 0)], "2026-11-12T08:00:00Z"));
  await settle();
  expect(railNames()).toEqual(["pay.py", "retry.py"]);
  expect(railCounts()).toEqual(["+4 −2", "+1 −0"]);
  expect([...target.querySelectorAll(".tf-cdiff .tf-flist button")], "the same rows, filled in place").toEqual(rows);
  expect(pages.map((p) => p.path), "the lines read stay").toEqual(["pay.py", "retry.py"]);
  expect(text(target.querySelector(".tf-cdiff .tf-diff[data-file='pay.py'] .tf-fh")), "the card's counts come with the list").toContain("+4");
  // The clone's list is checked against the source behind, under the same line, which says so once it answered.
  expect(note()).toBe("Read from your clone as of the last sync; checking the code host…");
  expect(target.querySelectorAll(".tf-cdiff p.tf-cnote").length, "one line, never a second").toBe(1);
  checks[0]!.answer(counted([listed("pay.py", 4, 2), listed("retry.py", 1, 0)]));
  await settle();
  expect(note()).toBe("Read from your clone: the same as on the code host.");
  expect(target.querySelectorAll(".tf-cdiff .tf-side .tf-r, .tf-side .tf-r").length, "the commits come with the counted list").toBe(1);
});

test("a counted list that lands first is drawn, and names that come after it are not", async () => {
  const { lists, named } = draw("feature/a");
  lists[0]!.answer(counted([listed("pay.py", 4, 2)]));
  await settle();
  named[0]!.answer(names([bare("pay.py"), bare("stale.py")]));
  await settle();
  expect(railNames()).toEqual(["pay.py"]);
  expect(railCounts()).toEqual(["+4 −2"]);
});

test("where the clone cannot say, the page reads the list as before", async () => {
  const { lists, named } = draw("feature/a");
  named[0]!.answer(null);
  await settle();
  expect(railNames()).toEqual([]);
  lists[0]!.answer(counted([listed("pay.py", 4, 2)]));
  await settle();
  expect(railNames()).toEqual(["pay.py"]);
});

test("a counted list that fails leaves the names drawn", async () => {
  const { lists, named } = draw("feature/a");
  named[0]!.answer(names([bare("pay.py")]));
  await settle();
  lists[0]!.fail({ code: "unreachable", message: "connection refused" });
  await settle();
  expect(railNames()).toEqual(["pay.py"]);
  expect(target.querySelector(".tf-cdiff [role='alert']"), "no error over the names").toBeNull();
});
