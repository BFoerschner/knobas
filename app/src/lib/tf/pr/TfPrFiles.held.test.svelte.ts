/**
 * *Files changed* drawn from what knobas held (the held-files pass of PR #2479, 2026-11-15; ADR-0042): the sync holds
 * each open pull request's *Files*, so a first open draws it at once under the held line, and the source's answer read
 * behind replaces it in place. A thread the source brings above the reader's line moves the page by as much, so the
 * line the reader is on stays where it was.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DiffFile, LineThread, ShownView, SourceView } from "../../ipc/entity";
import { createPendingComments } from "../../detail/view/pending-comments.svelte";
import { forgetEchoes } from "../queued.svelte";
import TfPrFiles from "./TfPrFiles.svelte";

const PR = "gitea:tidewater/payout-service#142";

/** A file of twelve changed lines, each added. */
function file(threads: LineThread[]): DiffFile {
  return {
    path: "src/sepa/retry.rs",
    old_path: null,
    change: "modified",
    additions: 12,
    deletions: 0,
    hunks: [
      {
        header: "@@ -1,0 +1,12 @@",
        lines: Array.from({ length: 12 }, (_, n) => ({ kind: "add" as const, old: null, new: n + 1, text: `line ${n + 1}` })),
      },
    ],
    truncated: false,
    threads,
  };
}

const thread = (line: number, id: string, body: string): LineThread => ({
  side: "new",
  line,
  outdated: false,
  comments: [{ author: "priya", body, at: null }],
  id,
  resolved_by: null,
  replies: true,
});

function filesView(threads: LineThread[]): SourceView {
  return {
    sections: [
      {
        id: "files",
        title: "Files",
        count: 1,
        deferred: false,
        blocks: [{ type: "diff", files: [file(threads)], truncated: false, commentable: null, gone: [] }],
      },
    ],
    actions: [],
  };
}

/** The rows `TfPrFiles` keeps the reader on, laid out 20 px each in document order, since jsdom lays nothing out. */
const ROWS = ".tf-hunk, .tf-ln, .tf-srow, .tf-thread";

let scroller: HTMLElement;
let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  scroller = document.createElement("div");
  scroller.className = "tf-main";
  target = document.createElement("div");
  scroller.appendChild(target);
  document.body.appendChild(scroller);
  vi.spyOn(Element.prototype, "getClientRects").mockReturnValue([new DOMRect()] as unknown as DOMRectList);
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    if (this === scroller) return new DOMRect(0, 0, 800, 100);
    const n = [...target.querySelectorAll(ROWS)].indexOf(this);
    return n < 0 ? new DOMRect(0, -scroller.scrollTop, 800, 0) : new DOMRect(0, n * 20 - scroller.scrollTop, 800, 20);
  });
});

afterEach(() => {
  forgetEchoes();
  if (view) unmount(view);
  view = null;
  scroller.remove();
  vi.restoreAllMocks();
});

async function settle() {
  for (let n = 0; n < 4; n += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  flushSync();
}

/** The first row on screen: its text and where it is. */
function onScreen(): { text: string; top: number } {
  for (const row of target.querySelectorAll(ROWS)) {
    const box = row.getBoundingClientRect();
    if (box.bottom > 0) return { text: row.textContent?.replace(/\s+/g, " ").trim() ?? "", top: box.top };
  }
  throw new Error("no row on screen");
}

const rowAt = (text: string) =>
  [...target.querySelectorAll(ROWS)].find((row) => row.textContent?.replace(/\s+/g, " ").trim() === text) ?? null;

test("files knobas held draw at once under the held line; the source's answer with a new thread above the reader lands without moving the reader's line", async () => {
  const fresh: ((event: { payload: unknown }) => void)[] = [];
  const old = new Date(Date.now() - 3 * 3_600_000).toISOString();
  let reads = 0;
  view = mount(TfPrFiles, {
    target,
    props: {
      prId: PR,
      me: null,
      sourceName: "Tidewater Gitea",
      entityView: async (): Promise<ShownView> =>
        reads++ === 0
          ? { ...filesView([thread(10, "9", "Late one.")]), held_at: old }
          : filesView([thread(1, "8", "Why start here?"), thread(10, "9", "Late one.")]),
      rangeDiff: async () => {
        throw { code: "not_found", message: "no such compare" };
      },
      viewedFiles: async () => [],
      setViewed: async () => [],
      pending: createPendingComments(() => null),
      submitWrite: null,
      listen: async (event: string, handler: (event: { payload: unknown }) => void) => {
        if (event === "view:fresh") fresh.push(handler);
        return () => {};
      },
      follower: { follow: () => () => {} },
    },
  });
  await settle();
  expect(target.querySelector(".tf-heldnote")?.textContent).toBe("As knobas read it 3 h ago. Checking Tidewater Gitea…");
  expect(target.textContent).not.toContain("Why start here?");
  // The reader is on line 5, past line 1.
  scroller.scrollTop = 90;
  const was = onScreen();
  expect(was.text).toContain("line 5");

  for (const hear of fresh) hear({ payload: { entity_id: PR, section: "files" } });
  await settle();
  expect(reads).toBe(2);
  expect(target.textContent).toContain("Why start here?");
  // The thread came in above: the page moved by its row, and line 5 is where it was.
  expect(scroller.scrollTop).toBe(110);
  expect(rowAt(was.text)?.getBoundingClientRect().top).toBe(was.top);
  expect(target.querySelector(".tf-heldnote[role=status]")).toBeNull();
});
