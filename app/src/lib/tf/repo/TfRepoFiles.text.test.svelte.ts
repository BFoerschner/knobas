/**
 * *Text search* in the Files tab (ADR-0062): the tree's search box searches the files' names (`T`) or their text
 * (`Mod+Shift+F`); typed text is searched at the tree the tab shows with the *Aa* and *.\** switches, and its answer
 * stands in the tree's place, each file with its lines, a line opening the file there. *Search in repository* hands its
 * words over and asks for file names too. The backend is a stand-in port: the scans are the Rust tests'.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { SourceView, TextFound, TextQuery } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";
import { createRendererDefaults } from "../../render/defaults.svelte";
import { pageContext } from "../kept-pages.svelte";
import { layers } from "../layers.svelte";
import { fileHash, lineHash } from "./repo";
import { askText } from "./text-search.svelte";

const REPO = "gitea:tidewater/payout-service";
const SHA = "4c1d0aa9e2b7c4d9018e6a2b3c4d5e6f70819200";

async function entityView(_at: string, section: string | null): Promise<SourceView> {
  const id = section ?? "";
  const one = (blocks: unknown[]) => ({ sections: [{ id, title: "Files", count: null, deferred: false, blocks }], actions: [] }) as unknown as SourceView;
  if (id === "files" || id === "") {
    return one([{ type: "tree", trail: [], entries: [{ name: "README.md", kind: "file", size: 9, section: "file:README.md" }], truncated: false, url: null }]);
  }
  if (id === "commits") return { sections: [{ id, title: "Commits", count: null, deferred: false, blocks: [] }], actions: [] } as unknown as SourceView;
  return one([{ type: "file", trail: [], path: "README.md", text: "# payout\n", language: null, size: 9, truncated: false, binary: false, url: null, sha: "b10b", commit: SHA }]);
}

interface Asked {
  at: string;
  commit: string | null;
  working: boolean;
  branch: string | null;
  query: TextQuery;
}

const ANSWER: TextFound = {
  from: "clone",
  commit: SHA,
  branch: null,
  files: [
    {
      path: "src/sepa/retry.rs",
      hits: [
        { line: 12, parts: [{ text: "fn ", hit: false }, { text: "retry", hit: true }, { text: "_on() {", hit: false }] },
        { line: 40, parts: [{ text: "    ", hit: false }, { text: "retry", hit: true }, { text: "();", hit: false }] },
      ],
      more: 3,
    },
  ],
  names: [],
  searched: 6,
  skipped: 1,
  cut: false,
  invalid: null,
  why: null,
  stale: false,
};

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let asked: Asked[];
let answer: TextFound;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  location.hash = "";
  asked = [];
  answer = ANSWER;
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function draw(props: Record<string, unknown> = {}) {
  view = mount(TfRepoFiles, {
    target,
    context: pageContext(() => true),
    props: {
      ports: {
        entityView,
        submitWrite: async () => {
          throw { code: "invalid", message: "no write here" };
        },
      },
      repoId: REPO,
      at: `${REPO}:feature`,
      branch: "feature",
      path: "README.md",
      changed: new Set<string>(),
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
      textPort: (at: string, commit: string | null, working: boolean, branch: string | null, query: TextQuery) => {
        asked.push({ at, commit, working, branch, query });
        return Promise.resolve(answer);
      },
      ...props,
    },
  });
  flushSync();
}

async function settle(ms = 0) {
  await new Promise((resolve) => setTimeout(resolve, ms));
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const textField = () => target.querySelector<HTMLInputElement>("input[aria-label='Search text']");
const goField = () => target.querySelector<HTMLInputElement>("input[aria-label='Go to file']");
const segment = (name: string) => [...target.querySelectorAll<HTMLButtonElement>(".tf-seg button")].find((b) => b.textContent === name)!;
const chord = (key: string, over: KeyboardEventInit = {}) => {
  (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...over }));
  flushSync();
};
const typeIn = (input: HTMLInputElement, text: string) => {
  input.value = text;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
};

test("the box offers Files | Text: Mod+Shift+F focuses Text, from anywhere, and T goes back to Go to file", async () => {
  draw();
  await settle();
  expect(segment("Files").getAttribute("aria-pressed")).toBe("true");
  expect(textField()).toBeNull();

  chord("F", { metaKey: true, shiftKey: true });
  expect(segment("Text").getAttribute("aria-pressed")).toBe("true");
  expect(document.activeElement).toBe(textField());
  // Ctrl stands for ⌘ off the Mac; a chord is taken from a field too.
  chord("t");
  expect(document.activeElement).toBe(textField());

  (document.activeElement as HTMLElement).blur();
  chord("t");
  expect(segment("Files").getAttribute("aria-pressed")).toBe("true");
  expect(document.activeElement).toBe(goField());
  chord("f", { ctrlKey: true, shiftKey: true });
  expect(document.activeElement).toBe(textField());
});

test("a dialog over the page keeps Mod+Shift+F", async () => {
  draw();
  await settle();
  const off = layers.add({ kind: "dialog", close: () => {} });
  try {
    chord("F", { metaKey: true, shiftKey: true });
    expect(textField()).toBeNull();
  } finally {
    off();
  }
});

test("typed text is searched at the tree shown, smart case and literal by default, its lines opening the file there", async () => {
  draw();
  await settle();
  segment("Text").click();
  flushSync();
  typeIn(textField()!, "retry");
  await settle(200);
  expect(asked).toEqual([{ at: `${REPO}:feature`, commit: null, working: false, branch: "feature", query: { text: "retry", case_sensitive: false, regex: false, names: false } }]);
  // The answer stands in the tree's place: the file, its count, its lines with the hits marked.
  expect(target.querySelector(".tf-ftree [data-path]")).toBeNull();
  const file = target.querySelector<HTMLAnchorElement>("a.tf-txfile")!;
  expect(file.getAttribute("href")).toBe(fileHash(REPO, "src/sepa/retry.rs"));
  expect(file.querySelector(".tf-txcount")!.textContent).toBe("5");
  const hits = [...target.querySelectorAll<HTMLAnchorElement>("a.tf-txhit")];
  expect(hits.map((a) => a.getAttribute("href"))).toEqual([lineHash(REPO, "src/sepa/retry.rs", 12), lineHash(REPO, "src/sepa/retry.rs", 40)]);
  expect(hits[0]!.querySelector(".tf-txn")!.textContent).toBe("12");
  expect(hits[0]!.querySelector("mark")!.textContent).toBe("retry");
  expect(target.querySelector(".tf-txmore")!.textContent).toContain("3 more lines");
  expect(target.querySelector(".tf-ftree [role='status']")!.textContent).toBe("5 lines in 1 file in your clone at 4c1d0aa.");
  expect(target.textContent).toContain("1 binary or large file was not searched.");

  // Aa and .* ask again with their switch on.
  const chip = (label: string) => target.querySelector<HTMLButtonElement>(`.tf-fchips button[aria-label='${label}']`)!;
  chip("Match case").click();
  await settle();
  expect(chip("Match case").getAttribute("aria-pressed")).toBe("true");
  expect(asked.at(-1)!.query).toEqual({ text: "retry", case_sensitive: true, regex: false, names: false });
  chip("Regular expression").click();
  await settle();
  expect(asked.at(-1)!.query).toEqual({ text: "retry", case_sensitive: true, regex: true, names: false });
});

test("words typed apart go as typed, the box says how words and quotes match, and every word is marked", async () => {
  draw();
  await settle();
  segment("Text").click();
  flushSync();
  // Before anything is typed, the note says what words and quotes do (#2561).
  const note = target.querySelector(".tf-ftree .tf-gonote")!.textContent!;
  expect(note).toContain("Words match in the order typed on one line, anything between them");
  expect(note).toContain("\"in quotes\"");
  expect(textField()!.title).toContain("xy room");
  answer = {
    ...ANSWER,
    files: [{ path: "src/room.rs", hits: [{ line: 3, parts: [{ text: "let ", hit: false }, { text: "xy", hit: true }, { text: "_", hit: false }, { text: "room", hit: true }] }], more: 0 }],
  };
  typeIn(textField()!, "xy room");
  await settle(200);
  // The words are the backend's to read: sent as typed, spaces and all.
  expect(asked.at(-1)!.query).toEqual({ text: "xy room", case_sensitive: false, regex: false, names: false });
  expect([...target.querySelectorAll("a.tf-txhit mark")].map((mark) => mark.textContent)).toEqual(["xy", "room"]);
});

test("the working tree is searched on disk, and where the text came from is said", async () => {
  draw({ disk: { head: SHA, marks: new Map(), stamp: "s1", branch: "main", standing: null } });
  await settle();
  answer = { ...ANSWER, from: "index", branch: "main" };
  chord("F", { metaKey: true, shiftKey: true });
  typeIn(textField()!, "retry");
  await settle(200);
  expect(asked[0]).toMatchObject({ working: true, commit: null });
  expect(target.querySelector(".tf-ftree [role='status']")!.textContent).toContain("in knobas's copy of main, as of the last sync");
  expect(target.textContent).toContain("A clone searches any branch.");
});

test("a pattern that does not parse, and a branch nothing can search, say why", async () => {
  draw();
  await settle();
  answer = { ...ANSWER, files: [], from: "none", invalid: "The pattern does not parse: unclosed group." };
  chord("F", { metaKey: true, shiftKey: true });
  typeIn(textField()!, "(open");
  await settle(200);
  expect(target.querySelector(".tf-txbad")!.textContent).toBe("The pattern does not parse: unclosed group.");
  answer = { ...ANSWER, files: [], from: "none", why: "Clone this repository to search this branch: knobas keeps the text of the default branch only." };
  typeIn(textField()!, "retry");
  await settle(200);
  expect(target.querySelector(".tf-ftree [role='status']")!.textContent).toContain("Clone this repository to search this branch");
});

test("Search in repository opens Text with the selection's words and asks for file names too", async () => {
  answer = { ...ANSWER, names: ["src/sepa/retry.rs"] };
  askText(REPO, "retry");
  draw();
  await settle();
  expect(segment("Text").getAttribute("aria-pressed")).toBe("true");
  expect(textField()!.value).toBe("retry");
  expect(asked).toHaveLength(1);
  expect(asked[0]!.query).toEqual({ text: "retry", case_sensitive: false, regex: false, names: true });
  expect([...target.querySelectorAll(".tf-txhead")].map((p) => p.textContent)).toEqual(["Files named “retry”", "Lines"]);
  // Another repository's words wait for its own tab.
  askText("gitea:tidewater/other", "x");
  await settle();
  expect(asked).toHaveLength(1);
});
