/**
 * Compare read a file at a time (the compare pass, 2026-11-13): the branch's list of files draws as soon as it answers,
 * its commits with it, each file's lines read as it comes near the view; another pick calls the last read off at the
 * backend and drops its late answer; the check behind a predicted list asks the source's list, never its every line;
 * and *Create pull request* waits on none of it.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile, RangeFilePage, RangeFiles } from "../../ipc/entity";
import TfRepoCompare from "./TfRepoCompare.svelte";

const REPO = "gitea:tidewater/payout-service";
const BASE = "ba5e000000000000000000000000000000000000";
const HEAD = "4ead000000000000000000000000000000000000";

const listed = (path: string, additions = 2): DiffFile => ({ path, old_path: null, change: "added", additions, deletions: 0, hunks: [], truncated: false, threads: [] });
const list = (files: DiffFile[], predicted?: string, commits: string[] = []): RangeFiles => ({
  files,
  truncated: false,
  range: { repository: REPO, base: BASE, head: HEAD, commits: commits.map((title, n) => ({ sha: `${n}`.padStart(40, "c"), title, meta: null })) },
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
});

interface Asked {
  head: string;
  read: string;
  answer: (files: RangeFiles) => void;
}

function draw(head: string) {
  const lists: Asked[] = [];
  const pages: { path: string; base: string; head: string; from: number }[] = [];
  const checks: { head: string; read: string }[] = [];
  const cancelled: string[] = [];
  const sent: unknown[] = [];
  const ports = {
    entityView: () => new Promise<never>(() => {}),
    getEntity: () => new Promise<never>(() => {}),
    listSources: async () => [],
    submitWrite: async (payload: unknown) => {
      sent.push(payload);
      return { id: 7, state: "queued" } as never;
    },
    rangeDiff: () => Promise.reject(new Error("the whole diff is never read")),
    sourceRangeDiff: () => Promise.reject(new Error("the whole diff is never read")),
    rangeFiles: (_repo: string, _base: string, h: string, _ws: boolean, read: string) => new Promise<RangeFiles>((answer) => lists.push({ head: h, read, answer })),
    rangeFile: async (_repo: string, base: string, h: string, path: string, _old: string | null, _ws: boolean, from: number): Promise<RangeFilePage> => {
      pages.push({ path, base, head: h, from });
      const file = { ...listed(path), hunks: [{ header: "@@ -0,0 +1,2 @@", lines: [{ kind: "add" as const, old: null, new: 1, text: `one of ${path}` }, { kind: "add" as const, old: null, new: 2, text: `two of ${path}` }] }] };
      return { file, from, continues: false, next: null };
    },
    sourceRangeFiles: (_repo: string, _base: string, h: string, read: string) => {
      checks.push({ head: h, read });
      return new Promise<RangeFiles>(() => {});
    },
    cancelRead: async (read: string) => {
      cancelled.push(read);
      return true;
    },
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
  return { props, lists, pages, checks, cancelled, sent };
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

test("the list draws as it answers, its commits with it, and each file's lines are read between the commits it settled", async () => {
  const { lists, pages } = draw("feature/a");
  expect(lists.map((l) => l.head)).toEqual(["feature/a"]);
  lists[0]!.answer(list([listed("pay.py"), listed("retry.py")], undefined, ["Retry SEPA payouts", "Add the payout"]));
  await settle();
  expect(railNames()).toEqual(["pay.py", "retry.py"]);
  expect(text(target.querySelector(".tf-cfiles"))).toBe("2 files changed against main");
  expect([...target.querySelectorAll(".tf-side .tf-r .tf-t")].map(text)).toEqual(["Retry SEPA payouts", "Add the payout"]);
  expect(pages.map((p) => [p.path, p.base, p.head, p.from])).toEqual([
    ["pay.py", BASE, HEAD, 0],
    ["retry.py", BASE, HEAD, 0],
  ]);
  expect(text(target.querySelector('.tf-diff[data-file="pay.py"] .tf-dbody'))).toContain("one of pay.py");
});

test("another pick calls the last read off at the backend, and its late answer is dropped", async () => {
  const { props, lists, cancelled } = draw("feature/a");
  props.head = "feature/b";
  flushSync();
  expect(lists.map((l) => l.head)).toEqual(["feature/a", "feature/b"]);
  expect(cancelled).toContain(lists[0]!.read);
  lists[1]!.answer(list([listed("b.py")]));
  lists[0]!.answer(list([listed("a.py")]));
  await settle();
  expect(railNames()).toEqual(["b.py"]);
});

test("a predicted list is checked against the source's list, which the next pick calls off too", async () => {
  const { props, lists, checks, cancelled } = draw("feature/a");
  lists[0]!.answer(list([listed("pay.py")], "2026-11-13T10:00:00Z"));
  await settle();
  expect(checks).toEqual([{ head: "feature/a", read: `${lists[0]!.read}-check` }]);
  expect(text(target.querySelector(".tf-cdiff .tf-cnote"))).toBe("Read from your clone as of the last sync; checking the code host…");
  props.head = "feature/b";
  flushSync();
  expect(cancelled).toContain(`${lists[0]!.read}-check`);
});

test("Create pull request goes while the list is still being read", async () => {
  const { lists, sent } = draw("feature/a");
  expect(lists).toHaveLength(1);
  const title = target.querySelector<HTMLInputElement>("#tf-cmp-title")!;
  title.value = "Retry SEPA payouts";
  title.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  target.querySelector<HTMLFormElement>(".tf-cform")!.requestSubmit();
  await settle();
  expect(sent).toHaveLength(1);
});
