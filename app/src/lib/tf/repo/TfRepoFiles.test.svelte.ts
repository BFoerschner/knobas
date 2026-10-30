/**
 * The Files tab mounted on its own over a stand-in source whose answers the
 * test releases by hand, so the order the tab asks in and the order it draws
 * in are both visible (#2244, spec #2234 step 2), and over long files for the
 * windowed viewer (#2251, spec #2234 step 5). The stand-in is shaped like
 * `knobas-source-gitea`'s tree (`files`, `files:<dir>`, `file:<path>`), as the
 * dev fake's is.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { SourceView, TreeEntry } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";

const REPO = "gitea:tidewater/payout-service";

/** A tree of files by path; the stand-in lists its directories from it. */
type Files = Record<string, string>;

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

function answer(files: Files, section: string): SourceView {
  const one = (blocks: unknown[]) => ({ sections: [{ id: section, title: "Files", count: null, deferred: false, blocks }], actions: [] }) as unknown as SourceView;
  if (section === "files" || section.startsWith("files:")) {
    const dir = section === "files" ? "" : section.slice("files:".length);
    return one([{ type: "tree", trail: [], entries: listing(files, dir), truncated: false, url: null }]);
  }
  const path = section.slice("file:".length);
  const text = files[path];
  if (text === undefined) throw { code: "not_found", message: `gitea: no file ${path} at main` };
  return one([{ type: "file", trail: [], path, text, language: null, size: text.length, truncated: false, binary: false, url: null, sha: "b10b" }]);
}

/** A read asked and not yet answered: `release` answers it. */
interface Asked {
  section: string;
  release: () => void;
}

/** A source that answers each read only when the test releases it. */
function heldSource(files: Files) {
  const asked: Asked[] = [];
  const answered: string[] = [];
  const entityView = (_at: string, section: string | null) =>
    new Promise<SourceView>((resolve, reject) => {
      const id = section ?? "";
      asked.push({
        section: id,
        release: () => {
          answered.push(id);
          try {
            resolve(answer(files, id));
          } catch (error) {
            reject(error);
          }
        },
      });
    });
  return { asked, answered, entityView };
}

/** A source that answers every read at once. */
function openSource(files: Files) {
  return { entityView: async (_at: string, section: string | null) => answer(files, section ?? "") };
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function draw(entityView: (at: string, section: string | null) => Promise<SourceView>, path: string | null, line: number | null = null) {
  const ports = {
    entityView,
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
  };
  view = mount(TfRepoFiles, {
    target,
    props: { ports, repoId: REPO, at: REPO, branch: "main", path, line, changed: new Set<string>() },
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

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const tree = () => [...target.querySelectorAll(".tf-ftree a, .tf-ftree button")].map(text);
const drawnLines = () => target.querySelectorAll(".tf-fileview .tf-code .tf-l");
/** Drawn line `i`'s number and text. */
const lineText = (i: number) => [...(drawnLines()[i]?.children ?? [])].map((span) => span.textContent);

const DEEP = "src/psp/gateway/sepa/codes.rs";
const DEEP_FILES: Files = {
  "README.md": "# payout-service",
  [DEEP]: "pub const ACCEPTED: &str = \"ACCP\";\n",
  "src/psp/gateway/sepa/mod.rs": "pub mod codes;",
};

test("a file four directories deep asks for its directories and the file before any of them answers", async () => {
  const source = heldSource(DEEP_FILES);
  draw(source.entityView, DEEP);
  await settle();

  expect(source.answered).toEqual([]);
  expect(source.asked.map((read) => read.section).sort()).toEqual(
    ["files", "files:src", "files:src/psp", "files:src/psp/gateway", "files:src/psp/gateway/sepa", `file:${DEEP}`].sort(),
  );
});

test("the deep file draws as soon as its read answers, and the tree fills in as its directories arrive", async () => {
  const source = heldSource(DEEP_FILES);
  draw(source.entityView, DEEP);
  await settle();
  const release = (section: string) => source.asked.find((read) => read.section === section)!.release();

  release(`file:${DEEP}`);
  await settle();
  expect(text(target.querySelector(".tf-fileview header .tf-p"))).toBe(DEEP);
  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACCP";']);
  expect(tree()).toEqual([]);

  release("files");
  release("files:src");
  await settle();
  expect(tree()).toEqual(["src/", "psp/", "README.md"]);

  for (const section of ["files:src/psp", "files:src/psp/gateway", "files:src/psp/gateway/sepa"]) release(section);
  await settle();
  expect(tree()).toEqual(["src/", "psp/", "gateway/", "sepa/", "codes.rs", "mod.rs", "README.md"]);
  // The listing that names the file takes the read already answered: one read each.
  expect(source.asked).toHaveLength(6);
  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACCP";']);
});

test("a deep file opened again after its early read reads it afresh", async () => {
  const files: Files = { ...DEEP_FILES };
  const reads: string[] = [];
  const entityView = async (_at: string, section: string | null) => {
    reads.push(section ?? "");
    return answer(files, section ?? "");
  };
  const ports = {
    entityView,
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
  };
  const props = $state({ ports, repoId: REPO, at: REPO, branch: "main", path: DEEP as string | null, line: null, changed: new Set<string>() });
  view = mount(TfRepoFiles, { target, props });
  flushSync();
  await settle();
  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACCP";']);

  files[DEEP] = "pub const ACCEPTED: &str = \"ACSC\";\n";
  props.path = "src/psp/gateway/sepa/mod.rs";
  await settle();
  props.path = DEEP;
  await settle();

  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACSC";']);
  expect(reads.filter((read) => read === `file:${DEEP}`)).toHaveLength(2);
});

test("stepping back to the file drawn before the next one answers shows it undimmed", async () => {
  const source = heldSource(DEEP_FILES);
  const ports = {
    entityView: source.entityView,
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
  };
  const props = $state({ ports, repoId: REPO, at: REPO, branch: "main", path: DEEP as string | null, line: null, changed: new Set<string>() });
  view = mount(TfRepoFiles, { target, props });
  flushSync();
  const releaseAll = async () => {
    for (let round = 0; round < 6; round += 1) {
      for (const read of [...source.asked]) if (!source.answered.includes(read.section)) read.release();
      await settle();
    }
  };
  await releaseAll();
  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACCP";']);

  // mod.rs's read is held; the reader steps back to codes.rs before it answers.
  props.path = "src/psp/gateway/sepa/mod.rs";
  await settle();
  expect(target.querySelector(".tf-fileview .tf-wait-dim")).not.toBeNull();
  props.path = DEEP;
  await settle();

  expect(target.querySelector(".tf-fileview .tf-wait-dim")).toBeNull();
  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACCP";']);
});

test("a link to a file that does not exist says so once its directory is read", async () => {
  const missing = "src/psp/gateway/sepa/gone.rs";
  const source = heldSource(DEEP_FILES);
  draw(source.entityView, missing);
  await settle();

  source.asked.find((read) => read.section === `file:${missing}`)!.release();
  await settle();
  expect(target.querySelector(".tf-cfail")).toBeNull();

  for (const read of [...source.asked]) if (!source.answered.includes(read.section)) read.release();
  await settle();
  expect(text(target.querySelector(".tf-fileview .tf-cfail"))).toBe(`There is no file ${missing} on main.`);
});

test("a source that names its sections otherwise still opens the deep file, through the sections its listings name", async () => {
  // `dir:`/`blob:` in place of Gitea's `files:`/`file:`: every early guess is refused.
  const theirs = (id: string) => id.replace(/^files:/, "dir:").replace(/^file:/, "blob:");
  const ours = (id: string) => id.replace(/^dir:/, "files:").replace(/^blob:/, "file:");
  const reads: string[] = [];
  const entityView = async (_at: string, section: string | null) => {
    const id = section ?? "";
    reads.push(id);
    if (id !== "files" && ours(id) === id) throw { code: "invalid", message: `no section ${id}` };
    const view = answer(DEEP_FILES, ours(id));
    const block = view.sections[0]!.blocks[0]!;
    if (block.type === "tree") for (const entry of block.entries) entry.section = entry.section && theirs(entry.section);
    return view;
  };
  draw(entityView, DEEP);
  await settle();

  expect(lineText(0)).toEqual(["1", 'pub const ACCEPTED: &str = "ACCP";']);
  expect(target.querySelector(".tf-cfail")).toBeNull();
  expect(tree()).toEqual(["src/", "psp/", "gateway/", "sepa/", "codes.rs", "mod.rs", "README.md"]);
  expect(reads).toContain(`blob:${DEEP}`);
});

/** A file of `n` numbered lines. */
const long = (n: number) => Array.from({ length: n }, (_, i) => `let value_${i + 1} = ${i + 1};`).join("\n");

test("a 50,000-line file draws fewer than 500 of its lines", async () => {
  draw(openSource({ "BULK.rs": long(50_000) }).entityView, "BULK.rs");
  await settle();

  expect(text(target.querySelector(".tf-fileview header .tf-dim"))).toBe("50000 lines, on main");
  expect(drawnLines().length).toBeGreaterThan(0);
  expect(drawnLines().length).toBeLessThan(500);
  expect(lineText(0)).toEqual(["1", "let value_1 = 1;"]);
});

test("a 1,999-line file draws whole", async () => {
  draw(openSource({ "BULK.rs": long(1_999) }).entityView, "BULK.rs");
  await settle();

  expect(drawnLines()).toHaveLength(1_999);
  expect(lineText(1_998)).toEqual(["1999", "let value_1999 = 1999;"]);
});

test("a link to line 40,000 of a long file draws and marks that line", async () => {
  draw(openSource({ "BULK.rs": long(50_000) }).entityView, "BULK.rs", 40_000);
  await settle();

  const marked = target.querySelectorAll(".tf-fileview .tf-code .tf-l.tf-at");
  expect(marked).toHaveLength(1);
  expect([...marked[0]!.children].map((span) => span.textContent)).toEqual(["40000", "let value_40000 = 40000;"]);
  expect(drawnLines().length).toBeLessThan(500);
});

test("copying a selection across lines gives the selected text, without the line numbers", async () => {
  draw(openSource({ "src/main.rs": "fn main() {\n    run();\n}\n" }).entityView, "src/main.rs");
  await settle();

  // From after `fn ` on line 1 to after `}` on line 3, as a drag leaves it.
  const body = (i: number) => drawnLines()[i]!.lastElementChild!.firstChild!;
  const range = document.createRange();
  range.setStart(body(0), 3);
  range.setEnd(body(2), 1);
  const selection = document.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
  const copied: Record<string, string> = {};
  const event = Object.assign(new Event("copy", { bubbles: true, cancelable: true }), {
    clipboardData: { setData: (type: string, value: string) => (copied[type] = value) },
  });
  body(1).parentElement!.dispatchEvent(event);

  expect(copied["text/plain"]).toBe("main() {\n    run();\n}");
  expect(event.defaultPrevented).toBe(true);
});

// ---- the tree at a commit, moved behind (#2241, #2242) ----

/**
 * A source whose listings name the commit they were read at, as Gitea's do
 * (`file@<n>:<path>`), and whose branch `push()` moves to a new one.
 */
function commitSource() {
  let tip = 1;
  const texts: Record<number, string> = { 1: "# one", 2: "# two" };
  const reads: string[] = [];
  const entityView = async (_at: string, section: string | null): Promise<SourceView> => {
    const id = section ?? "";
    reads.push(id);
    const one = (blocks: unknown[]) => ({ sections: [{ id, title: "Files", count: null, deferred: false, blocks }], actions: [] }) as unknown as SourceView;
    if (id === "files")
      return one([{ type: "tree", trail: [], entries: [{ name: "README.md", kind: "file", size: 5, section: `file@${tip}:README.md` }], truncated: false, url: null }]);
    const at = Number(/^file@(\d+):/.exec(id)?.[1] ?? tip);
    return one([{ type: "file", trail: [], path: "README.md", text: texts[at], language: null, size: 5, truncated: false, binary: false, url: null, sha: `b${at}` }]);
  };
  return { reads, entityView, push: () => (tip += 1) };
}

test("the app's view:fresh for the top directory reads the tree at the new commit and the file there; one that did not move reads nothing more", async () => {
  const source = commitSource();
  const heard: ((event: { payload: unknown }) => void)[] = [];
  const ports = {
    entityView: source.entityView,
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
    listen: async (event: string, handler: (event: { payload: unknown }) => void) => {
      if (event === "view:fresh") heard.push(handler);
      return () => {};
    },
  };
  view = mount(TfRepoFiles, { target, props: { ports, repoId: REPO, at: REPO, branch: "main", path: null, changed: new Set<string>() } });
  await settle();
  expect(lineText(0)).toEqual(["1", "# one"]);

  const tell = (entity_id: string, section: string) => {
    for (const hear of heard) hear({ payload: { entity_id, section } });
  };
  const before = source.reads.length;
  tell(REPO, "files");
  await settle();
  expect(source.reads.slice(before)).toEqual(["files"]);
  expect(lineText(0)).toEqual(["1", "# one"]);

  source.push();
  tell(`${REPO}@refs/heads/other`, "files");
  await settle();
  expect(lineText(0)).toEqual(["1", "# one"]);

  tell(REPO, "files");
  await settle();
  expect(lineText(0)).toEqual(["1", "# two"]);
  expect(source.reads).toContain("file@2:README.md");
});

test("a deep link's guess that drew the file stands for the section its listing names, so the file is not read twice", async () => {
  const source = commitSource();
  draw(source.entityView, "README.md");
  await settle();
  expect(lineText(0)).toEqual(["1", "# one"]);
  expect(source.reads.filter((id) => id.endsWith("README.md"))).toEqual(["file:README.md"]);
});
