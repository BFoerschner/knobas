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
