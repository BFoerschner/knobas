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
import { afterEach, beforeEach, describe, expect, test } from "vitest";

import type { LocalTree, SourceView, TreeEntry, ViewBlock } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";
import TfRepoFilesRefreshed from "./TfRepoFiles.refresh.fixture.svelte";
import { checkoutPorts } from "./checkout-ports.test-support";

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
    ...checkoutPorts(() => local),
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

  // The source's tree stands now; the file is read again through it, the copy drawn meanwhile, undimmed, its header
  // saying from when it is (the forge-waits pass, 2026-11-13).
  expect(tree()).toEqual(["src", "README.md", "CHANGELOG.md"]);
  expect(note()).toBe("");
  expect(text(target.querySelector(".tf-fileview .tf-fmeta"))).toBe("As knobas read it a moment ago, at c0ffee0. Checking the code host…");
  expect(shownFile()).toContain("Render as Markdown payout-service");
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
    ...checkoutPorts(() => late),
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
    ...checkoutPorts(() => held),
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
    ...checkoutPorts(() => late),
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

for (const order of ["the source's re-read, then the fetch", "the fetch, then the source's re-read"] as const)
  test(`after a push, the kept top at the commit before, then ${order}: the clone takes over at the new commit`, async () => {
    // Every listener by event: the last commit above the README hears `view:fresh` too.
    const handlers = new Map<string, ((event: { payload: unknown }) => void)[]>();
    const tell = (event: string, payload: unknown) => {
      for (const handler of handlers.get(event) ?? []) handler({ payload });
    };
    let top = MOVED;
    let held: LocalTree | null = null;
    const ports = {
      entityView: async (_at: string, section: string | null) => {
        asked.push({ section: section ?? "", release: () => {} });
        return sourceAnswer(FILES, section ?? "", top, "b10b");
      },
      ...checkoutPorts(() => held),
      localFile: async (_at: string, commit: string, path: string) => {
        localReads.push(`${commit.slice(0, 7)}:${path}`);
        const text = FILES[path];
        return text === undefined ? null : fileBlock(path, text, "b10b-local", commit);
      },
      submitWrite: async () => {
        throw { code: "invalid", message: "no write in this suite" };
      },
      listen: async (event: string, handler: (event: { payload: unknown }) => void) => {
        handlers.set(event, [...(handlers.get(event) ?? []), handler]);
        return () => {};
      },
    };
    view = mount(TfRepoFiles, {
      target,
      props: { ports, repoId: REPO, at: REPO, branch: "main", path: null, changed: new Set<string>(), treeSections: { dir: "files:", file: "file:" } },
    });
    await settle();
    expect(text(target)).not.toContain("From your clone");
    // The app's read behind its kept answer finds the branch at the mirror's new tip; the clone fetched it.
    top = TIP;
    const fresh = () => tell("view:fresh", { entity_id: REPO, section: "files", failed: false });
    const fetched = () => {
      held = localTree(FILES);
      tell("checkout:fetched", { entity_id: REPO, commit: TIP });
    };
    if (order === "the source's re-read, then the fetch") {
      fresh();
      await settle();
      fetched();
    } else {
      fetched();
      await settle();
      expect(text(target)).not.toContain("From your clone");
      fresh();
    }
    await settle();
    expect(text(target)).toContain("From your clone at c0ffee0, the same as on");
    const reads = sourceReads().length;
    (target.querySelector('.tf-ftree [data-path="src"]') as HTMLElement).click();
    await settle();
    expect(tree()).toEqual(["src", "lib.rs", "README.md"]);
    expect(sourceReads().length, "the folder came from the clone").toBe(reads);
  });

/**
 * The big-folder pass (2026-11-13): the tab reads the checkout's tree a folder at a time, so a folder past the twenty
 * thousandth entry is drawn with every entry (the whole tree's read stopped at twenty thousand, and the tab drew what
 * it held as the whole), and the page holds what it draws: the top at the tab's open, each folder when it opens.
 */
test("a folder past twenty thousand entries of a clone is drawn whole, read when it opens", async () => {
  const files: Files = { "README.md": "# big\n" };
  for (let n = 0; n < 20_500; n += 1) files[`a/f${String(n).padStart(5, "0")}.txt`] = "x\n";
  for (let n = 0; n < 300; n += 1) files[`z/g${String(n).padStart(3, "0")}.txt`] = "x\n";
  const ports = checkoutPorts(() => localTree(files));
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports: {
        entityView: () => new Promise<SourceView>(() => {}),
        ...ports,
        localFile: async (_at: string, commit: string, path: string) => fileBlock(path, files[path] ?? "", "b10b", commit),
        submitWrite: async () => {
          throw { code: "invalid", message: "no write in this suite" };
        },
      },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path: null,
      changed: new Set<string>(),
      treeSections: { dir: "files:", file: "file:" },
    },
  });
  await settle();
  expect(tree()).toEqual(["a", "z", "README.md"]);
  expect(ports.dirReads, "the tab's open reads the top alone").toEqual(["tip:"]);
  (target.querySelector('.tf-ftree [data-path="z"]') as HTMLElement).click();
  await settle();
  expect(ports.dirReads).toEqual(["tip:", `${TIP}:z`]);
  // Every entry of `z`, all 300 past the twenty thousandth of the tree, in the window's rows and room.
  const rows = [...target.querySelectorAll<HTMLElement>(".tf-ftree [data-path^='z/']")].map((el) => el.dataset["path"]);
  expect(rows[0]).toBe("z/g000.txt");
  expect(rows.length).toBeGreaterThan(20);
  const room = target.querySelector<HTMLElement>(".tf-ftrows")!;
  const drawn = target.querySelectorAll(".tf-ftree [data-path]").length;
  expect(parseFloat(room.style.paddingBottom || "0") / 24 + drawn, "every row drawn or room for it").toBe(303);
});

/** The clone taking over from the source's tree reads every folder it drew first, so an open folder's rows stay. */
test("a clone that takes over with a folder open keeps its rows where they are", async () => {
  const many: Files = { "README.md": "# r\n" };
  for (let n = 0; n < 5; n += 1) many[`src/m${n}.rs`] = "x\n";
  let fetched: ((event: { payload: unknown }) => void) | null = null;
  let held: LocalTree | null = null;
  const ports = checkoutPorts(() => held);
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports: {
        entityView: async (_at: string, section: string | null) => {
          asked.push({ section: section ?? "", release: () => {} });
          return sourceAnswer(many, section ?? "", TIP, "b10b");
        },
        ...ports,
        localFile: async (_at: string, commit: string, path: string) => fileBlock(path, many[path] ?? "", "b10b-local", commit),
        submitWrite: async () => {
          throw { code: "invalid", message: "no write in this suite" };
        },
        listen: async (event: string, handler: (event: { payload: unknown }) => void) => {
          if (event === "checkout:fetched") fetched = handler;
          return () => {};
        },
      },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path: null,
      changed: new Set<string>(),
      treeSections: { dir: "files:", file: "file:" },
    },
  });
  await settle();
  (target.querySelector('.tf-ftree [data-path="src"]') as HTMLElement).click();
  await settle();
  const before = tree();
  expect(before).toEqual(["src", "m0.rs", "m1.rs", "m2.rs", "m3.rs", "m4.rs", "README.md"]);
  held = localTree(many);
  fetched!({ payload: { entity_id: REPO, commit: TIP } });
  // Mid-read, the rows stand as drawn.
  expect(tree()).toEqual(before);
  await settle();
  expect(text(target)).toContain("From your clone at c0ffee0");
  expect(tree()).toEqual(before);
  expect(ports.dirReads).toContain(`${TIP}:src`);
});

/** A folder the source listed past its whole tree, or cut at its cap, says so under the tree, never among its rows. */
test("a folder the source listed on its own, or cut, says so under the tree", async () => {
  const ports = {
    entityView: async (_at: string, section: string | null) => {
      const id = section ?? "";
      const one = (block: unknown) => ({ sections: [{ id, title: "Files", count: null, deferred: false, blocks: [block] }], actions: [] }) as unknown as SourceView;
      if (id === "files")
        return one({ type: "tree", trail: [], truncated: false, url: null, commit: TIP, entries: [
          { name: "far", kind: "dir", size: null, section: "files:far" },
          { name: "wide", kind: "dir", size: null, section: "files:wide" },
        ] });
      if (id === "files:far") return one({ type: "tree", trail: [], truncated: false, url: null, commit: TIP, read_alone: true, entries: [{ name: "x", kind: "file", size: 1, section: "file:far/x" }] });
      return one({ type: "tree", trail: [], truncated: true, url: "https://gitea.example/o/r/src/branch/main/wide", commit: TIP, entries: [{ name: "y", kind: "file", size: 1, section: "file:wide/y" }] });
    },
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
  };
  view = mount(TfRepoFiles, {
    target,
    props: { ports, repoId: REPO, at: REPO, branch: "main", path: null, changed: new Set<string>(), treeSections: { dir: "files:", file: "file:" }, title: "o/r" },
  });
  await settle();
  const notes = () => [...target.querySelectorAll(".tf-gonote")].map(text);
  expect(notes()).toEqual([]);
  (target.querySelector('.tf-ftree [data-path="far"]') as HTMLElement).click();
  (target.querySelector('.tf-ftree [data-path="wide"]') as HTMLElement).click();
  await settle();
  expect(notes()).toEqual([
    "Folders past this repository's first 20,000 entries are read one at a time.",
    "wide shows its first 1 entries; the rest are on the code host.",
  ]);
  expect(target.querySelector<HTMLAnchorElement>(".tf-gonote a")?.href).toBe("https://gitea.example/o/r/src/branch/main/wide");
});

/**
 * The forge-waits pass (2026-11-13): after a sync moved the branch, the tab read the file at the new commit from the
 * forge (2.1 s for a large file on the live Gitea), because the source's tree replaced the clone's and the clone was
 * never asked again. The source names its sections by commit here, as Gitea does, so a file at a new commit is a read
 * knobas never made.
 */
describe("a file reopened after a sync moved the branch", () => {
  const SYNCED: Files = { ...FILES, "README.md": "# payout-service, synced\n" };
  const filesAt = (commit: string) => (commit === MOVED ? SYNCED : FILES);
  /** The source's answer with each file's section naming the commit, `file@<commit>:<path>`. */
  function atCommit(section: string, commit: string): SourceView {
    const at = section.match(/^file@([0-9a-f]+):(.*)$/);
    if (at !== null) return sourceAnswer(filesAt(at[1]!), `file:${at[2]}`, at[1]!, `blob-${at[1]!.slice(0, 4)}`);
    const view = sourceAnswer(filesAt(commit), section, commit, "b10b");
    for (const block of view.sections[0]!.blocks) {
      if (block.type === "tree") for (const entry of block.entries) if (entry.section?.startsWith("file:")) entry.section = `file@${commit}:${entry.section.slice(5)}`;
    }
    return view;
  }

  function mountSynced(cloned = true) {
    const handlers = new Map<string, ((event: { payload: unknown }) => void)[]>();
    const state = { top: TIP, clone: cloned ? localTree(FILES) : (null as LocalTree | null), holdFiles: false };
    const held: (() => void)[] = [];
    const ports = {
      entityView: (_at: string, section: string | null) => {
        asked.push({ section: section ?? "", release: () => {} });
        const answer = atCommit(section ?? "", state.top);
        if (state.holdFiles && (section ?? "").startsWith("file@")) return new Promise<SourceView>((resolve) => held.push(() => resolve(answer)));
        return Promise.resolve(answer);
      },
      ...checkoutPorts(() => state.clone),
      localFile: async (_at: string, commit: string, path: string) => {
        localReads.push(`${commit.slice(0, 7)}:${path}`);
        const text = filesAt(commit)[path];
        return text === undefined ? null : fileBlock(path, text, "b10b-local", commit);
      },
      submitWrite: async () => {
        throw { code: "invalid", message: "no write in this suite" };
      },
      listen: async (event: string, handler: (event: { payload: unknown }) => void) => {
        handlers.set(event, [...(handlers.get(event) ?? []), handler]);
        return () => {};
      },
    };
    view = mount(TfRepoFiles, {
      target,
      props: { ports, repoId: REPO, at: REPO, branch: "main", path: "README.md", changed: new Set<string>(), treeSections: { dir: "files:", file: "file:" } },
    });
    const tell = (event: string, payload: unknown) => {
      for (const handler of handlers.get(event) ?? []) handler({ payload });
    };
    return { state, held, tell };
  }
  const fileReads = () => sourceReads().filter((section) => section.startsWith("file@"));
  const meta = () => text(target.querySelector(".tf-fileview .tf-fmeta"));

  test("a clone that holds the new commit draws the tree and the file there at once, with no read of the file on the source", async () => {
    const { state, tell } = mountSynced();
    await settle();
    expect(text(target)).toContain("From your clone at c0ffee0");
    expect(shownFile()).toMatch(/payout-service$/);
    const before = fileReads().length;

    // A sync moved the branch; the clone holds the new commit (a pull, or a fetch behind an earlier read). The app's read
    // behind the kept top finds the branch there.
    state.top = MOVED;
    state.clone = localTree(SYNCED, MOVED);
    tell("view:fresh", { entity_id: REPO, section: "files", failed: null });
    await settle();

    expect(text(target)).toContain("From your clone at d00d000, the same as on");
    expect(shownFile()).toContain("payout-service, synced");
    expect(localReads).toContain("d00d000:README.md");
    expect(fileReads().length, "the file at the new commit came from the clone").toBe(before);
    expect(target.querySelector(".tf-fileview .tf-banner.tf-info"), "a sync's move is quiet").toBeNull();
  });

  test("on a return after the sync, the clone at the mirror's new tip takes over before the source says the branch moved", async () => {
    const handlers = new Map<string, ((event: { payload: unknown }) => void)[]>();
    const state = { clone: localTree(FILES) as LocalTree | null };
    const ports = {
      // The source answers what knobas kept from before the sync; its read behind has not landed.
      entityView: async (_at: string, section: string | null) => {
        asked.push({ section: section ?? "", release: () => {} });
        return atCommit(section ?? "", TIP);
      },
      ...checkoutPorts(() => state.clone),
      localFile: async (_at: string, commit: string, path: string) => {
        localReads.push(`${commit.slice(0, 7)}:${path}`);
        const text = filesAt(commit)[path];
        return text === undefined ? null : fileBlock(path, text, "b10b-local", commit);
      },
      submitWrite: async () => {
        throw { code: "invalid", message: "no write in this suite" };
      },
      listen: async (event: string, handler: (event: { payload: unknown }) => void) => {
        handlers.set(event, [...(handlers.get(event) ?? []), handler]);
        return () => {};
      },
    };
    const props = $state({ ports, repoId: REPO, at: REPO, branch: "main", path: "README.md", changed: new Set<string>(), treeSections: { dir: "files:", file: "file:" }, refreshed: 0 });
    // Under a page that counts its refreshes, as `TfRepo` holds the tab: a return reads the tab again.
    view = mount(TfRepoFilesRefreshed, { target, props: { props } });
    await settle();
    expect(text(target)).toContain("From your clone at c0ffee0");
    const before = fileReads().length;

    // A sync moved the mirror's tip, as of a later sync, and the clone holds it; the reader comes back to the page.
    state.clone = { ...localTree(SYNCED, MOVED), as_of: "2026-11-09T11:00:00Z" };
    props.refreshed += 1;
    await settle();

    expect(text(target)).toContain("From your clone at d00d000, as of the last sync");
    expect(shownFile()).toContain("payout-service, synced");
    expect(fileReads().length, "nothing read from the source at the new commit").toBe(before);
  });

  test("a clone that lacks it keeps the file knobas drew, undimmed under the held line, and the fetch's landing replaces it in place", async () => {
    const { state, held, tell } = mountSynced();
    await settle();
    const before = fileReads().length;

    // The clone lacks the mirror's new tip: the read at it fetches behind (ADR-0041), and the source is slow.
    state.top = MOVED;
    state.clone = null;
    state.holdFiles = true;
    tell("view:fresh", { entity_id: REPO, section: "files", failed: null });
    await settle();

    expect(shownFile()).toMatch(/payout-service$/);
    expect(shownFile()).not.toContain("synced");
    expect(target.querySelector(".tf-fileview [aria-busy='true']"), "the file knobas drew is not dimmed").toBeNull();
    expect(meta()).toBe("As knobas read it a moment ago, at c0ffee0. Checking the code host…");
    expect(fileReads().length).toBe(before + 1);

    state.clone = localTree(SYNCED, MOVED);
    tell("checkout:fetched", { entity_id: REPO, commit: MOVED });
    await settle();
    expect(shownFile()).toContain("payout-service, synced");
    expect(meta()).toContain("at d00d000");
    expect(meta()).not.toContain("Checking");
    expect(localReads).toContain("d00d000:README.md");
    // The forge's late answer lands nowhere.
    for (const release of held) release();
    await settle();
    expect(text(target)).toContain("From your clone at d00d000");
  });

  test("opened after the sync, the source's top knobas held from before it gives way to the clone's tree at the new commit", async () => {
    const files = new Map<string, number>();
    let release!: (tree: LocalTree) => void;
    const late = new Promise<LocalTree>((resolve) => (release = resolve));
    const ports = {
      // What knobas held from before the sync, answered at once (ADR-0042), its read behind still out.
      entityView: async (_at: string, section: string | null) => {
        asked.push({ section: section ?? "", release: () => {} });
        if ((section ?? "").startsWith("file@")) files.set(section!, (files.get(section!) ?? 0) + 1);
        return { ...atCommit(section ?? "", TIP), held_at: "2026-11-09T09:00:00Z" };
      },
      ...checkoutPorts(() => late),
      localFile: async (_at: string, commit: string, path: string) => {
        localReads.push(`${commit.slice(0, 7)}:${path}`);
        const text = filesAt(commit)[path];
        return text === undefined ? null : fileBlock(path, text, "b10b-local", commit);
      },
      submitWrite: async () => {
        throw { code: "invalid", message: "no write in this suite" };
      },
    };
    view = mount(TfRepoFiles, {
      target,
      props: { ports, repoId: REPO, at: REPO, branch: "main", path: "README.md", changed: new Set<string>(), treeSections: { dir: "files:", file: "file:" } },
    });
    await settle();
    // The clone stands at the mirror's tip as of the sync at 10:00, after the source's held read at 09:00.
    release(localTree(SYNCED, MOVED));
    await settle();
    expect(text(target)).toContain("From your clone at d00d000");
    expect(shownFile()).toContain("payout-service, synced");
    expect(localReads).toContain("d00d000:README.md");
    expect([...files.keys()].filter((section) => section.includes(MOVED)), "nothing at the new commit read from the source").toEqual([]);
  });

  test("a deep link's guess read at the commit before the sync does not stand for the clone's file at the commit after it", async () => {
    let release!: (tree: LocalTree) => void;
    const late = new Promise<LocalTree>((resolve) => (release = resolve));
    const ports = {
      // The guess at the file answers at once, from what knobas kept at the commit before; the source's tree never answers.
      entityView: (_at: string, section: string | null) =>
        section === "file:README.md" ? Promise.resolve(atCommit(section, TIP)) : new Promise<SourceView>(() => {}),
      ...checkoutPorts(() => late),
      localFile: async (_at: string, commit: string, path: string) => {
        localReads.push(`${commit.slice(0, 7)}:${path}`);
        const text = filesAt(commit)[path];
        return text === undefined ? null : fileBlock(path, text, "b10b-local", commit);
      },
      submitWrite: async () => {
        throw { code: "invalid", message: "no write in this suite" };
      },
    };
    view = mount(TfRepoFiles, {
      target,
      props: { ports, repoId: REPO, at: REPO, branch: "main", path: "README.md", changed: new Set<string>(), treeSections: { dir: "files:", file: "file:" } },
    });
    await settle();
    expect(shownFile()).toMatch(/payout-service$/);
    release(localTree(SYNCED, MOVED));
    await settle();
    expect(localReads).toEqual(["d00d000:README.md"]);
    expect(shownFile()).toContain("payout-service, synced");
  });

  test("with no clone, the source's answer at the new commit replaces the file knobas drew, quietly", async () => {
    const { state, held, tell } = mountSynced(false);
    await settle();
    state.top = MOVED;
    state.holdFiles = true;
    tell("view:fresh", { entity_id: REPO, section: "files", failed: null });
    await settle();
    expect(meta()).toBe("As knobas read it a moment ago, at c0ffee0. Checking the code host…");
    for (const release of held) release();
    await settle();
    expect(shownFile()).toContain("payout-service, synced");
    expect(meta()).toContain("at d00d000");
    expect(target.querySelector(".tf-fileview .tf-banner.tf-info")).toBeNull();
  });
});
