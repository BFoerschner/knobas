/**
 * The Files tab asks the source for a file once (the inner-pages pass, 2026-11-12): a deep link's guess by path that
 * drew the file stands for the section its listing names at a commit, when the reader comes back to it; and a
 * pointer resting on a row of the tree reads what a press on it opens, which the press then takes. The stand-in
 * names its files by commit in its listings (`file@<sha>:<path>`), as Gitea does, and answers a guess by path too.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { LocalTree, SourceView, TreeEntry, ViewBlock } from "../../ipc/entity";
import { checkoutPorts } from "./checkout-ports.test-support";
import TfRepoFiles from "./TfRepoFiles.svelte";
import { forgetAllDrawnFiles } from "./drawn-files";

const REPO = "gitea:tidewater/payout-service";
const SHA = "c0mmit";
const FILES: Record<string, string> = { "README.md": "# payout\n", "src/a.rs": "fn a() {}\n", "src/b.rs": "fn b() {}\n" };

function listing(dir: string): TreeEntry[] {
  const prefix = dir === "" ? "" : `${dir}/`;
  const dirs = new Set<string>();
  const entries: TreeEntry[] = [];
  for (const [path, text] of Object.entries(FILES)) {
    if (!path.startsWith(prefix)) continue;
    const rest = path.slice(prefix.length);
    if (rest.includes("/")) dirs.add(rest.slice(0, rest.indexOf("/")));
    else entries.push({ name: rest, kind: "file", size: text.length, section: `file@${SHA}:${path}` });
  }
  return [...[...dirs].map((name): TreeEntry => ({ name, kind: "dir", size: null, section: `files@${SHA}:${prefix}${name}` })), ...entries];
}

const reads: string[] = [];
async function entityView(_at: string, section: string | null): Promise<SourceView> {
  const id = section ?? "";
  reads.push(id);
  const one = (blocks: unknown[]) => ({ sections: [{ id, title: "Files", count: null, deferred: false, blocks }], actions: [] }) as unknown as SourceView;
  if (id === "files" || id.startsWith("files:") || id.startsWith(`files@${SHA}:`)) {
    const dir = id === "files" ? "" : id.slice(id.indexOf(":") + 1);
    return one([{ type: "tree", trail: [], entries: listing(dir), truncated: false, url: null, commit: SHA }]);
  }
  const path = id.slice(id.indexOf(":") + 1);
  const text = FILES[path];
  if (text === undefined) throw { code: "not_found", message: `no file ${path}` };
  return one([{ type: "file", trail: [], path, text, language: null, size: text.length, truncated: false, binary: false, url: null, sha: "b10b", commit: SHA }]);
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
const place = $state<{ path: string | null }>({ path: null });

beforeEach(() => {
  reads.length = 0;
  forgetAllDrawnFiles();
  target = document.createElement("div");
  document.body.appendChild(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function draw(path: string | null) {
  place.path = path;
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports: {
        entityView,
        submitWrite: async () => {
          throw { code: "invalid", message: "no write here" };
        },
      },
      repoId: REPO,
      at: REPO,
      branch: "main",
      get path() {
        return place.path;
      },
      line: null,
      changed: new Set<string>(),
      treeSections: { dir: "files:", file: "file:" },
    },
  });
  flushSync();
}

async function settle(ms = 0) {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, ms));
    flushSync();
  }
}

const shownText = () => target.querySelector("pre.tf-code")?.textContent ?? "";

test("a file a deep link's guess drew is not read again under its listing's section when the reader comes back to it", async () => {
  draw("src/a.rs");
  await settle();
  expect(shownText()).toContain("fn a()");
  expect(reads.filter((id) => id.endsWith(":src/a.rs"))).toEqual(["file:src/a.rs"]);
  place.path = "README.md";
  await settle();
  place.path = "src/a.rs";
  await settle();
  expect(shownText()).toContain("fn a()");
  expect(reads.filter((id) => id.endsWith(":src/a.rs")), "the source was asked once for the file").toEqual(["file:src/a.rs"]);
  // Mounted afresh, as a page the kept pages let go: still drawn from what the tab drew.
  unmount(view!);
  view = null;
  draw("src/a.rs");
  await settle();
  expect(shownText()).toContain("fn a()");
  expect(reads.filter((id) => id === `file@${SHA}:src/a.rs`), "the listing's section was never asked of the source").toEqual([]);
});

test("a pointer resting on a file in the tree reads it, and the press takes that read", async () => {
  draw("README.md");
  await settle();
  const link = target.querySelector<HTMLAnchorElement>(`.tf-ftree a[data-path="src/b.rs"]`) ?? null;
  // The directory is folded: open it first.
  if (link === null) {
    target.querySelector<HTMLButtonElement>(`.tf-ftree button[data-path="src"]`)!.click();
    await settle();
  }
  const row = target.querySelector<HTMLAnchorElement>(`.tf-ftree a[data-path="src/b.rs"]`)!;
  row.dispatchEvent(new Event("pointerenter"));
  row.dispatchEvent(new Event("pointerleave"));
  await settle();
  expect(reads, "a pointer passing over asks nothing").not.toContain(`file@${SHA}:src/b.rs`);
  row.dispatchEvent(new Event("pointerenter"));
  await settle(100);
  expect(reads.filter((id) => id === `file@${SHA}:src/b.rs`)).toHaveLength(1);
  place.path = "src/b.rs";
  await settle();
  expect(shownText()).toContain("fn b()");
  expect(reads.filter((id) => id === `file@${SHA}:src/b.rs`), "the press took the read under way").toHaveLength(1);
});

// The inner-6x pass (2026-11-15): the landing's README was asked only once the top listing had named it, a second round
// of the forge. With no checkout's copy it is asked beside the listing, and the listing's file takes that read.
test("a landing with no checkout asks its README beside the top listing, and draws it from that read", async () => {
  let release: () => void = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  place.path = null;
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports: {
        entityView: async (at: string, section: string | null) => {
          if (section === "files") await held;
          return entityView(at, section);
        },
        localDir: async () => null,
        submitWrite: async () => {
          throw { code: "invalid", message: "no write here" };
        },
      },
      repoId: REPO,
      at: REPO,
      branch: "main",
      get path() {
        return place.path;
      },
      line: null,
      changed: new Set<string>(),
      treeSections: { dir: "files:", file: "file:" },
    },
  });
  flushSync();
  await settle();
  // The top listing is still out, and the README already asked.
  expect(target.querySelector(".tf-ftree a")).toBeNull();
  expect(reads).not.toContain("files");
  expect(reads).toContain("file:README.md");
  release();
  await settle();
  expect(target.querySelector(".tf-fileview .tf-md")?.textContent).toContain("payout");
  expect(reads.filter((id) => id.endsWith(":README.md")), "the listing's section was not asked again").toEqual(["file:README.md"]);
});

// The ux14 Windows pass (2026-11-16): asked only once the checkout had said it holds no copy, the README waited a
// second round of IPC behind the checkout's read, about 90 ms of a landing's 372 at 6x on the PC (Fable's ruling that
// day). It is asked at once, beside the checkout's read and the top listing; where the checkout holds a copy, the copy
// draws it, the guess goes unread, and nothing reads the README again.
test("a landing asks its README beside the checkout's read, and a clone's copy draws it", async () => {
  let answer: (tree: LocalTree) => void = () => {};
  const clone = new Promise<LocalTree>((resolve) => (answer = resolve));
  const cloned: string[] = [];
  /** The source's reads as they are asked, before any answers. */
  const asked: string[] = [];
  // The source answers only once the clone has drawn: a forge slower than `git ls-tree`.
  let open: () => void = () => {};
  const gate = new Promise<void>((resolve) => (open = resolve));
  place.path = null;
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports: {
        entityView: async (at: string, section: string | null) => {
          asked.push(section ?? "");
          await gate;
          return entityView(at, section);
        },
        ...checkoutPorts(() => clone),
        localFile: async (_at: string, commit: string, path: string) => {
          cloned.push(path);
          const text = path === "README.md" ? "# payout, the clone's\n" : (FILES[path] ?? "");
          return { type: "file", trail: [], path, text, language: null, size: text.length, truncated: false, binary: false, url: null, sha: "b10b-local", commit } as ViewBlock;
        },
        submitWrite: async () => {
          throw { code: "invalid", message: "no write here" };
        },
      },
      repoId: REPO,
      at: REPO,
      branch: "main",
      get path() {
        return place.path;
      },
      line: null,
      changed: new Set<string>(),
      treeSections: { dir: "files:", file: "file:" },
    },
  });
  flushSync();
  await settle();
  // The checkout has not answered, and the README is asked already.
  expect(asked).toContain("file:README.md");
  answer({
    commit: SHA,
    as_of: "2026-11-16T09:00:00Z",
    truncated: false,
    entries: [
      { path: "README.md", kind: "file", size: 20 },
      { path: "src", kind: "dir", size: null },
      { path: "src/a.rs", kind: "file", size: 10 },
      { path: "src/b.rs", kind: "file", size: 10 },
    ],
  });
  await settle();
  expect(target.querySelector(".tf-fileview .tf-md")?.textContent).toContain("the clone's");
  expect(cloned).toContain("README.md");
  open();
  await settle();
  expect(target.querySelector(".tf-fileview .tf-md")?.textContent).toContain("the clone's");
  expect(asked.filter((id) => id.endsWith(":README.md")), "no read of the README at a commit follows").toEqual(["file:README.md"]);
});
