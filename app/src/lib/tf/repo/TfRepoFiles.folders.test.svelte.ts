/**
 * Folder renderers on the Files tab (ADR-0044): a folder a saved renderer takes gets a link of its own on its tree
 * row, from the backend's marks for one holding a file glob; the address `render/<id>/<folder>` draws the folder in
 * the file's place, opens it in the tree and marks its row; a hidden page asks without starting a run.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { FolderAsk, FolderMarks, FolderRenderer, FolderRenderView } from "../../ipc/app";
import type { SourceView, TreeEntry } from "../../ipc/entity";
import { createRendererDefaults } from "../../render/defaults.svelte";
import { createFolderRenderers } from "../../render/folders.svelte";
import { pageContext } from "../kept-pages.svelte";
import type { FolderRenderPorts } from "../render/TfFolderRender.svelte";
import TfRepoFiles from "./TfRepoFiles.svelte";

const REPO = "gitea:tidewater/payout-service";
const FILES: Record<string, string> = {
  "README.md": "# payout\n",
  "flows/orders/orders.lcproj": "{}",
  "flows/orders/main.flow": "a",
  "flows/notes.txt": "n",
  "docs/a.md": "# a\n",
};

function listing(dir: string): TreeEntry[] {
  const prefix = dir === "" ? "" : `${dir}/`;
  const dirs = new Set<string>();
  const entries: TreeEntry[] = [];
  for (const [path, text] of Object.entries(FILES)) {
    if (!path.startsWith(prefix)) continue;
    const rest = path.slice(prefix.length);
    if (rest.includes("/")) dirs.add(rest.slice(0, rest.indexOf("/")));
    else entries.push({ name: rest, kind: "file", size: text.length, section: `file:${path}` });
  }
  return [...[...dirs].sort().map((name): TreeEntry => ({ name, kind: "dir", size: null, section: `files:${prefix}${name}` })), ...entries];
}

async function entityView(_at: string, section: string | null): Promise<SourceView> {
  const id = section ?? "";
  const one = (blocks: unknown[]) => ({ sections: [{ id, title: "Files", count: null, deferred: false, blocks }], actions: [] }) as unknown as SourceView;
  if (id === "commits") {
    const rows = [{ entity_id: null, title: "Orders flow takes a refund step", meta: "", tone: null, chip: "c90d11a", url: null }];
    return { sections: [{ id, title: "Commits", count: null, deferred: false, blocks: [{ type: "entities", title: null, rows }] }], actions: [] } as unknown as SourceView;
  }
  if (id === "paths") return one([{ type: "tree", trail: [], entries: Object.keys(FILES).map((name) => ({ name, kind: "file", size: 1, section: `file:${name}` })), truncated: false, url: null }]);
  if (id === "files" || id.startsWith("files:")) {
    const dir = id === "files" ? "" : id.slice("files:".length);
    return one([{ type: "tree", trail: [], entries: listing(dir), truncated: false, url: null, paths: "paths" }]);
  }
  const path = id.slice("file:".length);
  const text = FILES[path] ?? "";
  return one([{ type: "file", trail: [], path, text, language: null, size: text.length, truncated: false, binary: false, url: null, sha: "b10b", commit: null }]);
}

const LC: FolderRenderer = { id: "lc", name: "Low code", command: "lc {path} {out}.png", holds: ["*.lcproj"], folders: [], output: "image", timeout_secs: 60 };
const DOCS: FolderRenderer = { id: "docs", name: "Docs site", command: "site {path}", holds: [], folders: ["docs"], output: "html", timeout_secs: 30 };

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
let shown = true;
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  shown = true;
});
afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

function folderPorts() {
  const asked: FolderAsk[] = [];
  const ports: FolderRenderPorts = {
    render: (at) => {
      asked.push(at);
      const answer: FolderRenderView = { renderer: at.renderer, folder: at.folder, drawn: null, current: false, running: { state: "queued" }, failure: null, from: "forge", commit: null };
      return Promise.resolve(answer);
    },
    stop: () => Promise.resolve(),
    leave: () => Promise.resolve(),
    output: () => Promise.resolve(new Uint8Array()),
    frame: () => Promise.resolve("t"),
    listen: () => Promise.resolve(() => {}),
  };
  return { ports, asked };
}

function draw(folderRender: { renderer: string; folder: string } | null, marked: FolderMarks[] = [{ renderer: "lc", folders: ["flows/orders"] }]) {
  const marksAsked: [string, string | null, string | null][] = [];
  const fp = folderPorts();
  app = mount(TfRepoFiles, {
    target,
    context: pageContext(() => shown),
    props: {
      ports: { entityView, submitWrite: async () => ({}) as never },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path: null,
      changed: new Set<string>(),
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
      folders: createFolderRenderers({ read: async () => [LC, DOCS], save: async () => [] }),
      folderMarks: (entityId: string, commit: string | null, paths: string | null) => {
        marksAsked.push([entityId, commit, paths]);
        return Promise.resolve(marked);
      },
      folderPorts: fp.ports,
      folderRender,
    },
  });
  flushSync();
  return { marksAsked, asked: fp.asked };
}

async function settle() {
  for (let round = 0; round < 5; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const rowLink = (path: string) => target.querySelector<HTMLButtonElement>(`.tf-fdir[data-path="${path}"]`)?.nextElementSibling?.querySelector<HTMLAnchorElement>("a") ?? null;

test("a folder a renderer takes gets its link on its row: by name at once, by a file it holds from the marks", async () => {
  const { marksAsked } = draw(null);
  await settle();
  // The marks are asked of the source's whole-tree section (no clone here).
  expect(marksAsked).toContainEqual([REPO, null, "paths"]);
  const docs = rowLink("docs");
  expect(docs?.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/render/docs/docs");
  expect(docs?.getAttribute("title")).toBe("Render with Docs site");
  expect(rowLink("flows")).toBeNull();
  // The landing keeps its last commit over the README.
  expect(target.querySelector(".tf-lastc .tf-lastc-msg")?.textContent).toBe("Orders flow takes a refund step");
  // Open flows: its folder orders holds the project file.
  target.querySelector<HTMLButtonElement>('.tf-fdir[data-path="flows"]')!.click();
  await settle();
  expect(rowLink("flows/orders")?.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/render/lc/flows/orders");
});

test("the address draws the folder in the file's place, opens it in the tree and marks its row", async () => {
  const { asked } = draw({ renderer: "lc", folder: "flows/orders" });
  await settle();
  expect(target.querySelector(".tf-frender header .tf-p")?.textContent).toBe("flows/orders");
  expect(asked[0]).toMatchObject({ entityId: REPO, renderer: "lc", folder: "flows/orders", ask: "open", paths: "paths" });
  expect(target.querySelector('.tf-fdir[data-path="flows/orders"]')?.getAttribute("aria-current")).toBe("true");
  // No file is marked open while a folder is drawn in its place, not even the landing's README.
  expect(target.querySelector('.tf-ftree a[aria-current="true"]')).toBeNull();
  expect(target.querySelector(".tf-lastc")).toBeNull();
});

test("a hidden page only looks, and a renderer that is not saved says so", async () => {
  shown = false;
  const { asked } = draw({ renderer: "lc", folder: "flows/orders" });
  await settle();
  expect(asked.map((a) => a.ask)).toEqual(["look"]);
  unmount(app!);
  app = null;
  shown = true;
  draw({ renderer: "gone", folder: "flows" });
  await settle();
  expect(target.textContent).toContain("There is no folder renderer named “gone”.");
});
