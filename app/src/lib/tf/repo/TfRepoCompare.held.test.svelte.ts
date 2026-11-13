/**
 * Compare without a clone (the forge-waits pass, 2026-11-13): its first open waited 0.65 to 2.7 s on Gitea's compare.
 * The list knobas last read between the two names is held (`held_at`) and drawn at once under the held line, checked
 * behind against the source's list and replaced in place, quietly, where it moved; its files' lines are read between the
 * two shas it names. A list the repository page asked early is taken, not asked again.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile, RangeFilePage, RangeFiles } from "../../ipc/entity";
import TfRepoCompare from "./TfRepoCompare.svelte";
import { askCompareEarly, forgetEarlyCompares } from "./compare-early";

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
  forgetEarlyCompares();
});

interface Asked {
  head: string;
  read: string;
  answer: (files: RangeFiles) => void;
}

function draw(head: string, before?: (ports: { rangeFiles: (...asked: [string, string, string, boolean, string]) => Promise<RangeFiles> }) => void) {
  const lists: Asked[] = [];
  const pages: { path: string; base: string; head: string; from: number }[] = [];
  const checks: { head: string; read: string; answer: (files: RangeFiles) => void; fail: (why: unknown) => void }[] = [];
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
      return new Promise<RangeFiles>((answer, fail) => checks.push({ head: h, read, answer, fail }));
    },
    cancelRead: async (read: string) => {
      cancelled.push(read);
      return true;
    },
    listen: async () => () => {},
  };
  before?.(ports);
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

const HELD_AT = "2026-11-12T08:00:00Z";
const held = (files: DiffFile[]): RangeFiles => ({ ...list(files), held_at: HELD_AT });
const note = () => text(target.querySelector(".tf-cdiff .tf-heldnote"));

test("a held list draws at once under the held line, its lines with it, and is checked behind", async () => {
  const { lists, pages, checks, sent } = draw("feature/a");
  lists[0]!.answer(held([listed("pay.py"), listed("retry.py")]));
  await settle();
  expect(railNames()).toEqual(["pay.py", "retry.py"]);
  expect(note()).toMatch(/^As knobas read it .+\. Checking the code host…$/);
  expect(checks.map((c) => [c.head, c.read])).toEqual([["feature/a", `${lists[0]!.read}-check`]]);
  // Its lines are read between the two shas it names, which knobas holds too where the whole diff fits.
  expect(pages.map((p) => [p.path, p.base, p.head])).toEqual([
    ["pay.py", BASE, HEAD],
    ["retry.py", BASE, HEAD],
  ]);

  // Create pull request waits on none of it.
  const title = target.querySelector<HTMLInputElement>("#tf-cmp-title")!;
  title.value = "Retry SEPA payouts";
  title.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  target.querySelector<HTMLFormElement>(".tf-cform")!.requestSubmit();
  await settle();
  expect(sent).toHaveLength(1);

  checks[0]!.answer(list([listed("pay.py"), listed("retry.py")]));
  await settle();
  // The words go, the line's room stays: the list does not move up.
  expect(target.querySelector(".tf-cdiff .tf-heldnote")?.getAttribute("aria-hidden")).toBe("true");
  expect(railNames()).toEqual(["pay.py", "retry.py"]);
  expect(pages.map((p) => p.path), "nothing read again").toEqual(["pay.py", "retry.py"]);
  expect(target.querySelector(".tf-cdiff p.tf-cnote[role='status']")).toBeNull();
});

test("a held list the source's answer moved is replaced in place, quietly", async () => {
  const { lists, checks } = draw("feature/a");
  lists[0]!.answer(held([listed("pay.py")]));
  await settle();
  const moved = list([listed("pay.py"), listed("ledger.py")]);
  moved.range = { ...moved.range!, head: "5ead000000000000000000000000000000000000" };
  checks[0]!.answer(moved);
  await settle();
  expect(railNames()).toEqual(["ledger.py", "pay.py"]);
  expect(text(target.querySelector(".tf-cfiles"))).toBe("2 files changed against main");
  expect(target.querySelector(".tf-cdiff p.tf-cnote[role='status']"), "no moved line over a list that said from when it was").toBeNull();
});

test("a held list whose check fails stays, saying the source did not answer", async () => {
  const { lists, checks } = draw("feature/a");
  lists[0]!.answer(held([listed("pay.py")]));
  await settle();
  checks[0]!.fail({ code: "unreachable", message: "connection refused" });
  await settle();
  expect(railNames()).toEqual(["pay.py"]);
  expect(note()).toMatch(/^As knobas read it .+\. the code host did not answer: /);
});

test("a list the repository page asked early is the page's, not asked again", async () => {
  const { lists } = draw("feature/a", (ports) => askCompareEarly(ports.rangeFiles, REPO, "main", "feature/a"));
  expect(lists.map((l) => l.head)).toEqual(["feature/a"]);
  expect(lists[0]!.read).toMatch(/^compare-early-/);
  lists[0]!.answer(list([listed("pay.py")]));
  await settle();
  expect(railNames()).toEqual(["pay.py"]);
});
