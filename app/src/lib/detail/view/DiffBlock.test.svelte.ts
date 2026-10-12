/**
 * The diff block (#661, ADR-0020; `CONTEXT.md` **Diff**): a file list with
 * counts at the top, one collapsible card per file, two line-number gutters,
 * and a cut file that says so and offers the source's own page.
 *
 * Every string is source text: a line of a diff is drawn as text, never as
 * markup (rule 5).
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";

import type { DiffFile } from "../../ipc/entity";
import DiffBlock from "./DiffBlock.svelte";

function file(over: Partial<DiffFile> & { path: string }): DiffFile {
  return {
    old_path: null,
    change: "modified",
    additions: 0,
    deletions: 0,
    hunks: [],
    truncated: false,
    ...over,
  };
}

const MODIFIED = file({
  path: "src/retry.rs",
  additions: 2,
  deletions: 1,
  hunks: [
    {
      header: "@@ -1,3 +1,4 @@ pub fn backoff() {",
      lines: [
        { kind: "context", old: 1, new: 1, text: "    let base = 30;" },
        { kind: "del", old: 2, new: null, text: "    base * attempts" },
        { kind: "add", old: null, new: 2, text: "    let jitter = 3;" },
        { kind: "add", old: null, new: 3, text: "    base * attempts + <b>jitter</b>" },
        { kind: "context", old: 3, new: 4, text: "}" },
      ],
    },
  ],
});

const RENAMED = file({
  path: "docs/retry.md",
  old_path: "docs/backoff.md",
  change: "renamed",
});

function render(files: DiffFile[], over: { truncated?: boolean; webUrl?: string | null } = {}) {
  const target = document.createElement("div");
  document.body.append(target);
  const opened: string[] = [];
  const app = mount(DiffBlock, {
    target,
    props: {
      files,
      truncated: over.truncated ?? false,
      webUrl: over.webUrl === undefined ? "https://gitea.example/c/c90d11" : over.webUrl,
      sourceName: "Tidewater Git",
      onopenurl: (url: string) => opened.push(url),
    },
  });
  flushSync();
  return {
    target,
    opened,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    cards: () => [...target.querySelectorAll<HTMLElement>(".df-file")],
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("the file list counts every file's additions and deletions, and the whole", () => {
  const screen = render([MODIFIED, file({ path: "CHANGELOG.md", change: "added", additions: 1 })]);

  const summary = screen.target.querySelector(".df-sum")!.textContent!.replace(/\s+/g, " ");
  expect(summary).toContain("2 files");
  expect(summary).toContain("+3");
  expect(summary).toContain("−1");

  const index = [...screen.target.querySelectorAll(".df-index li")].map((li) =>
    li.textContent!.replace(/\s+/g, " ").trim(),
  );
  expect(index).toEqual([
    expect.stringMatching(/src\/retry\.rs.*\+2.*−1/),
    expect.stringMatching(/CHANGELOG\.md.*\+1/),
  ]);
  screen.done();
});

test("a line carries both its numbers, its kind and its text, as text", () => {
  const screen = render([MODIFIED]);
  const rows = [...screen.target.querySelectorAll<HTMLElement>(".df-line")];
  const read = rows.map((row) => [
    row.dataset.kind,
    row.querySelector(".df-old")!.textContent!.trim(),
    row.querySelector(".df-new")!.textContent!.trim(),
    row.querySelector(".df-text")!.textContent,
  ]);
  expect(read).toEqual([
    ["context", "1", "1", "    let base = 30;"],
    ["del", "2", "", "    base * attempts"],
    ["add", "", "2", "    let jitter = 3;"],
    ["add", "", "3", "    base * attempts + <b>jitter</b>"],
    ["context", "3", "4", "}"],
  ]);
  // The angle brackets are characters on the line, not an element.
  expect(screen.target.querySelector(".df-text b")).toBeNull();
  expect(screen.text()).toContain("@@ -1,3 +1,4 @@ pub fn backoff() {");
  screen.done();
});

test("a rename names where the file came from and where it went", () => {
  const screen = render([RENAMED]);
  const head = screen.cards()[0]!.querySelector(".df-head")!.textContent!.replace(/\s+/g, " ");
  expect(head).toContain("docs/backoff.md");
  expect(head).toContain("→");
  expect(head).toContain("docs/retry.md");
  expect(head.toLowerCase()).toContain("renamed");
  // A move with no edits has no lines, and says so rather than drawing nothing.
  expect(screen.cards()[0]!.textContent).toContain("Renamed without changes");
  screen.done();
});

test("a file collapses and expands from its header, and all of them at once", () => {
  const screen = render([MODIFIED, file({ path: "b.txt", change: "added", additions: 1, hunks: [
    { header: "@@ -0,0 +1 @@", lines: [{ kind: "add", old: null, new: 1, text: "x" }] },
  ] })]);
  const toggle = () => screen.cards()[0]!.querySelector<HTMLButtonElement>(".df-head")!;
  expect(toggle().getAttribute("aria-expanded")).toBe("true");
  expect(screen.cards()[0]!.querySelectorAll(".df-line").length).toBe(5);

  toggle().click();
  flushSync();
  expect(toggle().getAttribute("aria-expanded")).toBe("false");
  expect(screen.cards()[0]!.querySelectorAll(".df-line").length).toBe(0);
  expect(screen.cards()[1]!.querySelectorAll(".df-line").length, "only the one clicked").toBe(1);

  toggle().click();
  flushSync();
  expect(screen.cards()[0]!.querySelectorAll(".df-line").length).toBe(5);

  const all = [...screen.target.querySelectorAll<HTMLButtonElement>(".df-sum button")].find((b) =>
    b.textContent!.includes("Collapse all"),
  )!;
  all.click();
  flushSync();
  expect(screen.target.querySelectorAll(".df-line").length).toBe(0);
  screen.done();
});

test("a cut file says so and offers the source's own page", () => {
  const cut = file({ ...MODIFIED, path: "package-lock.json", additions: 4000, truncated: true });
  const screen = render([cut]);
  const card = screen.cards()[0]!;
  expect(card.textContent).toContain("knobas shows the first");
  const open = [...card.querySelectorAll<HTMLButtonElement>("button")].find((b) =>
    b.textContent!.includes("Open in Tidewater Git"),
  )!;
  open.click();
  expect(screen.opened).toEqual(["https://gitea.example/c/c90d11"]);
  // Counts stay whole however much was cut.
  expect(card.querySelector(".df-head")!.textContent).toContain("+4000");
  screen.done();
});

test("a view cut at its cap says so once, at the top", () => {
  const screen = render([MODIFIED], { truncated: true });
  expect(screen.target.querySelector(".df-cut")?.textContent).toMatch(/files after the cut/);
  screen.done();
});

test("with no page to offer, a cut file still says it was cut", () => {
  const screen = render([file({ ...MODIFIED, truncated: true })], { webUrl: null });
  expect(screen.cards()[0]!.textContent).toContain("knobas shows the first");
  expect(screen.cards()[0]!.textContent).not.toContain("Open in");
  screen.done();
});

test("a binary file draws no lines and says what it is", () => {
  const screen = render([file({ path: "logo.png", change: "binary" })]);
  expect(screen.cards()[0]!.textContent).toContain("Binary file");
  screen.done();
});

test("the file list jumps to its file", () => {
  const screen = render([MODIFIED, RENAMED]);
  const target = screen.cards()[1]!;
  let scrolled = false;
  target.scrollIntoView = () => {
    scrolled = true;
  };
  [...screen.target.querySelectorAll<HTMLButtonElement>(".df-index button")][1]!.click();
  expect(scrolled).toBe(true);
  screen.done();
});
