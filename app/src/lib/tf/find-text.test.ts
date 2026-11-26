/** `findIn`: what the find bar counts on a page it has no renderer for. */
import { afterEach, expect, test } from "vitest";

import { findIn, selectedWords } from "./find-text";

afterEach(() => {
  document.body.replaceChildren();
  getSelection()?.removeAllRanges();
});

function page(html: string): HTMLElement {
  const root = document.createElement("div");
  root.innerHTML = html;
  document.body.append(root);
  return root;
}

const said = (ranges: Range[]): string[] => ranges.map((range) => range.toString());

test("any case, literally, with any run of spaces matching any other", () => {
  const root = page("<p>Retry the retry.\n  a.b axb</p>");
  expect(said(findIn(root, "RETRY"))).toEqual(["Retry", "retry"]);
  expect(said(findIn(root, "retry. a"))).toEqual(["retry.\n  a"]);
  expect(said(findIn(root, "a.b"))).toEqual(["a.b"]);
  expect(findIn(root, "   ")).toEqual([]);
});

test("a word split by syntax colours is one match, and a block's edge ends it", () => {
  const root = page('<div class="tf-ln"><span>+ <span class="tf-hl-k">con</span>st JIT<b>TER</b></span></div><div>const</div><p>JIT</p><p>TER</p>');
  expect(said(findIn(root, "const JITTER"))).toEqual(["const JITTER"]);
  expect(said(findIn(root, "const"))).toEqual(["const", "const"]);
  expect(findIn(root, "JITTER")).toHaveLength(1);
});

test("line numbers, controls, what is hidden and a closed fold hold no words of the page", () => {
  const root = page(
    '<div><span class="tf-no">142</span><span>code 142</span></div><button>142</button><span aria-hidden="true">142</span><p hidden>142</p>' +
      '<details><summary>142 title</summary><p>142 folded</p></details><details open><summary>sum</summary><p>142 open</p></details><div class="tf-findhold">142</div>',
  );
  expect(findIn(root, "142").map((range) => range.startContainer.textContent)).toEqual(["code 142", "142 title", "142 open"]);
});

test("the selected text starts the box on one line, and a selection across lines doesn't", () => {
  const root = page("<pre>one every two\nthree</pre>");
  const words = root.querySelector("pre")!.firstChild as Text;
  expect(selectedWords()).toBeNull();
  getSelection()!.setBaseAndExtent(words, 4, words, 9);
  expect(selectedWords()?.words).toBe("every");
  getSelection()!.setBaseAndExtent(words, 4, words, 19);
  expect(selectedWords()).toBeNull();
});
