/**
 * *Files changed* (#842, R-8.17), mounted over stand-in reads shaped like
 * `knobas-source-gitea`'s *Files* section (`view/pr.rs`): the file list and
 * its filter, *Viewed* through its two commands, threads under their lines
 * and the outdated apart, and a `+` that keeps a comment pending.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile, SourceView } from "../../ipc/entity";
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

async function draw(answer: SourceView = filesView()) {
  view = mount(TfPrFiles, {
    target,
    props: {
      prId: PR,
      entityView: async (_id: string, section: string | null) => {
        asked.push(section);
        return answer;
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
    "A docs/backoff.md +1 −0 Viewed",
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
