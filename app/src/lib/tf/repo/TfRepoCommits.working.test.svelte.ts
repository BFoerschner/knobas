/**
 * *Commits* on the working tree (2026-11-21, `docs/decisions/2026-11-21-the-working-tree-is-a-choice-in-the-branch-box.md`):
 * *Uncommitted changes* first where the checkout has any, then the commits the checkout has that the forge lacks, read
 * from the checkout, then the forge's commits; a branch the forge lacks says so over the table; the tab draws once both
 * reads have answered.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";

import type { RangeCommit, RangeFiles, SourceView, ViewRow } from "../../ipc/entity";
import type { WriteSource } from "../../shell/queued-toast";
import TfRepoCommits from "./TfRepoCommits.svelte";

const REPO = "gitea:tidewater/payout-service";
const SOURCE = { id: "gitea", display_name: "Tidewater Git" } as WriteSource;

function commit(sha: string, title: string): ViewRow {
  return { entity_id: `${REPO}@${sha}`, title, meta: "mara · 3 hours ago", tone: null, chip: sha.slice(0, 10), url: null, commit: null } as ViewRow;
}
const VIEW = {
  sections: [{ id: "commits", title: "Commits", count: 2, deferred: false, blocks: [{ type: "entities", rows: [commit("1".repeat(40), "on the forge 1"), commit("2".repeat(40), "on the forge 2")] }] }],
  actions: [],
} as unknown as SourceView;
const LOCAL: RangeCommit[] = [
  { sha: "a".repeat(40), title: "Retry a parked batch", meta: "mara · 5 minutes ago" },
  { sha: "b".repeat(40), title: "Name the retry limit", meta: "mara · 1 hour ago" },
];
const listing = (commits: RangeCommit[]) => ({ files: [], truncated: false, range: { repository: REPO, base: "x", head: "~working-tree", commits } }) as unknown as RangeFiles;

type Disk = { branch: string | null; onForge: boolean | null; base: string | null; changed: number; ahead: number; stamp: string };

let target: HTMLElement | null = null;
let view: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target?.remove();
});

async function settle() {
  for (let n = 0; n < 6; n++) {
    await tick();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  flushSync();
}

/** Mounts the tab on the working tree; `range` answers the checkout's read, which a test may hold back. */
async function draw(disk: Disk | null, range: () => Promise<RangeFiles> = async () => listing(LOCAL)) {
  const asked: string[] = [];
  target = document.createElement("div");
  document.body.append(target);
  const props = $state({
    ports: {
      entityView: async () => VIEW,
      codeFacts: async () => [],
      rangeFiles: (entityId: string, base: string, head: string) => (asked.push(`${entityId} ${base}...${head}`), range()),
    },
    repoId: REPO,
    at: REPO,
    branch: "main",
    defaultBranch: "main",
    source: SOURCE,
    disk,
  });
  view = mount(TfRepoCommits, { target, props });
  await settle();
  return { asked, props };
}

/** Each drawn commit row's cells with words, joined by " | " (the skeleton's rows are not the table's). */
const rows = () =>
  [...target!.querySelectorAll(".tf-rcommits tbody tr")].map((row) =>
    [...row.querySelectorAll("td")].map((cell) => cell.textContent!.replace(/\s+/g, " ").trim()).filter((words) => words !== "").join(" | "),
  );
const note = () => target!.querySelector(".tf-wtnote")?.textContent?.trim() ?? null;

test("on the working tree, Uncommitted changes comes first, then the commits the forge lacks, then the forge's", async () => {
  const { asked } = await draw({ branch: "feat/retry", onForge: true, base: "feat/retry", changed: 3, ahead: 2, stamp: "s1" });
  expect(asked).toEqual([`${REPO} feat/retry...~working-tree`]);
  expect(rows()).toEqual([
    "Uncommitted changes | 3 files changed",
    "aaaaaaa | Retry a parked batch Not pushed | mara · 5 minutes ago",
    "bbbbbbb | Name the retry limit Not pushed | mara · 1 hour ago",
    expect.stringContaining("on the forge 1"),
    expect.stringContaining("on the forge 2"),
  ]);
  expect(target!.querySelector<HTMLAnchorElement>(".tf-rcommits tbody tr a")!.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/local");
  // A local commit has no id in the mirror: drawn plain, its box not offered.
  const local = target!.querySelectorAll(".tf-rcommits tbody tr")[1]!;
  expect(local.querySelector("a")).toBeNull();
  expect(local.querySelector("input")).toBeNull();
  expect(note()).toBeNull();
});

test("a branch the forge lacks says so over the table, and its commits are those the default branch lacks", async () => {
  const { asked } = await draw({ branch: "feat/local-only", onForge: false, base: "main", changed: 0, ahead: 0, stamp: "s1" });
  expect(asked).toEqual([`${REPO} main...~working-tree`]);
  expect(note()).toBe("feat/local-only is not on Tidewater Git yet. Push it to open a pull request from it.");
  expect(rows().slice(0, 2)).toEqual(["aaaaaaa | Retry a parked batch Not pushed | mara · 5 minutes ago", "bbbbbbb | Name the retry limit Not pushed | mara · 1 hour ago"]);
  // A clean checkout has no Uncommitted changes row.
  expect(rows().some((row) => row.startsWith("Uncommitted"))).toBe(false);
});

test("a branch the forge has with nothing ahead reads nothing from the checkout", async () => {
  const { asked } = await draw({ branch: "main", onForge: true, base: "main", changed: 1, ahead: 0, stamp: "s1" });
  expect(asked).toEqual([]);
  expect(rows()).toEqual(["Uncommitted changes | 1 file changed", expect.stringContaining("on the forge 1"), expect.stringContaining("on the forge 2")]);
});

test("the tab draws once: the forge's commits wait for the checkout's, so none comes in above another", async () => {
  let answer!: () => void;
  const late = new Promise<void>((resolve) => (answer = resolve));
  await draw({ branch: "feat/retry", onForge: true, base: "feat/retry", changed: 0, ahead: 2, stamp: "s1" }, async () => (await late, listing(LOCAL)));
  expect(rows()).toEqual([]);
  answer();
  await settle();
  expect(rows()).toHaveLength(4);
  expect(rows()[0]).toContain("Retry a parked batch");
});

test("while the branches are being read, whether the forge has the branch is unknown, and the tab waits", async () => {
  const { asked, props } = await draw({ branch: "feat/retry", onForge: null, base: "main", changed: 2, ahead: 0, stamp: "s1" });
  expect(asked).toEqual([]);
  expect(rows()).toEqual([]);
  expect(note()).toBeNull();
  props.disk = { branch: "feat/retry", onForge: true, base: "feat/retry", changed: 2, ahead: 0, stamp: "s1" };
  await settle();
  expect(rows()[0]).toBe("Uncommitted changes | 2 files changed");
});

test("a new stamp reads the checkout again, and the rows drawn stay until it lands", async () => {
  let reads = 0;
  const { props } = await draw({ branch: "feat/retry", onForge: true, base: "feat/retry", changed: 0, ahead: 2, stamp: "s1" }, async () => (reads++, listing(reads === 1 ? LOCAL : LOCAL.slice(0, 1))));
  expect(rows()).toHaveLength(4);
  // The same facts handed again ask nothing.
  props.disk = { branch: "feat/retry", onForge: true, base: "feat/retry", changed: 0, ahead: 2, stamp: "s1" };
  await settle();
  expect(reads).toBe(1);
  props.disk = { branch: "feat/retry", onForge: true, base: "feat/retry", changed: 0, ahead: 1, stamp: "s2" };
  await settle();
  expect(reads).toBe(2);
  expect(rows()).toHaveLength(3);
});

test("off the working tree, the tab is the forge's commits alone", async () => {
  const { asked } = await draw(null);
  expect(asked).toEqual([]);
  expect(rows()).toHaveLength(2);
});
