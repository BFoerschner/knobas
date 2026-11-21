/**
 * An open pull request's *Files* before its source answers (ADR-0042): the
 * diff between the two branches' tips the mirror holds, read at the clone,
 * drawn at once and said to be so; the source's answer replaces it, and a
 * source that does not answer is named over it.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile, RangeDiff, SourceView } from "../../ipc/entity";
import { createPendingComments } from "../../detail/view/pending-comments.svelte";
import TfPrFiles from "./TfPrFiles.svelte";

const REPO = "gitea:tidewater/payout-service";
const PR = `${REPO}#142`;
const PREDICT = { repoId: REPO, base: "main", head: "feature/PAY-231-sepa-retry" };

const file = (path: string): DiffFile => ({
  path,
  old_path: null,
  change: "added",
  additions: 1,
  deletions: 0,
  hunks: [{ header: "@@ -0,0 +1 @@", lines: [{ kind: "add", old: null, new: 1, text: "# Backoff" }] }],
  truncated: false,
  threads: [],
});

const LOCAL: RangeDiff = { files: [file("docs/backoff.md")], truncated: false, range: null, read_at: "checkout", predicted_at: "2026-11-09T10:00:00Z" };

function sourceView(): SourceView {
  return {
    sections: [
      {
        id: "files",
        title: "Files",
        count: 2,
        deferred: true,
        blocks: [{ type: "diff", files: [file("docs/backoff.md"), file("src/sepa/retry.rs")], truncated: false, commentable: null, gone: [] }],
      },
    ],
    actions: [],
  } as SourceView;
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let n = 0; n < 4; n += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  flushSync();
}

function draw(files: Promise<SourceView>, local: RangeDiff = LOCAL) {
  view = mount(TfPrFiles, {
    target,
    props: {
      prId: PR,
      entityView: () => files,
      rangeDiff: async () => local,
      viewedFiles: async () => [],
      setViewed: async () => [],
      pending: createPendingComments(() => null),
      sourceName: "Tidewater Git",
      predict: PREDICT,
      listen: async () => () => {},
      follower: { follow: () => () => {} },
    },
  });
}

const squash = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const paths = () => [...target.querySelectorAll<HTMLElement>(".tf-diff")].map((card) => card.dataset["file"] ?? "");

test("the clone's diff draws while the source's is out, said to be so, and the source's replaces it", async () => {
  let answer: (view: SourceView) => void = () => {};
  draw(new Promise((resolve) => (answer = resolve)));
  await settle();
  expect(squash(target.querySelector(".tf-heldnote"))).toBe("From your clone, between the two branches as of the last sync. Reading Tidewater Git…");
  expect(paths()).toEqual(["docs/backoff.md"]);

  answer(sourceView());
  await settle();
  expect(target.querySelector(".tf-heldnote")).toBeNull();
  expect(paths().sort()).toEqual(["docs/backoff.md", "src/sepa/retry.rs"]);
});

test("a source that does not answer is named over the clone's diff, which stays", async () => {
  let fail: (error: unknown) => void = () => {};
  draw(new Promise((_, reject) => (fail = reject)));
  await settle();
  fail({ code: "unreachable", message: "connection refused" });
  await settle();
  expect(squash(target.querySelector(".tf-heldnote"))).toContain("Tidewater Git did not answer");
  expect(paths()).toEqual(["docs/backoff.md"]);
});

test("a diff the clone cannot read is not drawn as one", async () => {
  draw(new Promise(() => {}), { ...LOCAL, read_at: "source" });
  await settle();
  expect(target.querySelector(".tf-heldnote")).toBeNull();
  expect(paths()).toEqual([]);
});

test("the clone's diff is read a file at a time where the page can: its list first, each file's lines as it is reached", async () => {
  const pages: string[] = [];
  const cancelled: string[] = [];
  view = mount(TfPrFiles, {
    target,
    props: {
      prId: PR,
      entityView: () => new Promise<SourceView>(() => {}),
      rangeDiff: () => Promise.reject(new Error("the whole diff is never read")),
      rangeFiles: async () => ({ ...LOCAL, files: LOCAL.files.map((f) => ({ ...f, hunks: [] })) }),
      rangeFile: async (_repo: string, _base: string, _head: string, path: string, _old: string | null, _ws: boolean, from: number) => {
        pages.push(path);
        return { file: file(path), from, continues: false, next: null };
      },
      cancelRead: async (read: string) => {
        cancelled.push(read);
        return true;
      },
      viewedFiles: async () => [],
      setViewed: async () => [],
      pending: createPendingComments(() => null),
      sourceName: "Tidewater Git",
      predict: PREDICT,
      listen: async () => () => {},
      follower: { follow: () => () => {} },
    },
  });
  await settle();
  expect(paths()).toEqual(["docs/backoff.md"]);
  expect(pages).toEqual(["docs/backoff.md"]);
  expect(squash(target.querySelector('.tf-diff[data-file="docs/backoff.md"] .tf-dbody'))).toContain("# Backoff");
  unmount(view);
  view = null;
  expect(cancelled.length).toBeGreaterThan(0);
});
