/**
 * Renders on the working tree (ux29; Björn: "In the folder rendering, when I have the working tree selected, it tries to
 * render the folder from the head and not from my local changes"). With *Working tree* chosen every render on the Files
 * tab reads the working tree on disk (`render/read-at.ts`): a folder renderer is asked at `~working-tree`, in the
 * working tree's lane, on the repository, and asks again when git's list moves; the bars say it was made from the
 * working tree; the tree's marks, a pipeline and a bytes renderer read a changed file from disk. A branch reads its
 * commit, as before.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DrawnKind, FolderAsk, FolderRenderer, FolderRenderView, MadeTree, NamedPipeline, ReadFrom, RendererDefault } from "../../ipc/app";
import type { PipeRun, PipeStep, SourceView, ViewBlock } from "../../ipc/entity";
import { createRendererDefaults } from "../../render/defaults.svelte";
import { createFolderRenderers } from "../../render/folders.svelte";
import type { MadePorts } from "../../render/made-ports";
import { createNamedPipelines } from "../../render/pipelines.svelte";
import { pageContext } from "../kept-pages.svelte";
import type { FolderRenderPorts } from "../render/TfFolderRender.svelte";
import { checkoutPorts } from "./checkout-ports.test-support";
import type { LocalTree } from "../../ipc/entity";
import type { WorkingMark } from "./repo";
import TfRepoFiles from "./TfRepoFiles.svelte";

const REPO = "gitea:tidewater/payout-service";
const BRANCH = "gitea:tidewater/payout-service@feature/flows";
const HEAD = "4".repeat(40);
const WT = "~working-tree";

/** The checkout's `HEAD`: `flows/orders` holds the project file; `flows/orders/main.flow` is changed on disk. */
const tree = (): LocalTree => ({
  commit: HEAD,
  as_of: null,
  truncated: false,
  entries: [
    { path: "flows", kind: "dir", size: null },
    { path: "flows/orders", kind: "dir", size: null },
    { path: "flows/orders/orders.lcproj", kind: "file", size: 2 },
    { path: "flows/orders/main.flow", kind: "file", size: 1 },
    { path: "fixtures", kind: "dir", size: null },
    { path: "fixtures/events.json.gz", kind: "file", size: 20 },
    { path: "fixtures/same.json.gz", kind: "file", size: 20 },
    { path: "img", kind: "dir", size: null },
    { path: "img/logo.png", kind: "file", size: 3 },
  ],
});

const SITE: FolderRenderer = { id: "site", name: "Low-code site", command: "lc site {path} {outdir}", holds: [], folders: ["flows/*"], output: "folder", timeout_secs: 60 };
const FLOW: FolderRenderer = { id: "flow", name: "Flow diagram", command: "flowviz {path} {out}", holds: ["*.lcproj"], folders: [], output: "image", timeout_secs: 60 };

const madePorts: MadePorts = {
  tree: (): Promise<MadeTree> => Promise.resolve({ dir: "/s/made", files: [{ path: "index.md", size: 8 }], bytes: 8, left_out: [], left_out_total: 0 }),
  file: (_t, path): Promise<ViewBlock> => Promise.resolve({ type: "file", trail: [], path, text: "# Index\n", language: null, size: 8, truncated: false, binary: false, url: null, sha: null, commit: null }),
  fileBytes: () => Promise.resolve(new TextEncoder().encode("# Index\n")),
};

/** Answers each ask as the backend does: from the working tree at `~working-tree`, else from the clone at the commit. */
function folderPorts() {
  const asked: FolderAsk[] = [];
  const left: string[] = [];
  let answer: (at: FolderAsk) => Promise<FolderRenderView> = (at) => {
    const disk = at.commit === WT;
    const from: ReadFrom = disk ? "working_tree" : "clone";
    const kind: DrawnKind = at.renderer === "site" ? { kind: "folder", files: 1, bytes: 8, left_out: 0 } : { kind: "image", mime: "image/png" };
    return Promise.resolve({
      renderer: at.renderer,
      folder: at.folder,
      drawn: { token: disk ? "W" : "C", kind, size: 8, commit: HEAD, from, took_ms: 3200, drawn_at: "2026-11-21T08:00:00Z" },
      current: true,
      running: null,
      failure: null,
      from,
      commit: HEAD,
    });
  };
  const ports: FolderRenderPorts = {
    render: (at) => {
      asked.push(at);
      return answer(at);
    },
    stop: () => Promise.resolve(),
    leave: (_e, _r, folder, lane) => (left.push(`${folder}:${lane ?? ""}`), Promise.resolve()),
    output: () => Promise.resolve(new Uint8Array([0x89, 0x50, 0x4e, 0x47])),
    frame: () => Promise.resolve("t"),
    listen: () => Promise.resolve(() => {}),
  };
  return { ports, asked, left, answerWith: (next: typeof answer) => (answer = next) };
}

interface Disk {
  head: string;
  marks: ReadonlyMap<string, WorkingMark>;
  stamp: string;
  branch: string | null;
  standing: string | null;
}
const CHANGED: Disk = { head: HEAD, marks: new Map([["flows/orders/main.flow", "M"], ["fixtures/events.json.gz", "M"], ["img/logo.png", "M"]]), stamp: "s1", branch: "main", standing: null };

interface Place {
  at: string;
  path: string | null;
  rendered: { renderer: string; folder: string } | null;
  folderRender: { renderer: string; folder: string } | null;
  disk: Disk | null;
}

const PIPE: NamedPipeline = { id: "gz", name: "Gzipped JSON", steps: [{ step: "gunzip" }] };

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  location.hash = "";
});
afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
  location.hash = "";
});

function draw(start: Partial<Place>, renderers: FolderRenderer[] = [SITE]) {
  const place: Place = $state({ at: REPO, path: null, rendered: null, folderRender: null, disk: CHANGED, ...start });
  const fp = folderPorts();
  const disk = checkoutPorts(tree);
  const bytesRead: string[] = [];
  const marksAsked: string[] = [];
  const piped: { path: string; commit: string | null }[] = [];
  const views: string[] = [];
  const defaults = createRendererDefaults({ read: async () => [], save: async (list) => list });
  defaults.hold([{ pattern: "*.json.gz", renderer: "", pipeline: "gz" } satisfies RendererDefault]);
  const pipelines = createNamedPipelines({ read: async () => [PIPE], save: async () => [] });
  pipelines.hold([PIPE]);
  app = mount(TfRepoFiles, {
    target,
    context: pageContext(() => true),
    props: {
      ports: {
        entityView: async (_at: string, section: string | null) => {
          views.push(section ?? "");
          return { sections: [], actions: [] } as unknown as SourceView;
        },
        submitWrite: async () => ({}) as never,
        piped: async (_e: string, path: string, commit: string | null, _steps: PipeStep[]): Promise<PipeRun> => {
          piped.push({ path, commit });
          return { outcome: "done", name: path, output: { kind: "text", text: "{}" } };
        },
        localDir: disk.localDir,
        localPaths: disk.localPaths,
        localFile: async (_at: string, commit: string, path: string): Promise<ViewBlock> => {
          const binary = path.endsWith(".png") || path.endsWith(".gz");
          return { type: "file", trail: [], path, text: binary ? "" : `${path} at HEAD\n`, language: null, size: 3, truncated: false, binary, url: null, sha: "b".repeat(40), commit };
        },
        fileBytes: async (_e: string, path: string, commit: string | null) => {
          bytesRead.push(`${commit}:${path}`);
          return new Uint8Array([0x89, 0x50, 0x4e, 0x47]);
        },
      },
      repoId: REPO,
      get at() {
        return place.at;
      },
      branch: "main",
      get path() {
        return place.path;
      },
      get rendered() {
        return place.rendered;
      },
      get folderRender() {
        return place.folderRender;
      },
      get disk() {
        return place.disk;
      },
      changed: new Set(["flows/orders/main.flow"]),
      defaults,
      pipelines,
      folders: createFolderRenderers({ read: async () => renderers, save: async () => [] }),
      folderMarks: (entityId: string, commit: string | null) => (marksAsked.push(`${entityId}@${commit}`), Promise.resolve([{ renderer: "flow", folders: ["flows/orders"] }])),
      folderPorts: fp.ports,
      madePorts,
    },
  });
  flushSync();
  return { place, ...fp, bytesRead, marksAsked, piped, views };
}

async function settle() {
  for (let round = 0; round < 6; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const RENDERED = { renderer: "site", folder: "flows/orders" };

test("on the working tree a folder of files is rendered from the working tree on disk, in its lane, on the repository", async () => {
  const { asked } = draw({ at: BRANCH, rendered: RENDERED });
  await settle();
  expect(asked.length).toBeGreaterThan(0);
  for (const ask of asked) expect(ask).toMatchObject({ entityId: REPO, commit: WT, paths: null, lane: "cmp:~working-tree" });
  expect(asked[0]!.ask).toBe("open");
  // The bar says where it was made from, never a commit, and no "your changes are not in it".
  const status = text(target.querySelector(".tf-madebar [role=status]"));
  expect(status).toBe("Made by Low-code site from your working tree, 1 file changed; 1 file, 0.0 MB, 3.2s");
  expect(status).not.toContain("4444444");
  expect(status).not.toContain("not in it");
});

test("Run again on the working tree runs on the working tree", async () => {
  const { asked } = draw({ rendered: RENDERED });
  await settle();
  asked.length = 0;
  [...target.querySelectorAll<HTMLButtonElement>(".tf-madebar button")].find((b) => text(b) === "Run again")!.click();
  await settle();
  expect(asked.map((a) => [a.ask, a.commit, a.lane])).toEqual([["again", WT, "cmp:~working-tree"]]);
});

test("a single drawing on the working tree is asked there and its header says so, with no commit banner", async () => {
  const { asked } = draw({ folderRender: { renderer: "flow", folder: "flows/orders" } }, [FLOW]);
  await settle();
  expect(asked.at(-1)).toMatchObject({ entityId: REPO, renderer: "flow", commit: WT, lane: "cmp:~working-tree", ask: "open" });
  expect(text(target.querySelector(".tf-frender .tf-fmeta"))).toBe("Drawn by Flow diagram from your working tree, 1 file changed, 3.2s");
  expect(target.querySelector(".tf-frender .tf-banner.tf-warn")).toBeNull();
});

test("a change on disk (a new stamp) asks again and keeps the drawing until the answer, with no leave", async () => {
  const { place, asked, left, answerWith } = draw({ rendered: RENDERED });
  await settle();
  const before = asked.length;
  let release!: () => void;
  const held = new Promise<void>((resolve) => (release = resolve));
  answerWith(async (at) => {
    await held;
    return { renderer: at.renderer, folder: at.folder, drawn: { token: "W", kind: { kind: "folder", files: 1, bytes: 8, left_out: 0 }, size: 8, commit: HEAD, from: "working_tree", took_ms: 3200, drawn_at: "2026-11-21T08:00:00Z" }, current: false, running: { state: "queued" }, failure: null, from: "working_tree", commit: HEAD };
  });
  place.disk = { ...CHANGED, stamp: "s2" };
  await settle();
  expect(asked.slice(before).map((a) => [a.ask, a.commit])).toEqual([["open", WT]]);
  // Still drawn while the answer is on its way: the bar did not go back to "Asking…".
  expect(text(target.querySelector(".tf-madebar [role=status]"))).toContain("from your working tree");
  expect(left).toEqual([]);
  release();
  await settle();
  expect(text(target.querySelector(".tf-madebar [role=status]"))).toBe("Low-code site waits for another folder renderer to finish.");
});

test("an earlier drawing on the working tree says it is from the working tree and that it changed since", async () => {
  const { answerWith, place } = draw({ rendered: RENDERED });
  await settle();
  answerWith(async (at) => ({
    renderer: at.renderer,
    folder: at.folder,
    drawn: { token: "W", kind: { kind: "folder", files: 1, bytes: 8, left_out: 0 }, size: 8, commit: HEAD, from: "working_tree", took_ms: 3200, drawn_at: new Date(Date.now() - 5 * 60_000).toISOString() },
    current: false,
    running: null,
    failure: null,
    from: "working_tree",
    commit: HEAD,
  }));
  place.disk = { ...CHANGED, stamp: "s3" };
  await settle();
  expect(text(target.querySelector(".tf-madebar [role=status]"))).toMatch(/^Made from your working tree .+ ago\. Your working tree changed since\.$/);
});

test("choosing a branch again renders the branch's commit, in the Files tab's own lane", async () => {
  const { place, asked } = draw({ at: BRANCH, rendered: RENDERED });
  await settle();
  asked.length = 0;
  place.disk = null;
  await settle();
  expect(asked.length).toBeGreaterThan(0);
  const last = asked.at(-1)!;
  expect(last.entityId).toBe(BRANCH);
  expect(last.commit).toBe(HEAD);
  expect(last.lane).toBeUndefined();
  expect(text(target.querySelector(".tf-madebar [role=status]"))).toContain("at 4444444");
});

test("the tree's marks are asked at the working tree", async () => {
  const { marksAsked } = draw({}, [FLOW]);
  await settle();
  expect(marksAsked).toEqual([`${REPO}@${WT}`]);
});

test("a pipeline on a changed file runs on it as it is on disk; on an unchanged one at HEAD", async () => {
  const { piped, place } = draw({ path: "fixtures/events.json.gz" });
  await settle();
  expect(piped).toEqual([{ path: "fixtures/events.json.gz", commit: WT }]);
  place.path = "fixtures/same.json.gz";
  await settle();
  expect(piped.at(-1)).toEqual({ path: "fixtures/same.json.gz", commit: HEAD });
});

test("a changed image's bytes are read from disk, and read again when it changes again", async () => {
  const { bytesRead, place } = draw({ path: "img/logo.png" });
  await settle();
  expect(bytesRead.filter((r) => r.endsWith("img/logo.png"))).toEqual([`${WT}:img/logo.png`, `${WT}:img/logo.png`]);
  const was = bytesRead.length;
  place.disk = { ...CHANGED, stamp: "s4" };
  await settle();
  expect(bytesRead.slice(was).filter((r) => r === `${WT}:img/logo.png`).length).toBeGreaterThan(0);
});
