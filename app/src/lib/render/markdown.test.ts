// A long text drawn a piece at a time (the render-cost pass, 2026-11-14) is the text drawn whole, cut between blocks.
import { expect, test } from "vitest";

import { keepFenceRooms, keptFenceRooms, markdownFragment, markdownPieces, nodesScope, type MarkdownLinks } from "./markdown";

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

const fenced = (...codes: string[]) => codes.map((code) => `\`\`\`mermaid\n${code}\n\`\`\``).join("\n\nBetween.\n\n");
const bodiesOf = (root: ParentNode) => [...root.querySelectorAll<HTMLElement>(".tf-fence-body")];
const roomsOf = (root: ParentNode) => bodiesOf(root).map((body) => (body.classList.contains("tf-roomkept") ? body.style.height : null));

test("a text drawn again gives the fence in the same place with the same code the room it was kept at (#2510)", () => {
  const text = fenced("flowchart LR\n  a --> b", "flowchart LR\n  a --> b", "sequenceDiagram\n  a->>b: c");
  const drawn = document.createElement("div");
  drawn.append(markdownFragment(text, LINKS));
  expect(keptFenceRooms(drawn)).toEqual([]);
  // The second of two fences of one code was fitted into its source's room, and the third.
  for (const [at, height] of [[1, "125px"], [2, "146.4px"]] as const) {
    bodiesOf(drawn)[at]!.style.height = height;
    bodiesOf(drawn)[at]!.classList.add("tf-roomkept");
  }
  const rooms = keptFenceRooms(drawn);
  expect(rooms).toEqual([
    { at: 1, code: "flowchart LR\n  a --> b", height: "125px" },
    { at: 2, code: "sequenceDiagram\n  a->>b: c", height: "146.4px" },
  ]);

  const again = markdownFragment(text, LINKS);
  expect(keepFenceRooms(again, rooms)).toBe(3);
  expect(roomsOf(again)).toEqual([null, "125px", "146.4px"]);

  // A text that changed there keeps no room there: its second fence holds other code now.
  const changed = markdownFragment(fenced("flowchart LR\n  a --> b", "flowchart TB\n  x --> y", "sequenceDiagram\n  a->>b: c"), LINKS);
  keepFenceRooms(changed, rooms);
  expect(roomsOf(changed)).toEqual([null, null, "146.4px"]);

  // A long text is drawn a piece at a time: each piece counts on from the fences before it.
  const pieces = document.createElement("div");
  pieces.append(markdownFragment(text, LINKS));
  const [first, ...rest] = [...pieces.querySelectorAll(".tf-fence")];
  const from = keepFenceRooms(nodesScope([first!]), rooms);
  expect([from, roomsOf(pieces)]).toEqual([1, [null, null, null]]);
  expect(keepFenceRooms(nodesScope(rest), rooms, from)).toBe(3);
  expect(roomsOf(pieces)).toEqual([null, "125px", "146.4px"]);
});

// A rendered comparison's changed blocks (ux26, ruling 3): with `lines`, each top-level block says the source lines it
// came from; a nested block says none; the sanitizer keeps the attribute; without `lines` nothing is added.
test("with lines, each top-level block carries its source lines, through the sanitizer", () => {
  const text = "# Title\n\nA paragraph\nof two lines.\n\n- one\n- two\n  - nested\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```js\nlet x = 1;\n```\n";
  const box = document.createElement("div");
  box.append(markdownFragment(text, LINKS, { lines: true }));
  const said = [...box.children].map((el) => `${el.tagName.toLowerCase()} ${el.getAttribute("data-md-lines")}`);
  // A list's lines run to the blank line after it, as markdown-it maps it.
  expect(said).toEqual(["h1 1-1", "p 3-4", "ul 6-9", "table 10-12", "div 14-16"]);
  expect(box.querySelector("li")?.hasAttribute("data-md-lines"), "a nested block marks nothing").toBe(false);
  const plain = document.createElement("div");
  plain.append(markdownFragment(text, LINKS));
  expect(plain.querySelector("[data-md-lines]")).toBeNull();
});

test("an author's own data-md-lines in raw HTML marks at most its own block", () => {
  const box = document.createElement("div");
  box.append(markdownFragment('<div data-md-lines="1-99">raw</div>\n\nText', LINKS, { lines: true }));
  expect([...box.querySelectorAll("[data-md-lines]")].map((el) => el.getAttribute("data-md-lines"))).toEqual(["1-99", "3-3"]);
});
