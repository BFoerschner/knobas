// A long text drawn a piece at a time (the render-cost pass, 2026-11-14) is the text drawn whole, cut between blocks.
import { expect, test } from "vitest";

import { markdownFragment, markdownPieces, type MarkdownLinks } from "./markdown";

const LINKS: MarkdownLinks = { base: "docs", fileLink: (path, line) => `#/repo/r/files/${path}${line === null ? "" : `@${line}`}` };

function html(fragment: DocumentFragment): string {
  const box = document.createElement("div");
  box.append(fragment);
  return box.innerHTML.replace(/>\s+</g, "><").trim();
}

const TEXT = [
  "# Notes",
  "See [the guide][guide] and [the guide][guide] again, and [a file](src/main.ts#L4).",
  "## Steps",
  "1. one\n2. two\n\n3. three after a blank line\n4. four",
  "- [x] done\n- [ ] not yet",
  "## Steps",
  "```ts\nconst a = 1;\n```",
  "| a | b |\n|---|---|\n| 1 | 2 |",
  "<details><summary>More</summary>\n\nInside the details.\n\n</details>",
  "## Steps",
  "Last paragraph with `code` and ~~gone~~.",
  "[guide]: https://example.com/guide",
].join("\n\n");

test("pieces of any size put together are the text drawn whole, heading ids counted across them", () => {
  const whole = html(markdownFragment(TEXT, LINKS));
  for (const budget of [1, 40, 120, Infinity]) {
    const pieces = markdownPieces(TEXT, LINKS);
    const parts: string[] = [];
    let n = 0;
    for (let piece = pieces.next(budget); piece !== null; piece = pieces.next(budget)) {
      parts.push(html(piece));
      n += 1;
    }
    expect(pieces.done()).toBe(true);
    expect(parts.join(""), `budget ${budget}`).toBe(whole);
    if (budget === 1) expect(n).toBeGreaterThan(8);
    if (budget === Infinity) expect(n).toBe(1);
  }
  expect(whole).toContain('id="user-content-steps-2"');
  expect(whole).toContain('href="https://example.com/guide"');
  expect(whole.match(/<ol>/g)).toHaveLength(1);
  expect(whole).toMatch(/<details><summary>More<\/summary><p>Inside the details.<\/p><\/details>/);
});
