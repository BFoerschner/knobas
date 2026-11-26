/**
 * Compare with *From* the working tree (ux26, ADR-0050): *Working tree* is *From*'s first choice, its pick sets the
 * address, and the page drawn for it is the form's short shape from its first flush, reads the working tree's names and
 * counted list and nothing of a branch, lists *Uncommitted changes* first among the commits, and reads again on focus
 * without laying down a state it drew already.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { CheckoutView, DiffFile, RangeFilePage, RangeFiles, RangeNames, WorkingAt } from "../../ipc/entity";
import TfRepoCompare from "./TfRepoCompare.svelte";
import TfRepoCompareWorking from "./TfRepoCompareWorking.svelte";
import { forgetEarlyCompares } from "./compare-early";

const REPO = "gitea:tidewater/payout-service";
const BASE = "ba5e000000000000000000000000000000000000";
const HEAD = "4ead000000000000000000000000000000000000";
const listed = (path: string, additions = 2, deletions = 1): DiffFile => ({ path, old_path: null, change: "modified", additions, deletions, hunks: [], truncated: false, threads: [] });
const BRANCHES = [
  { name: "main", entityId: `${REPO}@refs/heads/main`, isDefault: true },
  { name: "feature/a", entityId: `${REPO}@refs/heads/feature/a`, isDefault: false },
];
const at = (stamp: string, untracked: string[] = []): WorkingAt => ({ stamp, head: HEAD, branch: "feature/a", untracked, untracked_truncated: false });
const range = { repository: REPO, base: BASE, head: "~working-tree", commits: [{ sha: "c".repeat(40), title: "Retry SEPA payouts", meta: null }] };
const CLONED: CheckoutView = { path: "/Users/mara/src/payout-service", found_by: "scan", tree: { status: "read", branch: "feature/a", head: HEAD, upstream: null, ahead: 0, behind: 0, changed: 2, untracked: 1, conflicted: 0 } } as never;

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

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}
const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";

function drawWorking(checkout: CheckoutView | undefined = CLONED) {
  const asked: string[] = [];
  const lists: { base: string; answer: (files: RangeFiles) => void }[] = [];
  const named: { answer: (files: RangeNames | null) => void }[] = [];
  const ports = {
    entityView: (id: string, section: string | null) => (asked.push(`view ${id} ${section}`), new Promise<never>(() => {})),
    rangeFiles: (_repo: string, base: string, head: string) => (asked.push(`files ${base} ${head}`), new Promise<RangeFiles>((answer) => lists.push({ base, answer }))),
    rangeNames: (_repo: string, base: string, head: string) => (asked.push(`names ${base} ${head}`), new Promise<RangeNames | null>((answer) => named.push({ answer }))),
    rangeFile: async (_repo: string, _base: string, _head: string, path: string, _old: string | null, _ws: boolean, from: number): Promise<RangeFilePage> => ({
      file: { ...listed(path), hunks: [{ header: "@@ -1 +1 @@", lines: [{ kind: "add" as const, old: null, new: 1, text: `new in ${path}` }] }] },
      from,
      continues: false,
      next: null,
    }),
    cancelRead: async () => true,
    fileBytes: async () => new Uint8Array(),
  };
  view = mount(TfRepoCompareWorking, { target, props: { ports, repoId: REPO, branches: BRANCHES, defaultBranch: "main", checkout } as never });
  flushSync();
  return { asked, lists, named };
}

test("From offers the working tree first in Compare, and picking it sets the address with Into kept", () => {
  view = mount(TfRepoCompare, {
    target,
    props: { ports: { entityView: () => new Promise<never>(() => {}), getEntity: () => new Promise<never>(() => {}), listSources: async () => [], listen: async () => () => {} }, repoId: REPO, branches: BRANCHES, head: "feature/a", base: "feature/a", defaultBranch: "main", checkout: CLONED } as never,
  });
  flushSync();
  const from = target.querySelector<HTMLSelectElement>("#tf-cmp-from")!;
  expect([...from.options].map((o) => [o.value, o.textContent, o.disabled])).toEqual([
    ["~working-tree", "Working tree", false],
    ["main", "main", false],
    ["feature/a", "feature/a", false],
  ]);
  from.value = "~working-tree";
  from.dispatchEvent(new Event("change", { bubbles: true }));
  expect(decodeURIComponent(location.hash)).toBe(`#/repo/${REPO}/compare/feature/a...~working-tree`);
});

test("where the repository is not cloned here, the working tree is drawn disabled with why", () => {
  view = mount(TfRepoCompare, {
    target,
    props: { ports: { entityView: () => new Promise<never>(() => {}), getEntity: () => new Promise<never>(() => {}), listSources: async () => [], listen: async () => () => {} }, repoId: REPO, branches: BRANCHES, head: "feature/a", defaultBranch: "main", checkout: { path: null, found_by: null, tree: null } } as never,
  });
  flushSync();
  const first = target.querySelector<HTMLSelectElement>("#tf-cmp-from")!.options[0]!;
  expect([first.textContent, first.disabled]).toEqual(["Working tree: not cloned on this computer", true]);
});

test("the working tree's page is the short shape in its first flush and reads nothing of a branch", async () => {
  const { asked, lists, named } = drawWorking();
  expect(target.querySelector("#tf-cmp-title")).toBeNull();
  expect(text(target.querySelector("h2.tf-cgroup-h"))).toBe("Compare");
  expect(text(target.querySelector(".tf-cworkline"))).toBe(
    "Your checkout at /Users/mara/src/payout-service, on feature/a at 4ead000. Commit and push it to propose a pull request.",
  );
  expect(target.querySelector<HTMLSelectElement>("#tf-cmp-from")!.value).toBe("~working-tree");
  expect(asked).toEqual(["names main ~working-tree", "files main ~working-tree"]);
  named[0]!.answer({ paths: ["docs/a.md", "notes/new.md"], changes: "MA", renamed: [], modes: [], truncated: false, range, predicted_at: "", working: at("s1", ["notes/new.md"]) });
  await settle();
  expect([...target.querySelectorAll(".tf-cdiff .tf-flist button")].map((b) => b.getAttribute("title"))).toEqual(["docs/a.md", "notes/new.md (untracked)"]);
  lists[0]!.answer({ files: [listed("docs/a.md"), { ...listed("notes/new.md", 3, 0), change: "added" }], truncated: false, range, read_at: "checkout", working: at("s1", ["notes/new.md"]) });
  await settle();
  expect(text(target.querySelector(".tf-cfiles"))).toBe("2 files changed against main");
  expect([...target.querySelectorAll(".tf-ccommits .tf-r")].map((r) => text(r))).toEqual(["Uncommitted changes", "ccccccccccRetry SEPA payouts"]);
  expect(text(target.querySelector(".tf-ccount"))).toBe("1, and what is not committed");
  // No source read of any kind: the working tree is read at the checkout alone.
  expect(asked.filter((a) => a.startsWith("view"))).toEqual([]);
});

test("a focus re-read lays nothing down at the same stamp, and replaces the list at a new one", async () => {
  const { lists, named } = drawWorking();
  named[0]!.answer(null);
  lists[0]!.answer({ files: [listed("docs/a.md")], truncated: false, range, read_at: "checkout", working: at("s1") });
  await settle();
  const before = target.querySelector(".tf-cdiff .tf-flist button");
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(lists).toHaveLength(2);
  lists[1]!.answer({ files: [listed("docs/a.md")], truncated: false, range, read_at: "checkout", working: at("s1") });
  await settle();
  expect(target.querySelector(".tf-cdiff .tf-flist button"), "the same row, not laid down again").toBe(before);
  window.dispatchEvent(new Event("focus"));
  await settle();
  lists[2]!.answer({ files: [listed("docs/a.md"), listed("docs/b.md")], truncated: false, range, read_at: "checkout", working: at("s2") });
  await settle();
  expect([...target.querySelectorAll(".tf-cdiff .tf-flist button")].map((b) => b.getAttribute("title"))).toEqual(["docs/a.md", "docs/b.md"]);
});

test("with no checkout here, the page says so and reads nothing", () => {
  const { asked } = drawWorking({ path: null, found_by: null, tree: null } as never);
  expect(asked).toEqual([]);
  expect(text(target.querySelector(".tf-cdiff .tf-empty"))).toBe("This repository is not cloned on this computer, so it has no working tree here.");
});
