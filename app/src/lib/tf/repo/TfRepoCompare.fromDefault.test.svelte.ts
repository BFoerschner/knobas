/**
 * A pull request from the default branch into another (Björn, 2026-11-13: "I cannot make a pullrequest into any branch
 * FROM master"). *From* offers the default branch; choosing it while *Into* is the default moves *Into* to the branch the
 * reader came from, else the newest other, and says so; the diff, its count and *Commits* are against that *Into*; the
 * title names both ends; and the write goes with head the default branch and base the chosen one. The Branches tab's
 * default row offers *New pull request* too.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile, RangeDiff, SourceView, ViewAction, ViewRow } from "../../ipc/entity";
import TfRepoBranches from "./TfRepoBranches.svelte";
import TfRepoCompare from "./TfRepoCompare.svelte";
import { compareHash } from "./repo";

const REPO = "gitea:tidewater/payout-fromdefault";
const MAIN = `${REPO}@refs/heads/main`;
const NEWEST = "release/1.4";
const CAME = "feature/PAY-231-sepa-retry";
const BRANCHES = [
  { name: "main", entityId: MAIN, isDefault: true },
  { name: NEWEST, entityId: `${REPO}@refs/heads/${NEWEST}`, isDefault: false },
  { name: CAME, entityId: `${REPO}@refs/heads/${CAME}`, isDefault: false },
];

const file = (path: string): DiffFile => ({ path, old_path: null, change: "modified", additions: 1, deletions: 0, hunks: [], truncated: false, threads: [] });
const commit = (sha: string, title: string) => ({ sha: sha.repeat(40).slice(0, 40), title, meta: null });

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let asked: [string, string][];
let viewed: string[];
let sent: unknown[];

beforeEach(() => {
  asked = [];
  viewed = [];
  sent = [];
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  location.hash = "";
});

/** The diff from `base` to `head`: main carries two commits over the release and one file over the feature. */
async function rangeDiff(_repo: string, base: string, head: string): Promise<RangeDiff> {
  asked.push([base, head]);
  const into = base === NEWEST;
  return {
    files: into ? [file("pay.py"), file("retry.py")] : [file("pay.py")],
    truncated: false,
    range: {
      repository: REPO,
      base: "b".repeat(40),
      head: "a".repeat(40),
      commits: into ? [commit("c", "Retry with backoff"), commit("d", "Bump deps")] : [commit("e", "Retry with backoff")],
    },
    read_at: "source",
  };
}

const compareProps = $state({ head: "main" as string | null, base: null as string | null });

function drawCompare(head: string | null, base: string | null = null, over: Record<string, unknown> = {}) {
  compareProps.head = head;
  compareProps.base = base;
  const ports = {
    entityView: async (id: string) => {
      viewed.push(id);
      // The default branch's own view: its latest commits, measured against nothing.
      return { sections: [{ id: "commits", title: "Commits", count: 2, deferred: false, blocks: [{ type: "entities", title: null, rows: [{ entity_id: null, title: "Latest on main", meta: null, tone: null, chip: "9999999", url: null }] }] }], actions: [] } as unknown as SourceView;
    },
    getEntity: () => new Promise<never>(() => {}),
    listSources: async () => [],
    submitWrite: async (payload: unknown) => {
      sent.push(payload);
      return { id: 41, state: "queued" } as never;
    },
    rangeDiff,
    listen: async () => () => {},
    ...over,
  };
  view = mount(TfRepoCompare, {
    target,
    props: {
      ports,
      repoId: REPO,
      branches: BRANCHES,
      get head() {
        return compareProps.head;
      },
      get base() {
        return compareProps.base;
      },
      defaultBranch: "main",
    },
  });
  flushSync();
}

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const select = (id: string) => target.querySelector<HTMLSelectElement>(`#${id}`)!;
const pick = (id: string, value: string) => {
  const it = select(id);
  it.value = value;
  it.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
};
/** What the address names, as the page above would hand it back. */
function follow() {
  const tail = decodeURIComponent(location.hash.slice(location.hash.indexOf("/compare/") + "/compare/".length));
  const cut = tail.indexOf("...");
  compareProps.head = cut > 0 ? tail.slice(cut + 3) : tail;
  compareProps.base = cut > 0 ? tail.slice(0, cut) : null;
  flushSync();
}

test("From offers the default branch, and choosing it moves Into to the branch the reader came from", async () => {
  drawCompare(CAME);
  await settle();
  expect([...select("tf-cmp-from").options].map((o) => o.value)).toContain("main");

  pick("tf-cmp-from", "main");
  expect(location.hash).toBe(compareHash(REPO, "main", CAME));
  follow();
  await settle();

  expect(select("tf-cmp-into").value).toBe(CAME);
  expect(select("tf-cmp-from").value).toBe("main");
  expect(text(target.querySelector(".tf-cstart"))).toBe(`Into moved to ${CAME}, the branch you came from.`);
  expect(asked.at(-1)).toEqual([CAME, "main"]);
  expect(text(target.querySelector(".tf-cfiles"))).toBe(`1 file changed against ${CAME}`);
  expect(target.querySelector<HTMLInputElement>("#tf-cmp-title")!.value).toBe(`Merge main into ${CAME}`);
  expect(target.querySelector(".tf-dwhy")).toBeNull();
});

test("arriving at the default branch's compare: Into is the newest other branch, and the diff, count and commits are against it", async () => {
  drawCompare("main");
  await settle();

  expect(select("tf-cmp-into").value).toBe(NEWEST);
  expect(text(target.querySelector(".tf-cstart"))).toBe(`Into is ${NEWEST}, the newest other branch.`);
  expect(asked).toEqual([[NEWEST, "main"]]);
  expect(text(target.querySelector(".tf-cfiles"))).toBe(`2 files changed against ${NEWEST}`);
  // The compare's own commits, not the default branch's latest: its view measures it against nothing, so it is not read.
  expect([...target.querySelectorAll(".tf-side .tf-r .tf-t")].map(text)).toEqual(["Retry with backoff", "Bump deps"]);
  expect(viewed).not.toContain(MAIN);
  expect(target.querySelector<HTMLInputElement>("#tf-cmp-title")!.value).toBe(`Merge main into ${NEWEST}`);

  // Another Into: the title proposed follows it, the line goes.
  pick("tf-cmp-into", CAME);
  await settle();
  expect(asked.at(-1)).toEqual([CAME, "main"]);
  expect(target.querySelector<HTMLInputElement>("#tf-cmp-title")!.value).toBe(`Merge main into ${CAME}`);
  expect(target.querySelector(".tf-cstart")).toBeNull();
});

test("the pull request goes with head the default branch and base the chosen Into; one sent holds back only that Into", async () => {
  drawCompare("main");
  await settle();
  target.querySelector<HTMLFormElement>(".tf-cform")!.requestSubmit();
  await settle();
  expect(sent).toEqual([{ CreatePullRequest: { entity: REPO, title: `Merge main into ${NEWEST}`, body: "", head: "main", base: NEWEST } }]);

  // Back on the page (its echo stands): into the release it is on its way, into the feature the form is there.
  unmount(view!);
  view = null;
  drawCompare("main", NEWEST);
  await settle();
  expect(text(target.querySelector(".tf-cexists"))).toBe(`A pull request from main into ${NEWEST} is on its way: Merge main into ${NEWEST}.`);
  pick("tf-cmp-into", CAME);
  await settle();
  expect(target.querySelector(".tf-cexists")).toBeNull();
  expect(text(target.querySelector("button[type=submit]"))).toBe("Create pull request");
});

test("an Into the reader picked stays when From becomes the default branch; one the page moved goes back when From leaves it", async () => {
  drawCompare(CAME, NEWEST);
  await settle();
  pick("tf-cmp-from", "main");
  expect(location.hash).toBe(compareHash(REPO, "main", NEWEST));
  follow();
  await settle();
  expect(select("tf-cmp-into").value).toBe(NEWEST);
  expect(target.querySelector(".tf-cstart")).toBeNull();

  unmount(view!);
  view = null;
  drawCompare("main");
  await settle();
  expect(select("tf-cmp-into").value).toBe(NEWEST);
  pick("tf-cmp-from", CAME);
  expect(location.hash).toBe(compareHash(REPO, CAME));
  follow();
  await settle();
  expect(select("tf-cmp-into").value).toBe("main");
  expect(asked.at(-1)).toEqual(["main", CAME]);
});

test("the Branches tab's default row offers New pull request, to the default branch's compare", async () => {
  const vrow = (entity_id: string | null, title: string, chip: string | null = null): ViewRow => ({ entity_id, title, meta: "today", tone: null, chip, url: null });
  const offer = { write_op: "create_pull_request", disabled: null } as unknown as ViewAction;
  view = mount(TfRepoBranches, {
    target,
    props: {
      repoId: REPO,
      ports: {
        entityView: async (id: string, sectionId: string | null) => {
          if (id === REPO && sectionId === "branches")
            return { sections: [{ id: "branches", title: "Branches", count: null, deferred: false, blocks: [{ type: "entities", title: null, rows: [vrow(MAIN, "main", "default"), vrow(`${REPO}@refs/heads/${NEWEST}`, NEWEST)] }] }], actions: [] } as unknown as SourceView;
          return { sections: [{ id: "overview", title: "Overview", count: null, deferred: false, blocks: [{ type: "facts", facts: [{ label: "Ahead", value: "1" }, { label: "Behind", value: "0" }] }] }], actions: [offer] } as unknown as SourceView;
        },
        submitWrite: async () => ({ id: 1, state: "queued" }) as never,
        codeFacts: async () => [],
        listen: async () => () => {},
      },
    },
  });
  await settle();
  const first = target.querySelector(".tf-rbranches tbody tr[data-place]")!;
  expect(text(first.querySelector("td"))).toContain("main");
  const press = [...first.querySelectorAll("a.tf-btn")].find((a) => text(a) === "New pull request");
  expect(press?.getAttribute("href")).toBe(compareHash(REPO, "main"));
});

test("a diff predicted at the checkout lists no commits: the source's check behind it brings them", async () => {
  drawCompare("main", null, {
    rangeDiff: async (repo: string, base: string, head: string) => {
      const answer = await rangeDiff(repo, base, head);
      return { ...answer, range: { ...answer.range!, commits: [] }, read_at: "checkout", predicted_at: "2026-11-13T09:00:00Z" };
    },
    sourceRangeDiff: rangeDiff,
  });
  await settle();
  expect([...target.querySelectorAll(".tf-side .tf-r .tf-t")].map(text)).toEqual(["Retry with backoff", "Bump deps"]);
  expect(text(target.querySelector(".tf-cdiff .tf-cnote"))).toBe("Read from your clone: the same as on the code host.");
});
