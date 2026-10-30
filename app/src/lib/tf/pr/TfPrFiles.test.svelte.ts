/**
 * *Files changed* (#842, R-8.17), mounted over stand-in reads shaped like
 * `knobas-source-gitea`'s *Files* section (`view/pr.rs`): the file list and
 * its filter, *Viewed* through its two commands, threads under their lines
 * and the outdated apart, and a `+` that keeps a comment pending.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile, DiffRange, SourceView } from "../../ipc/entity";
import { createPendingComments, type PendingCommentStore } from "../../detail/view/pending-comments.svelte";
import TfPrFiles from "./TfPrFiles.svelte";

const PR = "gitea:tidewater/payout-service#142";

const RETRY: DiffFile = {
  path: "src/sepa/retry.rs",
  old_path: null,
  change: "modified",
  additions: 2,
  deletions: 1,
  hunks: [
    {
      header: "@@ -1,3 +1,4 @@",
      lines: [
        { kind: "context", old: 1, new: 1, text: "use std::time::Duration;" },
        { kind: "del", old: 2, new: null, text: "pub const JITTER: f64 = 0.2;" },
        { kind: "add", old: null, new: 2, text: "pub const JITTER: f64 = 0.1;" },
        { kind: "context", old: 3, new: 3, text: "" },
      ],
    },
    { header: "@@ -20,2 +21,3 @@", lines: [{ kind: "add", old: null, new: 21, text: "let jitter = rng.gen_range(-JITTER..JITTER);" }] },
  ],
  truncated: false,
  threads: [
    { side: "new", line: 21, outdated: false, comments: [{ author: "jonas", body: "Should the jitter be bounded?", at: null }] },
    { side: "new", line: 8, outdated: true, comments: [{ author: "priya", body: "Ten attempts is **a lot**.", at: null }] },
  ],
};
const NOTE: DiffFile = {
  path: "docs/backoff.md",
  old_path: null,
  change: "added",
  additions: 1,
  deletions: 0,
  hunks: [{ header: "@@ -0,0 +1 @@", lines: [{ kind: "add", old: null, new: 1, text: "# Backoff" }] }],
  truncated: false,
  threads: [],
};

function filesView(commentable = true): SourceView {
  return {
    sections: [
      {
        id: "files",
        title: "Files",
        count: 2,
        deferred: false,
        blocks: [{ type: "diff", files: [RETRY, NOTE], truncated: false, commentable: commentable ? { write_op: "submit_review" } : null, gone: [] }],
      },
    ],
    actions: [],
  };
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let pending: PendingCommentStore;
let viewed: string[];
let asked: (string | null)[];

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  pending = createPendingComments(() => null);
  viewed = [];
  asked = [];
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function draw(answer: SourceView = filesView(), others: Record<string, SourceView> = {}) {
  view = mount(TfPrFiles, {
    target,
    props: {
      prId: PR,
      entityView: async (id: string, section: string | null) => {
        asked.push(id === PR ? section : `${id} ${section}`);
        return (section !== null && others[section]) || answer;
      },
      viewedFiles: async () => [...viewed],
      setViewed: async (_id: string, path: string, on: boolean) => {
        viewed = on ? [...viewed, path].sort() : viewed.filter((p) => p !== path);
        return [...viewed];
      },
      pending,
    },
  });
  await settle();
}

async function settle() {
  for (let n = 0; n < 4; n += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  flushSync();
}

const squash = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const list = () => [...target.querySelectorAll(".tf-flist button")].map(squash);
const cards = () => [...target.querySelectorAll<HTMLElement>(".tf-diff")];

test("the Files section is read when the tab opens; the list names each file with its counts", async () => {
  await draw();
  expect(asked).toEqual(["files"]);
  // The tree (#2268): directories by name, each file with its badge.
  expect(list()).toEqual(["Abackoff.md+1 −0", "Mretry.rs+2 −1"]);
  expect(cards().map((card) => squash(card.querySelector(".tf-fh")))).toEqual([
    "M src/sepa/retry.rs +2 −1 Viewed",
    // An added Markdown file draws rendered from its hunk (#2279): the mode switch sits before Viewed.
    "A docs/backoff.md +1 −0 RenderedLines Viewed",
  ]);
  // One hunk after another: a ⋯ row between them.
  expect(cards()[0]!.querySelectorAll(".tf-hunk")).toHaveLength(1);
});

test("the filter narrows the list and the diffs; nothing matching says so", async () => {
  await draw();
  const filter = target.querySelector<HTMLInputElement>("input[aria-label='Filter changed files']")!;
  filter.value = "BACKOFF";
  filter.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(list()).toEqual(["Abackoff.md+1 −0"]);
  expect(cards()).toHaveLength(1);
  filter.value = "nothing";
  filter.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(squash(target.querySelector(".tf-flist"))).toBe("No file matches.");
});

test("Viewed collapses a file and ticks it in the list, through the command; unticked, it opens again", async () => {
  viewed = ["docs/backoff.md"];
  await draw();
  expect(list()).toEqual(["Abackoff.md+1 −0 ✓", "Mretry.rs+2 −1"]);
  expect(cards()[1]!.classList.contains("tf-viewed")).toBe(true);
  expect(cards()[1]!.querySelectorAll(".tf-ln")).toHaveLength(0);
  const box = cards()[0]!.querySelector<HTMLInputElement>(".tf-fh input[type=checkbox]")!;
  box.click();
  await settle();
  expect(viewed).toEqual(["docs/backoff.md", "src/sepa/retry.rs"]);
  expect(cards()[0]!.querySelectorAll(".tf-ln")).toHaveLength(0);
  cards()[0]!.querySelector<HTMLInputElement>(".tf-fh input[type=checkbox]")!.click();
  await settle();
  expect(viewed).toEqual(["docs/backoff.md"]);
  expect(cards()[0]!.querySelectorAll(".tf-ln").length).toBeGreaterThan(0);
});

test("a thread sits under its line; an outdated one is kept apart at the end of its file", async () => {
  await draw();
  const retry = cards()[0]!;
  const thread = retry.querySelector(".tf-thread")!;
  expect(squash(thread.previousElementSibling)).toBe("21+ let jitter = rng.gen_range(-JITTER..JITTER); +");
  // The author line and the Markdown body are two blocks since #2261, so no colon joins them.
  expect(squash(thread)).toBe("jonas Should the jitter be bounded?");
  const apart = retry.querySelector("details.tf-outdated")!;
  expect(apart.hasAttribute("open")).toBe(false);
  expect(squash(apart)).toBe("1 outdated line 8priya Ten attempts is a lot.");
  // A line comment's body is the source's Markdown (#2261).
  expect(apart.querySelector(".tf-thread .tf-md strong")?.textContent).toBe("a lot");
});

test("+ on a removed line opens a box on its old side; Add to review keeps it pending under the line", async () => {
  await draw();
  const removed = [...cards()[0]!.querySelectorAll(".tf-ln")].find((ln) => ln.classList.contains("tf-del"))!;
  const plus = removed.querySelector<HTMLButtonElement>(".tf-addc")!;
  expect(plus.title).toBe("Comment on old line 2");
  plus.click();
  flushSync();
  const box = target.querySelector<HTMLTextAreaElement>(".tf-newc textarea")!;
  expect(box.placeholder).toBe("Comment on old line 2");
  box.value = "Why 0.2 before?";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-newc button")].find((b) => b.textContent === "Add to review")!.click();
  flushSync();
  expect(pending.of(PR).map(({ path, side, line, body }) => ({ path, side, line, body }))).toEqual([
    { path: "src/sepa/retry.rs", side: "old", line: 2, body: "Why 0.2 before?" },
  ]);
  expect(target.querySelector(".tf-newc")).toBeNull();
  const mine = target.querySelector(".tf-thread.tf-pending")!;
  expect(squash(mine)).toBe("You pending Why 0.2 before?");
  expect(mine.previousElementSibling).toBe(removed);
});

test("a diff the source takes no comments on draws no +", async () => {
  await draw(filesView(false));
  expect(target.querySelector(".tf-addc")).toBeNull();
  expect(target.querySelectorAll(".tf-diff .tf-ln").length).toBeGreaterThan(0);
});

// #2273, #2310: the commit picker, since my last review, and expanded context at the range's shas.

const REPO = "gitea:tidewater/payout-service";
const HEAD = "1262a5b610480702cfed4a1f3f76c87bf68fae55";
const MID = "08232cb289a2d05c5813b08948b2063901c33ba7";
const FIRST = "97cb5a8667f8aac8e6cdaeff83ffec9574f56a4b";
const MERGE_BASE = "907900b245f2941018dfd3ee155b876ef701c102";
const RANGE: DiffRange = {
  repository: REPO,
  base: MERGE_BASE,
  head: HEAD,
  commits: [
    { sha: HEAD, title: "Jitter in backoff", meta: "mara · 5 h ago" },
    { sha: MID, title: "Backoff jitter", meta: "mara · 1 d ago" },
    { sha: FIRST, title: "Retry SEPA payouts", meta: "mara · 2 d ago" },
  ],
  reviewed: FIRST,
};

function rangedView(range: DiffRange | undefined = RANGE): SourceView {
  const answer = filesView();
  const block = answer.sections[0]!.blocks[0]!;
  if (block.type === "diff" && range) block.range = range;
  return answer;
}

/** A compare's answer: one file, its own range. */
function compareView(base: string, head: string): SourceView {
  return {
    sections: [{ id: `compare:${base}...${head}`, title: "Diff", count: 1, deferred: false, blocks: [{ type: "diff", files: [NOTE], truncated: false, commentable: null, gone: [] }] }],
    actions: [],
  };
}

const face = () => target.querySelector<HTMLButtonElement>(".tf-pick-face");
const options = () => [...target.querySelectorAll<HTMLElement>(".tf-pick .tf-menu [role^=menuitem]")];
const option = (label: RegExp) => options().find((b) => label.test(squash(b)));
const plus = () => target.querySelectorAll(".tf-addc").length;

test("the picker names all commits; one commit shows only that commit's changes, and offers no line comments", async () => {
  await draw(rangedView(), { [`compare:${MID}^...${MID}`]: compareView(`${MID}^`, MID) });
  expect(squash(face())).toMatch(/Commits All 3/);
  expect(plus()).toBeGreaterThan(0);
  face()!.click();
  flushSync();
  option(/08232cb/)!.click();
  await settle();
  expect(asked).toContain(`${REPO} compare:${MID}^...${MID}`);
  expect(list()).toEqual([expect.stringMatching(/backoff\.md/)]);
  // Gitea takes a review at one commit; knobas sends one at the head, so a line is commented on the whole diff only.
  expect(plus()).toBe(0);
  expect(squash(face())).toMatch(/08232cb/);
  // Back to all commits: the pull request's own diff, comments and all.
  option(/All commits/)!.click();
  await settle();
  expect(list()).toHaveLength(2);
  expect(plus()).toBeGreaterThan(0);
});

test("since my last review shows the changes between the reviewed head and the head now", async () => {
  await draw(rangedView(), { [`compare:${FIRST}...${HEAD}`]: compareView(FIRST, HEAD) });
  face()!.click();
  flushSync();
  expect(squash(option(/Since your last review/))).toMatch(/You reviewed 97cb5a8 · 2 commits since/);
  option(/Since your last review/)!.click();
  await settle();
  expect(asked).toContain(`${REPO} compare:${FIRST}...${HEAD}`);
  expect(list()).toEqual([expect.stringMatching(/backoff\.md/)]);
  expect(plus()).toBe(0);
});

test("a viewer who never reviewed the pull request is not offered since my last review", async () => {
  await draw(rangedView({ ...RANGE, reviewed: undefined }));
  face()!.click();
  flushSync();
  expect(option(/All commits/)).toBeDefined();
  expect(option(/Since your last review/)).toBeUndefined();
});

test("a diff that names no range draws no picker", async () => {
  await draw();
  expect(face()).toBeNull();
});

test("context expands between hunks, read whole at the head in the range's repository (#2310)", async () => {
  const whole = Array.from({ length: 30 }, (_, n) => `line ${n + 1}`).join("\n");
  const fileAt: SourceView = {
    sections: [{ id: "f", title: "f", count: null, deferred: false, blocks: [{ type: "file", trail: [], path: RETRY.path, text: whole, language: null, size: whole.length, truncated: false, binary: false, url: null, sha: null } as never] }],
    actions: [],
  };
  await draw(rangedView(), { [`file@${HEAD}:${RETRY.path}`]: fileAt, [`file@${MERGE_BASE}:${RETRY.path}`]: fileAt });
  const expand = target.querySelector<HTMLButtonElement>(".tf-exp");
  expect(expand).not.toBeNull();
  expand!.click();
  await settle();
  expect(asked).toContain(`${REPO} file@${HEAD}:${RETRY.path}`);
  // The hidden lines, old and new numbers beside each: old 4 is new 5, old 19 new 20.
  expect(squash(target.querySelector(".tf-diff"))).toMatch(/45 line 5 .*1920 line 20 21\+/);
});
