/**
 * The Files tab read from the checkout first (ADR-0041, the smoothness pass of
 * PR #2479): the checkout's copy of the tree at the mirror's tip draws before
 * the source answers anything, the source's top is read behind it, and a
 * branch that moved since the sync replaces the copy in place.
 *
 * The source is held: each read answers only when the test releases it, so a
 * source that never answers (a slow or hung forge) is the default here.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { LocalTree, SourceView, TreeEntry, ViewBlock } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";

const REPO = "gitea:tidewater/payout-service";
const TIP = "c0ffee0000000000000000000000000000000000";
const MOVED = "d00d000000000000000000000000000000000000";

type Files = Record<string, string>;

const FILES: Files = { "README.md": "# payout-service\n", "src/lib.rs": "pub fn pay() {}\n" };

function localTree(files: Files, commit = TIP): LocalTree {
  const dirs = new Set<string>();
  for (const path of Object.keys(files)) {
    const parts = path.split("/");
    for (let i = 1; i < parts.length; i += 1) dirs.add(parts.slice(0, i).join("/"));
  }
  return {
    commit,
    as_of: "2026-11-09T10:00:00Z",
    truncated: false,
    entries: [
      ...[...dirs].map((path) => ({ path, kind: "dir" as const, size: null })),
      ...Object.entries(files).map(([path, text]) => ({ path, kind: "file" as const, size: text.length })),
    ],
  };
}

const fileBlock = (path: string, text: string, sha: string, commit: string): ViewBlock =>
  ({ type: "file", trail: [], path, text, language: null, size: text.length, truncated: false, binary: false, url: null, sha, commit }) as ViewBlock;

function sourceAnswer(files: Files, section: string, commit: string, sha: string): SourceView {
  const one = (blocks: unknown[]) => ({ sections: [{ id: section, title: "Files", count: null, deferred: false, blocks }], actions: [] }) as unknown as SourceView;
  if (section === "files" || section.startsWith("files:")) {
    const dir = section === "files" ? "" : section.slice("files:".length);
    const prefix = dir === "" ? "" : `${dir}/`;
    const dirs = new Set<string>();
    const entries: TreeEntry[] = [];
    for (const [path, text] of Object.entries(files)) {
      if (!path.startsWith(prefix)) continue;
      const rest = path.slice(prefix.length);
      if (rest.includes("/")) dirs.add(rest.slice(0, rest.indexOf("/")));
      else entries.push({ name: rest, kind: "file", size: text.length, section: `file:${path}` });
    }
    const all = [...[...dirs].map((name): TreeEntry => ({ name, kind: "dir", size: null, section: `files:${prefix}${name}` })), ...entries];
    return one([{ type: "tree", trail: [], entries: all, truncated: false, url: null, commit }]);
  }
  const path = section.slice("file:".length);
  return one([fileBlock(path, files[path] ?? "", sha, commit)]);
}

interface Asked {
  section: string;
  release: (files: Files, commit: string, sha: string) => void;
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let asked: Asked[];
let localReads: string[];

beforeEach(() => {
  asked = [];
  localReads = [];
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function draw(local: LocalTree | null, files: Files = FILES) {
  const ports = {
    entityView: (_at: string, section: string | null) =>
      new Promise<SourceView>((resolve) => {
        asked.push({ section: section ?? "", release: (f, commit, sha) => resolve(sourceAnswer(f, section ?? "", commit, sha)) });
      }),
    localTree: async () => local,
    localFile: async (_at: string, commit: string, path: string) => {
      localReads.push(`${commit.slice(0, 7)}:${path}`);
      const text = files[path];
      return text === undefined ? null : fileBlock(path, text, "b10b-local", commit);
    },
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
  };
  view = mount(TfRepoFiles, {
    target,
    props: { ports, repoId: REPO, at: REPO, branch: "main", path: null, changed: new Set<string>(), treeSections: { dir: "files:", file: "file:" } },
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
const shownFile = () => text(target.querySelector(".tf-fileview"));
const note = () => text(target.querySelector(".tf-gonote"));
/** The source reads Files made; the last commit above the README (`commits`) is the source's own read. */
const sourceReads = () => asked.map((read) => read.section).filter((section) => section !== "commits");
const release = (section: string, files: Files, commit: string, sha = "b10b-local") =>
  asked.find((read) => read.section === section)!.release(files, commit, sha);

test("the checkout's copy draws the tree and the README while the source has not answered at all", async () => {
  draw(localTree(FILES));
  await settle();

  expect(tree()).toEqual(["src", "README.md"]);
  expect(shownFile()).toContain("at c0ffee0 Render as Markdown payout-service");
  expect(localReads).toEqual(["c0ffee0:README.md"]);
  expect(note()).toBe("From your clone at c0ffee0, as of the last sync.");
  // The source's top is asked behind it, and nothing else is: the README came from the checkout.
  expect(sourceReads()).toEqual(["files"]);
});

test("a directory opened from the copy is the checkout's too, with no read of the source", async () => {
  draw(localTree(FILES));
  await settle();
  target.querySelector<HTMLButtonElement>(".tf-ftree button.tf-fdir")!.click();
  await settle();
  expect(tree()).toEqual(["src", "lib.rs", "README.md"]);
  expect(sourceReads()).toEqual(["files"]);
});

test("the source's top at the same commit checks the copy, and nothing is read again", async () => {
  draw(localTree(FILES));
  await settle();
  release("files", FILES, TIP);
  await settle();

  expect(note()).toBe("From your clone at c0ffee0, the same as on the code host.");
  expect(tree()).toEqual(["src", "README.md"]);
  expect(sourceReads()).toEqual(["files"]);
  expect(localReads).toEqual(["c0ffee0:README.md"]);
});

test("a branch that moved since the sync replaces the copy in place, and a file that changed says so", async () => {
  draw(localTree(FILES));
  await settle();
  const newer: Files = { ...FILES, "README.md": "# payout-service, newer\n", "CHANGELOG.md": "1.1\n" };
  release("files", newer, MOVED);
  await settle();

  // The source's tree stands now; the file is read again through it, the copy drawn meanwhile.
  expect(tree()).toEqual(["src", "README.md", "CHANGELOG.md"]);
  expect(note()).toBe("");
  expect(shownFile()).toContain("at c0ffee0 Render as Markdown payout-service");
  release("file:README.md", newer, MOVED, "b10b-newer");
  await settle();
  expect(shownFile()).toContain("newer");
  expect(text(target.querySelector(".tf-fileview .tf-banner.tf-info"))).toBe(
    "main moved on the code host since the last sync, and this file changed with it: the newest is shown.",
  );
});

test("a moved branch whose file did not change says nothing about it", async () => {
  draw(localTree(FILES));
  await settle();
  const newer: Files = { ...FILES, "CHANGELOG.md": "1.1\n" };
  release("files", newer, MOVED);
  await settle();
  release("file:README.md", newer, MOVED, "b10b-local");
  await settle();
  expect(target.querySelector(".tf-fileview .tf-banner.tf-info")).toBeNull();
});

test("a hung source leaves the copy standing, with no error over it", async () => {
  draw(localTree(FILES));
  await settle();
  asked.splice(0);
  await settle();
  expect(target.querySelector(".tf-cfail")).toBeNull();
  expect(tree()).toEqual(["src", "README.md"]);
});

test("with no checkout's copy the tab waits on the source, as before", async () => {
  draw(null);
  await settle();
  expect(tree()).toEqual([]);
  release("files", FILES, TIP);
  await settle();
  expect(tree()).toEqual(["src", "README.md"]);
  expect(note()).toBe("");
  expect(localReads).toEqual([]);
});

/**
 * The source's top answered before the checkout's tree (the inner-pages pass, 2026-11-12): the app keeps the top, so on
 * a later open it beat `git ls-tree`, and every file was then read on the forge. Where the top stands at the checkout's
 * commit, the checkout's tree takes over and the file is read there; at another commit the source's tree stands.
 */
test("a checkout's tree that lands after the source's top at the same commit draws the tree, and the file drawn at that commit stands", async () => {
  let release!: (tree: LocalTree) => void;
  const late = new Promise<LocalTree>((resolve) => (release = resolve));
  const ports = {
    entityView: async (_at: string, section: string | null) => {
      asked.push({ section: section ?? "", release: () => {} });
      return sourceAnswer(FILES, section ?? "", TIP, "b10b");
    },
    localTree: () => late,
    localFile: async (_at: string, commit: string, path: string) => {
      localReads.push(`${commit.slice(0, 7)}:${path}`);
      const text = FILES[path];
      return text === undefined ? null : fileBlock(path, text, "b10b-local", commit);
    },
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
  };
  view = mount(TfRepoFiles, {
    target,
    props: { ports, repoId: REPO, at: REPO, branch: "main", path: null, changed: new Set<string>(), treeSections: { dir: "files:", file: "file:" } },
  });
  flushSync();
  await settle();
  expect(asked.map((a) => a.section)).toContain("files");
  release(localTree(FILES));
  await settle();
  expect(target.querySelector(".tf-local, .tf-fromclone, .tf-heldnote") !== null || text(target).includes("From your clone")).toBe(true);
  // The README the source drew at that commit is the same file: it stays as drawn, not read again (the big-folder pass).
  expect(localReads).toEqual([]);
  expect(target.querySelector(".tf-fileview [aria-busy='true']")).toBeNull();
});

test("a fetch that lands after the source drew at the same commit hands the tree to the clone: a folder then reads nothing from the source", async () => {
  let fetched: ((event: { payload: unknown }) => void) | null = null;
  let held: LocalTree | null = null;
  const ports = {
    entityView: async (_at: string, section: string | null) => {
      asked.push({ section: section ?? "", release: () => {} });
      return sourceAnswer(FILES, section ?? "", TIP, "b10b");
    },
    // Behind at first: the checkout lacks the mirror's commit until the fetch behind the read lands.
    localTree: async () => held,
    localFile: async (_at: string, commit: string, path: string) => {
      localReads.push(`${commit.slice(0, 7)}:${path}`);
      const text = FILES[path];
      return text === undefined ? null : fileBlock(path, text, "b10b-local", commit);
    },
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
    listen: async (event: string, handler: (event: { payload: unknown }) => void) => {
      if (event === "checkout:fetched") fetched = handler;
      return () => {};
    },
  };
  view = mount(TfRepoFiles, {
    target,
    props: { ports, repoId: REPO, at: REPO, branch: "main", path: null, changed: new Set<string>(), treeSections: { dir: "files:", file: "file:" } },
  });
  await settle();
  expect(tree()).toEqual(["src", "README.md"]);
  expect(text(target)).not.toContain("From your clone");
  held = localTree(FILES);
  fetched!({ payload: { entity_id: REPO, commit: TIP } });
  await settle();
  expect(text(target)).toContain("From your clone at c0ffee0, the same as on");
  // The rows stand as drawn, and the README drawn at that commit is not read again.
  expect(tree()).toEqual(["src", "README.md"]);
  expect(localReads).toEqual([]);
  const reads = sourceReads().length;
  (target.querySelector('.tf-ftree [data-path="src"]') as HTMLElement).click();
  await settle();
  expect(tree()).toEqual(["src", "lib.rs", "README.md"]);
  expect(sourceReads().length, "the folder came from the clone").toBe(reads);
});

test("a checkout's tree that lands after the source's top at another commit leaves the source's tree", async () => {
  let release!: (tree: LocalTree) => void;
  const late = new Promise<LocalTree>((resolve) => (release = resolve));
  const ports = {
    entityView: async (_at: string, section: string | null) => sourceAnswer(FILES, section ?? "", MOVED, "b10b"),
    localTree: () => late,
    localFile: async (_at: string, commit: string, path: string) => {
      localReads.push(`${commit.slice(0, 7)}:${path}`);
      return null;
    },
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
  };
  view = mount(TfRepoFiles, {
    target,
    props: { ports, repoId: REPO, at: REPO, branch: "main", path: null, changed: new Set<string>(), treeSections: { dir: "files:", file: "file:" } },
  });
  flushSync();
  await settle();
  release(localTree(FILES));
  await settle();
  expect(text(target)).not.toContain("From your clone");
  expect(localReads).toEqual([]);
});
