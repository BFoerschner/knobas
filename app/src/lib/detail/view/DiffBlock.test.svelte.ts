/**
 * The diff block (#661, ADR-0020; `CONTEXT.md` **Diff**): a file list with
 * counts at the top, one collapsible card per file, two line-number gutters,
 * and a cut file that says so and offers the source's own page.
 *
 * Every string is source text: a line of a diff is drawn as text, never as
 * markup (rule 5).
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { expect, test } from "vitest";

import type { DiffFile, FileThread } from "../../ipc/entity";
import DiffBlock from "./DiffBlock.svelte";
import { asReview, pendingComments } from "./pending-comments.svelte";

function file(over: Partial<DiffFile> & { path: string }): DiffFile {
  return {
    old_path: null,
    change: "modified",
    additions: 0,
    deletions: 0,
    hunks: [],
    truncated: false,
    threads: [],
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

const PR = "gitea:tidewater/payout-service#142";

function render(
  files: DiffFile[],
  over: { truncated?: boolean; webUrl?: string | null; canComment?: boolean; gone?: FileThread[] } = {},
) {
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
      entityId: PR,
      canComment: over.canComment ?? false,
      gone: over.gone ?? [],
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

// -- line comments (#709) ------------------------------------------------------

const AN_HOUR_AGO = new Date(Date.now() - 60 * 60_000).toISOString();

const REVIEWED = file({
  ...MODIFIED,
  threads: [
    {
      side: "old",
      line: 2,
      outdated: false,
      comments: [{ author: "priya.nair", body: "Why drop the `<b>constant</b>`?", at: AN_HOUR_AGO }],
    },
    {
      side: "new",
      line: 3,
      outdated: false,
      comments: [
        { author: "priya.nair", body: "Bound the **jitter**.", at: AN_HOUR_AGO },
        { author: "olu", body: "Agreed.", at: null },
      ],
    },
    {
      side: "new",
      line: 9,
      outdated: true,
      comments: [{ author: "olu", body: "Why 35?", at: AN_HOUR_AGO }],
    },
  ],
});

/** Each line's text, and the thread drawn after it: its authors and words. */
function layout(target: HTMLElement): string[] {
  return [...target.querySelectorAll<HTMLElement>(".df-lines > .df-line, .df-lines > .df-thread")].map((el) =>
    el.classList.contains("df-line")
      ? `line ${el.querySelector(".df-text")!.textContent}`
      : `thread ${[...el.querySelectorAll(".df-c")]
          .map((c) => `${c.querySelector(".df-c-who")!.textContent}: ${c.querySelector(".df-c-body")!.textContent!.trim()}`)
          .join(" / ")}`,
  );
}

function button(within: Element, words: string): HTMLButtonElement {
  const found = [...within.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent!.trim() === words || b.getAttribute("aria-label") === words,
  );
  if (!found) throw new Error(`no button ${words} in ${within.textContent}`);
  return found;
}

function type(area: HTMLTextAreaElement, words: string) {
  area.value = words;
  area.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

test("a thread is drawn under its line, by side: authors, age and words", () => {
  const screen = render([REVIEWED]);
  expect(layout(screen.target)).toEqual([
    "line     let base = 30;",
    "line     base * attempts",
    "thread priya.nair: Why drop the <b>constant</b>?",
    "line     let jitter = 3;",
    "line     base * attempts + <b>jitter</b>",
    "thread priya.nair: Bound the jitter. / olu: Agreed.",
    "line }",
  ]);
  const thread = screen.target.querySelectorAll(".df-lines > .df-thread")[1]!;
  expect(thread.textContent).toContain("1 h ago");
  // Markdown is drawn legibly, and still never as markup.
  expect(thread.querySelector(".df-c-body strong")?.textContent).toBe("jitter");
  expect(screen.target.querySelector(".df-c-body b")).toBeNull();
  expect(screen.target.querySelector(".df-c-body code")?.textContent).toBe("<b>constant</b>");
  screen.done();
});

test("an outdated thread is kept apart, at the end of its file, muted and closed", () => {
  const screen = render([REVIEWED]);
  const card = screen.cards()[0]!;
  expect(card.querySelector(".df-lines")!.textContent).not.toContain("Why 35?");
  const toggle = button(card, "1 outdated comment");
  expect(toggle.getAttribute("aria-expanded")).toBe("false");
  expect(card.textContent).not.toContain("Why 35?");
  toggle.click();
  flushSync();
  const outdated = card.querySelector(".df-outdated")!;
  expect(outdated.textContent).toContain("Why 35?");
  expect(outdated.textContent).toContain("line 9");
  screen.done();
});

test("the file list and the file's header count its comments", () => {
  const screen = render([REVIEWED, RENAMED]);
  expect(screen.target.querySelector(".df-index li")!.textContent).toMatch(/4/);
  expect(screen.cards()[0]!.querySelector(".df-head .df-ccount")!.textContent!.trim()).toBe("4");
  screen.done();
});

test("a diff that takes no comments offers none", () => {
  const screen = render([REVIEWED]);
  expect(screen.target.querySelector(".df-add")).toBeNull();
  screen.done();
});

/** Open the editor on the row whose text is `text`, and answer its text area. */
function openEditor(screen: ReturnType<typeof render>, text: string): HTMLTextAreaElement {
  const row = [...screen.target.querySelectorAll<HTMLElement>(".df-line")].find(
    (r) => r.querySelector(".df-text")!.textContent === text,
  )!;
  row.querySelector<HTMLButtonElement>(".df-add")!.click();
  flushSync();
  const area = screen.target.querySelector<HTMLTextAreaElement>(".df-editor textarea");
  if (!area) throw new Error("no editor");
  return area;
}

test("a comment written on a line is pending under it, on its side, until sent", () => {
  const screen = render([REVIEWED], { canComment: true });
  // A removed line is commented on the old side, by its old number.
  type(openEditor(screen, "    base * attempts"), "Keep the old name?");
  button(screen.target, "Add comment").click();
  flushSync();
  // An added line on the new side.
  type(openEditor(screen, "    let jitter = 3;"), "Name the 3.");
  button(screen.target, "Add comment").click();
  flushSync();

  expect(asReview(pendingComments.of(PR))).toEqual([
    { path: "src/retry.rs", side: "old", line: 2, body: "Keep the old name?" },
    { path: "src/retry.rs", side: "new", line: 2, body: "Name the 3." },
  ]);
  expect(layout(screen.target).slice(1, 6)).toEqual([
    "line     base * attempts",
    "thread priya.nair: Why drop the <b>constant</b>? / You: Keep the old name?",
    "line     let jitter = 3;",
    "thread You: Name the 3.",
    "line     base * attempts + <b>jitter</b>",
  ]);
  const pending = [...screen.target.querySelectorAll(".df-c.pending")];
  expect(pending.map((c) => c.textContent)).toEqual([expect.stringContaining("Pending"), expect.stringContaining("Pending")]);
  expect(screen.target.querySelector(".df-sum")!.textContent).toContain("2 pending");
  screen.done();
});

test("a pending comment is edited and discarded in place; a blank or cancelled one is none", () => {
  const screen = render([MODIFIED], { canComment: true });
  type(openEditor(screen, "}"), "first words");
  button(screen.target, "Add comment").click();
  flushSync();

  button(screen.target.querySelector(".df-c.pending")!, "Edit").click();
  flushSync();
  const area = screen.target.querySelector<HTMLTextAreaElement>(".df-editor textarea")!;
  expect(area.value).toBe("first words");
  type(area, "second words");
  button(screen.target, "Save").click();
  flushSync();
  expect(asReview(pendingComments.of(PR)).map((c) => c.body)).toEqual(["second words"]);
  expect(screen.target.querySelector(".df-c.pending")!.textContent).toContain("second words");

  button(screen.target.querySelector(".df-c.pending")!, "Discard").click();
  flushSync();
  expect(pendingComments.of(PR)).toEqual([]);
  expect(screen.target.querySelector(".df-thread")).toBeNull();

  // Cancel, and Escape, leave nothing behind.
  type(openEditor(screen, "}"), "never mind");
  button(screen.target, "Cancel").click();
  flushSync();
  type(openEditor(screen, "}"), "never mind");
  screen.target
    .querySelector(".df-editor textarea")!
    .dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  flushSync();
  expect(screen.target.querySelector(".df-editor")).toBeNull();
  // A blank comment is not added.
  type(openEditor(screen, "}"), "   ");
  button(screen.target, "Add comment").click();
  flushSync();
  expect(pendingComments.of(PR)).toEqual([]);
  screen.done();
});

test("Cmd+Enter adds the comment, as the button would", () => {
  const screen = render([MODIFIED], { canComment: true });
  const area = openEditor(screen, "}");
  type(area, "by keyboard");
  area.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", metaKey: true, bubbles: true }));
  flushSync();
  expect(asReview(pendingComments.of(PR)).map((c) => c.body)).toEqual(["by keyboard"]);
  screen.done();
});

test("a pending comment survives the diff being drawn again", () => {
  const first = render([MODIFIED], { canComment: true });
  type(openEditor(first, "}"), "do not lose me");
  button(first.target, "Add comment").click();
  flushSync();
  first.done();

  const again = render([MODIFIED], { canComment: true });
  expect(again.target.querySelector(".df-c.pending")?.textContent).toContain("do not lose me");
  again.done();
});

test("every pending comment is discarded at once, after asking", () => {
  const screen = render([MODIFIED], { canComment: true });
  for (const text of ["}", "    let jitter = 3;"]) {
    type(openEditor(screen, text), `on ${text}`);
    button(screen.target, "Add comment").click();
    flushSync();
  }
  const sum = screen.target.querySelector(".df-sum")!;
  button(sum, "Discard pending").click();
  flushSync();
  expect(pendingComments.of(PR)).toHaveLength(2);
  button(sum, "Keep").click();
  flushSync();
  expect(pendingComments.of(PR)).toHaveLength(2);
  button(sum, "Discard pending").click();
  flushSync();
  button(sum, "Discard 2").click();
  flushSync();
  expect(pendingComments.of(PR)).toEqual([]);
  expect(screen.target.querySelector(".df-c.pending")).toBeNull();
  screen.done();
});

test("a pending comment whose line this diff no longer has is kept apart, still pending", () => {
  pendingComments.add(PR, { path: "src/retry.rs", side: "new", line: 80, body: "a line since gone" });
  const screen = render([MODIFIED], { canComment: true });
  const card = screen.cards()[0]!;
  expect(card.querySelector(".df-lines")!.textContent).not.toContain("a line since gone");
  const outdated = card.querySelector(".df-outdated")!;
  expect(outdated.textContent).toContain("a line since gone");
  expect(outdated.querySelector(".df-c.pending")).not.toBeNull();
  screen.done();
});

test("a thread the source did not call outdated, on a line past the file's cut, is apart but not called outdated", () => {
  const cut = file({
    ...MODIFIED,
    truncated: true,
    threads: [{ side: "new", line: 900, outdated: false, comments: [{ author: "olu", body: "far down", at: null }] }],
  });
  const screen = render([cut]);
  const card = screen.cards()[0]!;
  button(card, "1 comment not on a line shown").click();
  flushSync();
  expect(card.querySelector(".df-outdated")!.textContent).toContain("line 900, past the cut");
  expect(card.textContent).not.toContain("outdated comment");
  screen.done();
});

/** Press `key` on the focused element, as a keyboard would. */
function press(key: string) {
  document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  flushSync();
}

test("a file's lines are one tab stop: Up and Down move along them, Enter or c comments", async () => {
  const screen = render([MODIFIED], { canComment: true });
  const adds = () => [...screen.target.querySelectorAll<HTMLButtonElement>(".df-add")];
  // One stop per file, on its first line, however many lines it has.
  expect(adds().map((b) => b.tabIndex)).toEqual([0, -1, -1, -1, -1]);

  adds()[0]!.focus();
  press("ArrowDown");
  press("ArrowDown");
  expect(document.activeElement).toBe(adds()[2]);
  press("ArrowUp");
  expect(document.activeElement).toBe(adds()[1]);
  // The stop follows the focus, so Tab back in lands where the reader was.
  expect(adds().map((b) => b.tabIndex)).toEqual([-1, 0, -1, -1, -1]);
  // Past either end, nowhere.
  press("ArrowUp");
  press("ArrowUp");
  expect(document.activeElement).toBe(adds()[0]);

  press("ArrowDown");
  press("c");
  const area = screen.target.querySelector<HTMLTextAreaElement>(".df-editor textarea")!;
  expect(area.getAttribute("aria-label")).toBe("Comment on old line 2");
  // Escape hands the focus back to the line it came from.
  area.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  flushSync();
  await tick();
  expect(document.activeElement).toBe(adds()[1]);
  screen.done();
});

test("a thread, or a pending comment, on a file this diff no longer holds is kept in the outdated list at its end", () => {
  pendingComments.add(PR, { path: "src/legacy.rs", side: "old", line: 4, body: "mine, on a dropped file" });
  const gone: FileThread[] = [
    {
      path: "src/legacy.rs",
      thread: { side: "new", line: 7, outdated: true, comments: [{ author: "olu", body: "A second retry loop?", at: null }] },
    },
  ];
  const screen = render([MODIFIED], { canComment: true, gone });
  const list = screen.target.querySelector(".df-gone")!;
  // After every file, and open: a pending comment is in it.
  expect(screen.target.querySelector(".df")!.lastElementChild).toBe(list);
  const toggle = button(list, "Outdated: 2 comments on files no longer in this diff");
  expect(toggle.getAttribute("aria-expanded")).toBe("true");
  const text = list.textContent!.replace(/\s+/g, " ");
  expect(text).toContain("src/legacy.rs, line 7");
  expect(text).toContain("A second retry loop?");
  expect(text).toContain("src/legacy.rs, old line 4, not in this diff");
  expect(list.querySelector(".df-c.pending")!.textContent).toContain("mine, on a dropped file");
  // Still the reader's to edit.
  button(list.querySelector(".df-c.pending")!, "Edit").click();
  flushSync();
  type(list.querySelector<HTMLTextAreaElement>(".df-editor textarea")!, "edited");
  button(list, "Save").click();
  flushSync();
  expect(asReview(pendingComments.of(PR))).toEqual([{ path: "src/legacy.rs", side: "old", line: 4, body: "edited" }]);
  screen.done();

  // With nothing pending on it, it starts closed.
  pendingComments.clear(PR);
  const quiet = render([MODIFIED], { gone });
  expect(button(quiet.target, "Outdated: 1 comment on files no longer in this diff").getAttribute("aria-expanded")).toBe(
    "false",
  );
  expect(quiet.target.textContent).not.toContain("A second retry loop?");
  quiet.done();
});
