/**
 * Swapping branches in a big compare (the big-compare pass, 2026-11-16): a pair Compare drew before this session draws
 * its list in the pick's own frame, with nothing dimmed, while the pick's reads go out and fill it in place; the names
 * of the next likely picks are read at the clone while the page is idle, so such a pick draws at once too; *Commits*
 * draws one line a commit with its count, in a box that draws only the rows it shows; and a check against the source
 * that finds the same diff keeps the clone's longer list of commits.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile, RangeCommit, RangeFilePage, RangeFiles, RangeNames } from "../../ipc/entity";
import TfRepoCompare from "./TfRepoCompare.svelte";
import { forgetEarlyCompares } from "./compare-early";
import { forgetKeptPages } from "../diff/fill";

const REPO = "gitea:tidewater/payout-service";
const BASE = "ba5e000000000000000000000000000000000000";
const SYNCED = "2026-11-12T08:00:00Z";
const tip = (head: string) => (head.endsWith("a") ? "a" : head.endsWith("b") ? "b" : "c").repeat(40);

const listed = (path: string, additions = 2, deletions = 1): DiffFile => ({ path, old_path: null, change: "modified", additions, deletions, hunks: [], truncated: false, threads: [] });
const commitsOf = (n: number, of: string): RangeCommit[] =>
  Array.from({ length: n }, (_, i) => ({ sha: String(i).padStart(40, "0"), title: `${of}: change ${i + 1} with a title long enough to be cut at the rail's width`, meta: null }));
const counted = (head: string, files: DiffFile[], commits: RangeCommit[] = commitsOf(1, head), read_at: "checkout" | "source" = "checkout"): RangeFiles => ({
  files,
  truncated: false,
  range: { repository: REPO, base: BASE, head: tip(head), commits },
  read_at,
  ...(read_at === "checkout" ? { predicted_at: SYNCED } : {}),
});
const names = (head: string, files: string[]): RangeNames => ({
  paths: files,
  changes: "M".repeat(files.length),
  renamed: [],
  modes: [],
  truncated: false,
  range: { repository: REPO, base: BASE, head: tip(head), commits: [] },
  predicted_at: SYNCED,
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
  forgetKeptPages();
});

function draw(head: string) {
  const lists: { base: string; head: string; answer: (files: RangeFiles) => void }[] = [];
  const named: { base: string; head: string; answer: (files: RangeNames | null) => void }[] = [];
  const checks: { head: string; answer: (files: RangeFiles) => void }[] = [];
  const ports = {
    entityView: () => new Promise<never>(() => {}),
    getEntity: () => new Promise<never>(() => {}),
    listSources: async () => [],
    submitWrite: async () => ({ id: 7, state: "queued" }) as never,
    rangeFiles: (_repo: string, b: string, h: string) => new Promise<RangeFiles>((answer) => lists.push({ base: b, head: h, answer })),
    rangeNames: (_repo: string, b: string, h: string) => new Promise<RangeNames | null>((answer) => named.push({ base: b, head: h, answer })),
    rangeFile: async (_repo: string, _base: string, _h: string, path: string, _old: string | null, _ws: boolean, from: number): Promise<RangeFilePage> => ({
      file: { ...listed(path), hunks: [{ header: "@@ -1,1 +1,2 @@", lines: [{ kind: "add" as const, old: null, new: 1, text: `new in ${path}` }] }] },
      from,
      continues: false,
      next: null,
    }),
    sourceRangeFiles: (_repo: string, _b: string, h: string) => new Promise<RangeFiles>((answer) => checks.push({ head: h, answer })),
    cancelRead: async () => true,
    listen: async () => () => {},
  };
  const props = $state({
    ports,
    repoId: REPO,
    branches: [
      { name: "main", entityId: `${REPO}@refs/heads/main`, isDefault: true },
      { name: "feature/a", entityId: `${REPO}@refs/heads/feature/a`, isDefault: false },
      { name: "feature/b", entityId: `${REPO}@refs/heads/feature/b`, isDefault: false },
    ],
    head,
    defaultBranch: "main",
  });
  view = mount(TfRepoCompare, { target, props });
  flushSync();
  return { props, lists, named, checks };
}

async function settle(rounds = 4) {
  for (let round = 0; round < rounds; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}
/** Past the idle waits the page reads ahead in (a timer where the test's window has no idle callback). */
const idle = async () => {
  for (let i = 0; i < 12; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 120));
    await settle(1);
  }
};

/** Waits for `ready`, a loaded machine's idle turns and timers coming late: up to 10 s. */
async function until(ready: () => boolean) {
  for (let i = 0; i < 100 && !ready(); i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 100));
    await settle(1);
  }
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const railNames = () => [...target.querySelectorAll(".tf-cdiff .tf-flist button")].map((b) => b.getAttribute("title"));

test("a pair drawn before draws at once when it is picked again, nothing dimmed, and its reads fill it in place", async () => {
  const { props, lists } = draw("feature/a");
  lists.find((l) => l.head === "feature/a")!.answer(counted("feature/a", [listed("pay.py", 4, 2), listed("retry.py")]));
  await settle();
  expect(railNames()).toEqual(["pay.py", "retry.py"]);

  props.head = "feature/b";
  flushSync();
  expect(target.querySelector(".tf-cdiff .tf-wait-dim"), "a pair never drawn waits, dimmed, as before").not.toBeNull();
  lists.find((l) => l.head === "feature/b")!.answer(counted("feature/b", [listed("ledger.py")]));
  await settle();
  expect(railNames()).toEqual(["ledger.py"]);

  const before = lists.length;
  props.head = "feature/a";
  flushSync();
  expect(railNames(), "drawn in the pick's own frame").toEqual(["pay.py", "retry.py"]);
  expect(text(target.querySelector(".tf-cdiff .tf-flist button .tf-n")), "with the counts it had").toBe("+4 −2");
  expect(target.querySelector(".tf-cdiff .tf-wait-dim"), "nothing dimmed").toBeNull();
  expect(text(target.querySelector(".tf-cdiff p.tf-cnote"))).toBe("Read from your clone as of the last sync; checking the code host…");
  expect(lists.length, "the pick reads as any pick does").toBe(before + 1);
  const rows = [...target.querySelectorAll(".tf-cdiff .tf-flist button")];
  lists.at(-1)!.answer(counted("feature/a", [listed("pay.py", 4, 2), listed("retry.py")]));
  await settle();
  expect([...target.querySelectorAll(".tf-cdiff .tf-flist button")], "the same rows, filled in place").toEqual(rows);
});

test("the names of the next picks are read at the clone while the page is idle, and such a pick draws at once", async () => {
  const { props, lists, named } = draw("feature/a");
  named.find((n) => n.head === "feature/a")!.answer(null);
  lists.find((l) => l.head === "feature/a")!.answer(counted("feature/a", [listed("pay.py")]));
  await settle();
  const asked = named.length;
  await until(() => named.length > asked);
  const ahead = named.slice(asked).map((n) => `${n.base}...${n.head}`);
  // *From* the other branch (not the default one, which moves *Into*), then *Into* the other branches.
  expect(ahead[0]).toBe("main...feature/b");
  named.slice(asked)[0]!.answer(names("feature/b", ["ledger.py", "rails.py"]));
  await until(() => named.length > asked + 1);
  expect(named.slice(asked).map((n) => `${n.base}...${n.head}`)).toContain("feature/b...feature/a");

  props.head = "feature/b";
  flushSync();
  expect(railNames(), "drawn in the pick's own frame from the names read ahead").toEqual(["ledger.py", "rails.py"]);
  expect(target.querySelector(".tf-cdiff .tf-wait-dim")).toBeNull();
});

test("nothing is read ahead from a list the forge answered", async () => {
  const { lists, named } = draw("feature/a");
  named.find((n) => n.head === "feature/a")!.answer(null);
  lists.find((l) => l.head === "feature/a")!.answer(counted("feature/a", [listed("pay.py")], commitsOf(1, "a"), "source"));
  await settle();
  const asked = named.length;
  await idle();
  expect(named.length).toBe(asked);
});

test("Commits draws one line a commit with its count, and only the rows its box shows", async () => {
  const { lists } = draw("feature/a");
  lists.find((l) => l.head === "feature/a")!.answer(counted("feature/a", [listed("pay.py")], commitsOf(300, "PAY-412")));
  await settle();
  expect(text(target.querySelector(".tf-ccommits .tf-ccount"))).toBe("300");
  const rows = [...target.querySelectorAll(".tf-ccommits .tf-crows .tf-r")];
  expect(rows.length, "windowed").toBeLessThan(80);
  expect(rows.length).toBeGreaterThan(0);
  expect(rows[0]!.getAttribute("title"), "the whole title on hover").toBe("PAY-412: change 1 with a title long enough to be cut at the rail's width");
  expect(target.querySelector(".tf-ccommits .tf-t2"), "one line, not two").toBeNull();
});

test("a check that finds the same diff keeps the clone's longer list of commits", async () => {
  const { lists, checks } = draw("feature/a");
  lists.find((l) => l.head === "feature/a")!.answer(counted("feature/a", [listed("pay.py")], commitsOf(300, "a")));
  await settle();
  // The source's compare stops at 250 commits (Gitea's), for the same two commits.
  checks.find((c) => c.head === "feature/a")!.answer(counted("feature/a", [listed("pay.py")], commitsOf(250, "a"), "source"));
  await settle();
  expect(text(target.querySelector(".tf-cdiff p.tf-cnote"))).toBe("Read from your clone: the same as on the code host.");
  expect(text(target.querySelector(".tf-ccommits .tf-ccount"))).toBe("300");
});
