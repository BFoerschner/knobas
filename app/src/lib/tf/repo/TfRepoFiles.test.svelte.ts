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
