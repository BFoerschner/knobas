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
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import { forgetEchoes } from "../queued.svelte";
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
    {
      side: "new",
      line: 21,
      outdated: false,
      comments: [{ author: "jonas", body: "Should the jitter be bounded?", at: null }],
      id: "4088",
      resolved_by: null,
      replies: true,
    },
    { side: "new", line: 8, outdated: true, comments: [{ author: "priya", body: "Ten attempts is **a lot**.", at: null }], id: "4060", resolved_by: null, replies: false },
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
        blocks: [{ type: "diff", files: [RETRY, NOTE], truncated: false, commentable: commentable ? { write_op: "submit_review", reply: null, resolve: null, head: null } : null, gone: [] }],
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
  sent = [];
  refreshed = [];
  stored = new Map();
});

afterEach(() => {
  forgetEchoes();
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

/**
 * The ticks as the app keeps them (`knobas_core::viewed`, #2276): by the
 * blob a file held when ticked, or by the head where its diff named none.
 */
let stored: Map<string, string | null>;
const viewedNow = (blobs: Record<string, string>) =>
  [...stored].filter(([path, blob]) => (blob === null ? !(path in blobs) : blobs[path] === blob)).map(([path]) => path).sort();

/** What `submit_write` was handed, and the `item:refreshed` handler the page listens with. */
let sent: WriteOpPayload[];
let refreshed: ((event: { payload: unknown }) => void)[];

async function draw(answer: SourceView | (() => SourceView) = filesView(), writes = false) {
  for (const path of viewed) if (!stored.has(path)) stored.set(path, null);
  view = mount(TfPrFiles, {
    target,
    props: {
      prId: PR,
      entityView: async (_id: string, section: string | null) => {
        asked.push(section);
        return typeof answer === "function" ? answer() : answer;
      },
      viewedFiles: async (_id: string, blobs: Record<string, string>) => viewedNow(blobs),
      setViewed: async (_id: string, path: string, on: boolean, blobs: Record<string, string>) => {
        if (on) stored.set(path, blobs[path] ?? null);
        else stored.delete(path);
        viewed = viewedNow(blobs);
        return [...viewed];
      },
      pending,
      submitWrite: writes
        ? async (payload: WriteOpPayload) => {
            sent.push(payload);
            return { id: sent.length, state: "pending" } as QueuedWrite;
          }
        : null,
      listen: async (event: string, handler: (event: { payload: unknown }) => void) => {
        if (event === "item:refreshed") refreshed.push(handler);
        return () => {};
      },
      follower: { follow: () => () => {} },
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

/** The files at a head: each file's blob, and the commentable diff's head, as Gitea's *Files* section answers. */
function at(head: string, blobs: { retry: string; note: string }, threads: DiffFile["threads"] = RETRY.threads, resolve = true): SourceView {
  const base = filesView();
  const block = base.sections[0]!.blocks[0]!;
  if (block.type !== "diff") throw new Error("a diff");
  block.files = [
    { ...RETRY, blob: blobs.retry, threads },
    { ...NOTE, blob: blobs.note },
  ];
  block.commentable = { write_op: "submit_review", reply: "reply_to_thread", resolve: resolve ? "resolve_thread" : null, head };
  return base;
}

const viewedBox = (n: number) => cards()[n]!.querySelector<HTMLInputElement>(".tf-fh input[type=checkbox]")!;

test("#2276: after a new commit that changes one file, only that file loses Viewed", async () => {
  await draw(at("c90d11", { retry: "1111111", note: "2222222" }));
  viewedBox(0).click();
  await settle();
  viewedBox(1).click();
  await settle();
  expect(list()).toEqual(["Abackoff.md+1 −0 ✓", "Mretry.rs+2 −1 ✓"]);
  unmount(view!);
  view = null;

  // A push changes retry.rs and leaves the note as it was.
  await draw(at("e41a07", { retry: "3333333", note: "2222222" }));
  expect(list()).toEqual(["Abackoff.md+1 −0 ✓", "Mretry.rs+2 −1"]);
});

test("#2277: Reply… queues a reply on the thread's line, drawn queued until the read after it holds it", async () => {
  let reply: string | null = null;
  const thread = () => [
    {
      ...RETRY.threads[0]!,
      comments: [...RETRY.threads[0]!.comments, ...(reply === null ? [] : [{ author: "knobas", body: reply, at: null }])],
    },
    RETRY.threads[1]!,
  ];
  await draw(() => at("c90d11", { retry: "1111111", note: "2222222" }, thread()), true);
  const card = cards()[0]!;
  [...card.querySelectorAll<HTMLButtonElement>(".tf-thread button")].find((b) => b.textContent === "Reply…")!.click();
  flushSync();
  const box = card.querySelector<HTMLTextAreaElement>(".tf-thread textarea")!;
  box.value = "Bounded to ±10 % now.";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...card.querySelectorAll<HTMLButtonElement>(".tf-thread button")].find((b) => b.textContent === "Reply")!.click();
  await settle();
  expect(sent).toEqual([
    {
      ReplyToThread: {
        entity: PR,
        thread: "4088",
        comment: { path: "src/sepa/retry.rs", line: 21, side: "new", body: "Bounded to ±10 % now." },
      },
    },
  ]);
  expect(squash(card.querySelector(".tf-thread"))).toBe("jonas Should the jitter be bounded? You queued Bounded to ±10 % now. Reply… Resolve");

  // The write landed: its refresh reads the files again, and the reply is the source's now.
  reply = "Bounded to ±10 % now.";
  for (const hear of refreshed) hear({ payload: { entity_id: PR, write_id: 1 } });
  await settle();
  expect(asked).toEqual(["files", "files"]);
  expect(squash(cards()[0]!.querySelector(".tf-thread"))).toBe("jonas Should the jitter be bounded? knobas Bounded to ±10 % now. Reply… Resolve");
});

test("#2277: Resolve queues the resolve and collapses the thread; a resolved thread reads collapsed, and Forgejo's offers none", async () => {
  await draw(at("c90d11", { retry: "1111111", note: "2222222" }), true);
  const card = cards()[0]!;
  [...card.querySelectorAll<HTMLButtonElement>(".tf-thread button")].find((b) => b.textContent === "Resolve")!.click();
  await settle();
  expect(sent).toEqual([{ ResolveThread: { entity: PR, thread: "4088", resolved: true } }]);
  expect(squash(card.querySelector(".tf-thread"))).toBe("Resolved 1 comment Show Unresolve");
  unmount(view!);
  view = null;
  forgetEchoes();

  // Read again with the source's state: resolved by mara, collapsed with no write of ours.
  const resolved = [{ ...RETRY.threads[0]!, resolved_by: "mara" }, RETRY.threads[1]!];
  await draw(at("c90d11", { retry: "1111111", note: "2222222" }, resolved), true);
  expect(squash(cards()[0]!.querySelector(".tf-thread"))).toBe("Resolved by mara · 1 comment Show Unresolve");
  unmount(view!);
  view = null;

  // A source that cannot resolve (Forgejo) offers a reply and no resolve.
  await draw(at("c90d11", { retry: "1111111", note: "2222222" }, RETRY.threads, false), true);
  expect(squash(cards()[0]!.querySelector(".tf-thread"))).toBe("jonas Should the jitter be bounded? Reply…");
});
