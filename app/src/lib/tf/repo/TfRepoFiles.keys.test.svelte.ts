/**
 * Go to file and the tree's keys (#2311): `T` focuses *Go to file*, which
 * searches every path at the commit (the source's whole tree, `paths`), and
 * the arrow keys move, fold and unfold in the tree; no key is taken while a
 * field has focus or while the page is held hidden. Also the file header's
 * meta and *More*. The stand-in is shaped like `knobas-source-gitea`'s tree,
 * whose listings name the whole tree's section.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { SourceView, TreeEntry } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";
import { createRendererDefaults } from "../../render/defaults.svelte";
import { pageContext } from "../kept-pages.svelte";
import { fileHash } from "./repo";

const REPO = "gitea:tidewater/payout-service";
const SHA = "4c1d0aa9e2b7c4d9018e6a2b3c4d5e6f70819200";

type Files = Record<string, string>;

const FILES: Files = {
  "README.md": "# payout\nhello\n",
  "Cargo.toml": "[package]\n",
  "src/main.rs": "fn main() {}\n",
  "src/sepa/retry.rs": "// retry\n",
  "docs/retry-policy.md": "# retry\n",
  "tests/sepa_retry.rs": "#[test]\n",
};

function listing(files: Files, dir: string): TreeEntry[] {
  const prefix = dir === "" ? "" : `${dir}/`;
  const dirs = new Set<string>();
  const entries: TreeEntry[] = [];
  for (const [path, text] of Object.entries(files)) {
    if (!path.startsWith(prefix)) continue;
    const rest = path.slice(prefix.length);
    if (rest.includes("/")) dirs.add(rest.slice(0, rest.indexOf("/")));
    else entries.push({ name: rest, kind: "file", size: text.length, section: `file:${path}` });
  }
  return [...[...dirs].sort().map((name): TreeEntry => ({ name, kind: "dir", size: null, section: `files:${prefix}${name}` })), ...entries];
}

interface Stand {
  /** Every section read, in order. */
  asked: string[];
  /** Whether the listings name a whole tree. */
  whole: boolean;
  /** Whether the whole tree says it was cut. */
  cut: boolean;
}

function source(stand: Stand, files: Files = FILES) {
  return async (_at: string, section: string | null): Promise<SourceView> => {
    const id = section ?? "";
    stand.asked.push(id);
    const one = (blocks: unknown[]) => ({ sections: [{ id, title: "Files", count: null, deferred: false, blocks }], actions: [] }) as unknown as SourceView;
    if (id === "commits") {
      const rows = [
        { entity_id: null, title: "PAY-231: jitter in backoff", meta: "Mara Lindqvist · 3 hours ago", tone: null, chip: "c90d11a2b3", url: null, ...(stand.whole ? { author: "Mara Lindqvist" } : {}) },
      ];
      return { sections: [{ id, title: "Commits", count: stand.whole ? 318 : null, deferred: false, blocks: [{ type: "entities", title: null, rows }] }], actions: [] } as unknown as SourceView;
    }
    if (id === "paths") {
      const entries = Object.keys(files).map((path) => ({ name: path, kind: "file", size: 1, section: `file:${path}` }));
      return one([{ type: "tree", trail: [], entries, truncated: stand.cut, url: null }]);
    }
    if (id === "files" || id.startsWith("files:")) {
      const dir = id === "files" ? "" : id.slice("files:".length);
      return one([{ type: "tree", trail: [], entries: listing(files, dir), truncated: false, url: null, ...(stand.whole ? { paths: "paths" } : {}) }]);
    }
    const path = id.slice("file:".length);
    const text = files[path];
    if (text === undefined) throw { code: "not_found", message: `no file ${path}` };
    return one([{ type: "file", trail: [], path, text, language: null, size: text.length, truncated: false, binary: false, url: "https://gitea.example/x", sha: "b10b", commit: SHA }]);
  };
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let shown = true;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  shown = true;
  location.hash = "";
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function draw(stand: Stand, path: string | null = "README.md", files: Files = FILES) {
  view = mount(TfRepoFiles, {
    target,
    context: pageContext(() => shown),
    props: {
      ports: {
        entityView: source(stand, files),
        submitWrite: async () => {
          throw { code: "invalid", message: "no write here" };
        },
      },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path,
      changed: new Set<string>(),
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
    },
  });
  flushSync();
}

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const stand = (over: Partial<Stand> = {}): Stand => ({ asked: [], whole: true, cut: false, ...over });
const field = () => target.querySelector<HTMLInputElement>("input[aria-label='Go to file']")!;
const press = (key: string, at: EventTarget = document.activeElement ?? window) => {
  at.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  flushSync();
};
const typeIn = (input: HTMLInputElement, text: string) => {
  input.value = text;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
};
const found = () => [...target.querySelectorAll<HTMLAnchorElement>(".tf-ftree a.tf-gotof")].map((a) => a.textContent);
const row = (path: string) => target.querySelector<HTMLElement>(`.tf-ftree [data-path="${path}"]`)!;

test("T focuses Go to file; typed into, it narrows every path at the commit and Enter opens the first", async () => {
  const s = stand();
  draw(s);
  await settle();
  expect(s.asked).not.toContain("paths");

  press("t", document.body);
  expect(document.activeElement).toBe(field());

  typeIn(field(), "retry");
  await settle();
  // The whole tree, read once the field is used: files below directories never opened.
  expect(s.asked.filter((id) => id === "paths")).toHaveLength(1);
  expect(found()).toEqual(["src/sepa/retry.rs", "docs/retry-policy.md", "tests/sepa_retry.rs"]);
  expect(target.querySelector(".tf-ftree [data-path]")).toBeNull();

  typeIn(field(), "sepa retry");
  await settle();
  expect(s.asked.filter((id) => id === "paths")).toHaveLength(1);
  press("Enter", field());
  expect(location.hash).toBe(fileHash(REPO, "src/sepa/retry.rs"));
  expect(field().value).toBe("");
});

test("Esc empties Go to file and the tree is drawn again; nothing typed draws no list", async () => {
  draw(stand());
  await settle();
  typeIn(field(), "zzz");
  await settle();
  expect(found()).toEqual([]);
  expect(target.querySelector(".tf-gonote")?.textContent).toBe("No file matches “zzz”.");
  press("Escape", field());
  expect(field().value).toBe("");
  expect(row("README.md")).not.toBeNull();
});

test("↓ from Go to file walks its answers, ↑ from the first goes back to the field", async () => {
  draw(stand());
  await settle();
  typeIn(field(), "retry");
  await settle();
  field().focus();
  press("ArrowDown", field());
  expect(document.activeElement?.textContent).toBe("src/sepa/retry.rs");
  press("ArrowDown");
  expect(document.activeElement?.textContent).toBe("docs/retry-policy.md");
  press("ArrowUp");
  press("ArrowUp");
  expect(document.activeElement).toBe(field());
});

test("a source with no whole tree is searched among the directories read, and says so", async () => {
  const s = stand({ whole: false });
  draw(s, "src/sepa/retry.rs");
  await settle();
  typeIn(field(), "retry");
  await settle();
  expect(s.asked).not.toContain("paths");
  expect(found()).toEqual(["src/sepa/retry.rs"]);
  expect(target.querySelector(".tf-gonote")?.textContent).toBe("The source lists no whole tree: searched the directories opened so far.");
});

test("a whole tree the source cut is searched as far as it was read, and says so", async () => {
  draw(stand({ cut: true }));
  await settle();
  typeIn(field(), "rs");
  await settle();
  expect(target.querySelector(".tf-gonote")?.textContent).toBe("The tree is larger than knobas reads at once: searched its first 6 files.");
});

test("the arrow keys move between the rows, fold and unfold, and Enter is the row's own", async () => {
  draw(stand());
  await settle();
  row("docs").focus();
  press("ArrowDown");
  expect(document.activeElement).toBe(row("src"));
  press("ArrowRight");
  await settle();
  expect(row("src").getAttribute("aria-expanded")).toBe("true");
  expect(document.activeElement).toBe(row("src"));
  press("ArrowRight");
  expect(document.activeElement).toBe(row("src/sepa"));
  press("ArrowDown");
  expect(document.activeElement).toBe(row("src/main.rs"));
  press("ArrowLeft");
  expect(document.activeElement).toBe(row("src"));
  press("ArrowLeft");
  expect(row("src").getAttribute("aria-expanded")).toBe("false");
  expect(row("src/main.rs")).toBeNull();
  press("ArrowUp");
  expect(document.activeElement).toBe(row("docs"));
});

test("no key is taken while typing in a field: T types, the arrows stay the field's", async () => {
  // A source file draws *Find in file*.
  draw(stand(), "Cargo.toml");
  await settle();
  const find = target.querySelector<HTMLInputElement>("input[aria-label='Find in file']")!;
  find.focus();
  const t = new KeyboardEvent("keydown", { key: "t", bubbles: true, cancelable: true });
  find.dispatchEvent(t);
  expect(t.defaultPrevented).toBe(false);
  expect(document.activeElement).toBe(find);

  field().focus();
  const left = new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true, cancelable: true });
  field().dispatchEvent(left);
  expect(left.defaultPrevented).toBe(false);
  const t2 = new KeyboardEvent("keydown", { key: "T", bubbles: true, cancelable: true });
  field().dispatchEvent(t2);
  expect(t2.defaultPrevented).toBe(false);
});

test("a page held hidden takes no T", async () => {
  draw(stand());
  await settle();
  shown = false;
  press("t", document.body);
  expect(document.activeElement).not.toBe(field());
  shown = true;
  press("t", document.body);
  expect(document.activeElement).toBe(field());
});

test("the tree's footer names its keys", async () => {
  draw(stand());
  await settle();
  expect(target.querySelector(".tf-ftree-keys")?.textContent).toBe("T go to file↑↓ move←→ fold");
});

test("the file header says the size, the lines and the commit, and More holds Edit, History and the source's page", async () => {
  draw(stand());
  await settle();
  expect(target.querySelector(".tf-fileview header .tf-fmeta")?.textContent).toBe("15 B · 2 lines · at 4c1d0aa");
  const more = target.querySelector<HTMLButtonElement>(".tf-fileview header button[aria-label='More']")!;
  expect(target.querySelector(".tf-fileview header a[href$='/edit/README.md']")).toBeNull();
  more.click();
  await settle();
  const items = [...target.querySelectorAll<HTMLElement>(".tf-fmore [role=menuitem]")];
  expect(items.map((i) => i.textContent)).toEqual(["Edit", "History", "Open on the code host"]);
  expect(items[0]!.getAttribute("href")).toBe(`#/repo/${encodeURIComponent(REPO)}/edit/README.md`);
  expect(document.activeElement).toBe(items[0]);
  press("ArrowDown");
  expect(document.activeElement).toBe(items[1]);
  press("Escape");
  expect(target.querySelector(".tf-fmore [role=menu]")).toBeNull();
  expect(document.activeElement).toBe(more);
});

test("the last commit says who made it, with their initials, and how many commits the branch has", async () => {
  draw(stand(), null);
  await settle();
  const strip = target.querySelector(".tf-lastc")!;
  expect(strip.querySelector(".tf-av")?.textContent).toBe("ML");
  expect([...strip.children].map((el) => el.textContent)).toEqual([
    "ML",
    "Mara Lindqvist",
    "PAY-231: jitter in backoff",
    "c90d11a2b3",
    "3 hours ago",
    "318 commits",
  ]);
  expect(strip.querySelector("a.tf-btn")?.getAttribute("href")).toBe(`#/repo/${encodeURIComponent(REPO)}/commits`);
});

test("a source that names no author or count draws the meta whole and History", async () => {
  draw(stand({ whole: false }), null);
  await settle();
  const strip = target.querySelector(".tf-lastc")!;
  expect(strip.querySelector(".tf-av")).toBeNull();
  expect([...strip.children].map((el) => el.textContent)).toEqual(["PAY-231: jitter in backoff", "c90d11a2b3", "Mara Lindqvist · 3 hours ago", "History"]);
});
