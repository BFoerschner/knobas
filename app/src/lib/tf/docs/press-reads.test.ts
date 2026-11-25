/** A Docs page's reads go with the press on its link (Fable's ruling, 2026-11-25). */
import { afterEach, expect, test, vi } from "vitest";

import { ASKED_MS, docsPageOf, forgetDocsPressReads, installDocsPressReads, readDocsPageAhead } from "./press-reads";

afterEach(() => forgetDocsPressReads());

test("a page's link names its page, at a section or a mention too; nothing else does", () => {
  expect(docsPageOf("#/page/confluence%3A2")).toBe("confluence:2");
  expect(docsPageOf("#/page/confluence%3A2/3")).toBe("confluence:2");
  expect(docsPageOf("#/page/confluence%3A2/mention")).toBe("confluence:2");
  expect(docsPageOf("#/docs/space/OPS")).toBeNull();
  expect(docsPageOf("#/page/%E0%A4%A")).toBeNull();
});

test("the page's text and row are asked once a press, no visit", () => {
  const reads = { page: vi.fn(async () => null), head: vi.fn(async () => null) };
  readDocsPageAhead("confluence:2", reads, 1000);
  readDocsPageAhead("confluence:2", reads, 1000 + ASKED_MS - 1);
  expect(reads.page.mock.calls).toEqual([["confluence:2"]]);
  expect(reads.head.mock.calls).toEqual([["confluence:2"]]);
  readDocsPageAhead("confluence:2", reads, 1000 + ASKED_MS);
  expect(reads.page).toHaveBeenCalledTimes(2);
});

test("a primary press or Enter on a page's link asks; another button, another key or another link does not", () => {
  const root = document.createElement("div");
  root.innerHTML = '<a href="#/page/confluence%3A9"><b>Retry</b></a><a href="#/repo/x">Repo</a>';
  document.body.append(root);
  const read = vi.fn();
  const stop = installDocsPressReads(root, read);
  const [page, repo] = [...root.querySelectorAll("a")];
  const down = (el: Element, button: number) => el.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, button }));
  down(page!.querySelector("b")!, 0);
  down(page!, 2);
  down(repo!, 0);
  page!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  page!.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true }));
  expect(read.mock.calls).toEqual([["confluence:9"], ["confluence:9"]]);
  stop();
  down(page!, 0);
  expect(read).toHaveBeenCalledTimes(2);
  root.remove();
});
