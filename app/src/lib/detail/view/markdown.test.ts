/**
 * The Markdown a text block carries (a README, #663), read into blocks the
 * shell draws with real elements -- never into an HTML string (ADR-0020
 * rule 5). What a README is mostly made of: headings, paragraphs, lists,
 * code, quotes, rules, a table.
 */
import { expect, test } from "vitest";

import { inline, parseMarkdown } from "./markdown";

test("headings, both spellings, and the paragraph between them", () => {
  expect(
    parseMarkdown("# payout-service\n\nPays out\nSEPA batches.\n\nSetup\n-----\n### Deep ##"),
  ).toEqual([
    { kind: "heading", level: 1, inline: [{ kind: "text", text: "payout-service" }] },
    { kind: "paragraph", inline: [{ kind: "text", text: "Pays out SEPA batches." }] },
    { kind: "heading", level: 2, inline: [{ kind: "text", text: "Setup" }] },
    { kind: "heading", level: 3, inline: [{ kind: "text", text: "Deep" }] },
  ]);
});

test("a fenced block keeps its lines, its blank lines and its markup verbatim", () => {
  expect(parseMarkdown("```sh\ncargo run\n\n# not a heading\n<b>x</b>\n```\nafter")).toEqual([
    { kind: "code", text: "cargo run\n\n# not a heading\n<b>x</b>" },
    { kind: "paragraph", inline: [{ kind: "text", text: "after" }] },
  ]);
});

test("an unterminated fence runs to the end rather than swallowing nothing", () => {
  expect(parseMarkdown("~~~\nopen")).toEqual([{ kind: "code", text: "open" }]);
});

test("lists: bullets and numbers, nesting by indentation, a wrapped item joined", () => {
  expect(parseMarkdown("- one\n  wrapped\n- two\n  - nested\n\n3. three\n4) four")).toEqual([
    {
      kind: "list",
      ordered: false,
      start: 1,
      items: [
        { depth: 0, inline: [{ kind: "text", text: "one wrapped" }] },
        { depth: 0, inline: [{ kind: "text", text: "two" }] },
        { depth: 1, inline: [{ kind: "text", text: "nested" }] },
      ],
    },
    {
      kind: "list",
      ordered: true,
      start: 3,
      items: [
        { depth: 0, inline: [{ kind: "text", text: "three" }] },
        { depth: 0, inline: [{ kind: "text", text: "four" }] },
      ],
    },
  ]);
});

test("a quote, a rule, and a table kept as its aligned lines", () => {
  expect(parseMarkdown("> quoted\n> on\n\n---\n| a | b |\n|---|---|\n| 1 | 2 |")).toEqual([
    { kind: "quote", inline: [{ kind: "text", text: "quoted on" }] },
    { kind: "rule" },
    { kind: "table", text: "| a | b |\n|---|---|\n| 1 | 2 |" },
  ]);
});

test("four spaces of indentation is code", () => {
  expect(parseMarkdown("text\n\n    let x = 1;\n      y();\n\nmore")).toEqual([
    { kind: "paragraph", inline: [{ kind: "text", text: "text" }] },
    { kind: "code", text: "let x = 1;\n  y();" },
    { kind: "paragraph", inline: [{ kind: "text", text: "more" }] },
  ]);
});

test("inline code, emphasis and links; a link keeps its target", () => {
  expect(inline("run `just check`, **then** _push_ to [Gitea](https://gitea.example/x)")).toEqual([
    { kind: "text", text: "run " },
    { kind: "code", text: "just check" },
    { kind: "text", text: ", " },
    { kind: "strong", text: "then" },
    { kind: "text", text: " " },
    { kind: "em", text: "push" },
    { kind: "text", text: " to " },
    { kind: "link", text: "Gitea", href: "https://gitea.example/x" },
  ]);
});

test("snake_case stays text, an image is its alt text, and markup inside code is code", () => {
  expect(inline("a snake_case_name ![build badge](https://gitea.example/b.svg) `<b>`")).toEqual([
    { kind: "text", text: "a snake_case_name " },
    { kind: "image", text: "build badge" },
    { kind: "text", text: " " },
    { kind: "code", text: "<b>" },
  ]);
});

test("HTML in a README is text, never markup", () => {
  expect(parseMarkdown('<p align="center"><script>alert(1)</script></p>')).toEqual([
    {
      kind: "paragraph",
      inline: [{ kind: "text", text: '<p align="center"><script>alert(1)</script></p>' }],
    },
  ]);
});
