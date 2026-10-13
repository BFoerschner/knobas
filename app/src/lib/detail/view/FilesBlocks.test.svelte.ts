/**
 * Reading code (#711): a directory's listing (`TreeBlock`), a file
 * (`FileBlock`) and the trail above both (`Trail`). Every entry and crumb
 * opens a section the source named, in the same tab; the shell never reads
 * the id.
 */
import { flushSync, mount, unmount, type Component } from "svelte";
import { afterEach, expect, test } from "vitest";

import type { Crumb, TreeEntry } from "../../ipc/entity";
import FileBlock from "./FileBlock.svelte";
import Trail from "./Trail.svelte";
import TreeBlock from "./TreeBlock.svelte";

const mounted: (() => void)[] = [];
afterEach(() => {
  for (const done of mounted.splice(0)) done();
});

function render<P extends Record<string, unknown>>(component: Component<P>, props: P) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(component, { target, props });
  flushSync();
  mounted.push(() => {
    unmount(app);
    target.remove();
  });
  return target;
}

const TRAIL: Crumb[] = [
  { name: "main", section: "files" },
  { name: "src", section: "files:src" },
  { name: "lib", section: "files:src/lib" },
];

/** A listing's props, less its entries. */
const TREE = {
  trail: TRAIL,
  entries: [] as TreeEntry[],
  truncated: false,
  url: "https://gitea.example/o/r/src/branch/main/src/lib",
  sourceName: "Tidewater Git",
  onopen: (_: string) => {},
  onopenurl: (_: string) => {},
};

const ENTRIES: TreeEntry[] = [
  { name: "diff", kind: "dir", size: null, section: "files:src/lib/diff" },
  { name: "view.rs", kind: "file", size: 12_800, section: "file:src/lib/view.rs" },
  { name: "current", kind: "symlink", size: 7, section: "file:src/lib/current" },
  { name: "vendor", kind: "submodule", size: null, section: null },
];

test("the trail leads back up: every step but the last opens its section", () => {
  const opened: string[] = [];
  const target = render(Trail, { trail: TRAIL, onopen: (s: string) => opened.push(s) });
  const nav = target.querySelector("nav")!;
  expect(nav.getAttribute("aria-label")).toBe("Path");
  const steps = [...nav.querySelectorAll("button")];
  expect(steps.map((b) => b.textContent?.trim())).toEqual(["main", "src"]);
  const here = nav.querySelector("[aria-current=location]")!;
  expect(here.textContent).toBe("lib");
  expect(here.tagName).not.toBe("BUTTON");
  steps[0]!.click();
  steps[1]!.click();
  expect(opened).toEqual(["files", "files:src"]);
});

test("a trail offers Back only when the tab has somewhere to go back to", () => {
  const plain = render(Trail, { trail: TRAIL, onopen: () => {} });
  expect(plain.querySelector(".tr-back")).toBeNull();
  let backs = 0;
  const target = render(Trail, { trail: TRAIL, onopen: () => {}, onback: () => backs++ });
  const back = target.querySelector<HTMLButtonElement>(".tr-back")!;
  expect(back.getAttribute("aria-label")).toBe("Back");
  back.click();
  expect(backs).toBe(1);
});

test("a listing draws each entry with its size, and opens what can be opened", () => {
  const opened: string[] = [];
  const target = render(TreeBlock, {
    ...TREE,
    entries: ENTRIES,
    onopen: (s: string) => opened.push(s),
  });
  const rows = [...target.querySelectorAll<HTMLElement>(".tb-row")];
  expect(rows.map((r) => r.dataset.kind)).toEqual(["dir", "file", "symlink", "submodule"]);
  expect(rows[1]!.textContent).toContain("12.5 KB");
  expect(rows[0]!.querySelector(".tb-size")?.textContent?.trim() ?? "").toBe("");
  // A submodule is another repository: named, not opened.
  expect(rows[3]!.querySelector("button")).toBeNull();
  expect(rows[3]!.textContent).toContain("submodule");
  for (const row of rows.slice(0, 3)) row.querySelector("button")!.click();
  expect(opened).toEqual(["files:src/lib/diff", "file:src/lib/view.rs", "file:src/lib/current"]);
  // The trail is drawn above the listing.
  expect(target.querySelector("nav[aria-label=Path]")).not.toBeNull();
});

test("an empty directory says so", () => {
  const target = render(TreeBlock, { ...TREE, entries: [] });
  expect(target.textContent).toContain("This directory is empty.");
  expect(target.querySelector(".tb-cut")).toBeNull();
});

test("a cut listing says so and opens the directory at its source", () => {
  const urls: string[] = [];
  const target = render(TreeBlock, {
    ...TREE,
    entries: ENTRIES,
    truncated: true,
    onopenurl: (u: string) => urls.push(u),
  });
  const note = target.querySelector(".tb-cut")!;
  expect(note.textContent).toContain("knobas shows the first 4 entries of this directory.");
  note.querySelector("button")!.click();
  expect(urls).toEqual(["https://gitea.example/o/r/src/branch/main/src/lib"]);
});

function file(over: Partial<Record<string, unknown>> = {}) {
  const opened: string[] = [];
  const urls: string[] = [];
  const target = render(FileBlock, {
    trail: [...TRAIL.slice(0, 2), { name: "main.rs", section: "file:src/main.rs" }],
    path: "src/main.rs",
    text: "fn main() {\n    <b>hi</b>\n}\n",
    language: "rust",
    size: 32,
    truncated: false,
    binary: false,
    url: "https://gitea.example/o/r/src/branch/main/src/main.rs",
    sourceName: "Tidewater Git",
    onopen: (s: string) => opened.push(s),
    onopenurl: (u: string) => urls.push(u),
    ...over,
  });
  return { target, opened, urls };
}

test("a file is drawn monospace with a number for each line, its text as text", () => {
  const { target } = file();
  const numbers = target.querySelector(".fb-nums")!.textContent!.split("\n");
  expect(numbers).toEqual(["1", "2", "3"]);
  const code = target.querySelector(".fb-code")!;
  expect(code.textContent).toBe("fn main() {\n    <b>hi</b>\n}");
  expect(code.querySelector("b")).toBeNull();
  expect(target.querySelector(".fb-meta")!.textContent).toContain("3 lines");
  expect(target.querySelector("[aria-current=location]")!.textContent).toBe("main.rs");
});

test("a cut file says where the rest is, and opens it", () => {
  const { target, urls } = file({ truncated: true, size: 3_200_000 });
  const note = target.querySelector(".fb-cut")!;
  expect(note.textContent!.replace(/\s+/g, " ")).toContain("knobas shows the start of this file: 3 lines of 3.1 MB.");
  note.querySelector("button")!.click();
  expect(urls).toEqual(["https://gitea.example/o/r/src/branch/main/src/main.rs"]);
});

test("a binary file says so with its size, and draws no lines", () => {
  const { target, urls } = file({ binary: true, text: "", size: 2_048, path: "logo.png", language: null });
  expect(target.querySelector(".fb-code")).toBeNull();
  const note = target.querySelector(".fb-note")!;
  expect(note.textContent).toContain("Binary file, 2.0 KB");
  note.querySelector("button")!.click();
  expect(urls).toHaveLength(1);
});

test("a Markdown file opens rendered and toggles to its raw text", () => {
  const { target } = file({ text: "# Payouts\n\nPays out.\n", language: "markdown", path: "README.md" });
  expect(target.querySelector(".md h1")?.textContent).toBe("Payouts");
  expect(target.querySelector(".fb-code")).toBeNull();
  const toggle = target.querySelector<HTMLButtonElement>(".fb-raw")!;
  expect(toggle.getAttribute("aria-pressed")).toBe("false");
  toggle.click();
  flushSync();
  expect(toggle.getAttribute("aria-pressed")).toBe("true");
  expect(target.querySelector(".md")).toBeNull();
  expect(target.querySelector(".fb-code")!.textContent).toBe("# Payouts\n\nPays out.");
});

test("a file not in Markdown has no raw toggle", () => {
  const { target } = file();
  expect(target.querySelector(".fb-raw")).toBeNull();
});
