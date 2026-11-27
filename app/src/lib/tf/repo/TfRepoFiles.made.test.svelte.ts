/**
 * A generated folder on the Files tab (ADR-0046): a folder of files renderer's glyph opens the `rendered` address, which
 * grafts the generated rows in the folder's place under a band, and draws a generated file in the normal file view with
 * the made bar over it. The graft holds while the reader opens files elsewhere and goes on *Real files*, a real file
 * under the folder, or a new branch.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { FolderAsk, FolderRenderer, FolderRenderView, MadeTree } from "../../ipc/app";
import type { SourceView, TreeEntry, ViewBlock } from "../../ipc/entity";
import { createRendererDefaults } from "../../render/defaults.svelte";
import { createFolderRenderers } from "../../render/folders.svelte";
import type { MadePorts } from "../../render/made-ports";
import { pageContext } from "../kept-pages.svelte";
import type { FolderRenderPorts } from "../render/TfFolderRender.svelte";
import TfRepoFiles from "./TfRepoFiles.svelte";
import { folderPicks } from "../../render/folder-picks.svelte";
import { jarAnswers } from "../../render/jar-fixture";
import type { ArchivePorts } from "../../render/jar-ports";

const REPO = "gitea:tidewater/payout-service";
const HASH = "#/repo/gitea%3Atidewater%2Fpayout-service";
const FILES: Record<string, string> = {
  "README.md": "# payout\n",
  "flows/orders/orders.lcproj": "{}",
  "flows/orders/main.flow": "a",
  "flows/notes.txt": "n",
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

let views = 0;
let bytesOfRepo = 0;
async function entityView(_at: string, section: string | null): Promise<SourceView> {
  views += 1;
  const id = section ?? "";
  const one = (blocks: unknown[]) => ({ sections: [{ id, title: "Files", count: null, deferred: false, blocks }], actions: [] }) as unknown as SourceView;
  if (id === "commits") return { sections: [{ id, title: "Commits", count: null, deferred: false, blocks: [{ type: "entities", title: null, rows: [] }] }], actions: [] } as unknown as SourceView;
  if (id === "files" || id.startsWith("files:")) {
    const dir = id === "files" ? "" : id.slice("files:".length);
    return one([{ type: "tree", trail: [], entries: listing(dir), truncated: false, url: null }]);
  }
  const path = id.slice("file:".length);
  const text = FILES[path] ?? "";
  return one([{ type: "file", trail: [], path, text, language: null, size: text.length, truncated: false, binary: false, url: null, sha: "b10b", commit: null }]);
}

const SITE: FolderRenderer = { id: "site", name: "Low-code site", command: "lc site {path} {outdir}", holds: [], folders: ["flows/*"], output: "folder", timeout_secs: 60 };
/** A second renderer on the same folders (ux26, the pick): a drawing, not a folder of files. */
const FLOW: FolderRenderer = { id: "flow", name: "Flow diagram", command: "C:\\Tools\\flowviz\\flowviz.exe {path} {out}", holds: [], folders: ["flows/*"], output: "image", timeout_secs: 60 };
let savedRenderers: FolderRenderer[] = [SITE];

/** What each run made, by token. */
const MADE: Record<string, Record<string, string>> = {
  A: {
    "README.md": "# Orders, generated\n\n![flow](img/flow.png)\n\nSee [the steps](docs/steps.md) and [the repository](../../README.md).\n",
    "index.md": "# Index\n",
    "docs/steps.md": "# Steps\n",
    "img/flow.png": "PNG",
  },
  B: {
    "README.md": "# Orders, generated again\n",
    "index.md": "# Index\n",
    "docs/steps.md": "# Steps\n",
    "img/flow.png": "PNG",
  },
  /** A run whose output holds an archive (a build's JAR). */
  J: {
    "app.jar": "PK\u0003\u0004",
    "index.md": "# Index\n",
  },
};
const treeOf = (token: string): MadeTree => ({
  dir: `/s/knobas-rendered/x/orders.site.${token}`,
  files: Object.entries(MADE[token]!)
    .map(([path, text]) => ({ path, size: text.length }))
    .sort((a, b) => (a.path < b.path ? -1 : 1)),
  bytes: 1,
  left_out: [{ path: ".git", why: "git" }],
  left_out_total: 1,
});

let token = "A";
let madeBytes: string[] = [];
const madePorts: MadePorts = {
  tree: (t) => Promise.resolve(treeOf(t)),
  file: (t, path): Promise<ViewBlock> => {
    const text = MADE[t]?.[path];
    if (text === undefined) return Promise.reject({ code: "not_found", message: "no" });
    const binary = path.endsWith(".png") || path.endsWith(".jar");
    return Promise.resolve({ type: "file", trail: [], path, text: binary ? "" : text, language: null, size: text.length, truncated: false, binary, url: null, sha: null, commit: null });
  },
  fileBytes: (t, path) => {
    madeBytes.push(`${t}:${path}`);
    return Promise.resolve(new TextEncoder().encode(MADE[t]?.[path] ?? ""));
  },
  archive: (t, path) => {
    archivesOpened.push(`${t}:${path}`);
    return archiveCall("archive_open", { path: "build/libs/ledger-core-2.3.1.jar" });
  },
};
let archivesOpened: string[] = [];
const archiveAnswers = jarAnswers() as unknown as Record<string, (args: Record<string, unknown>) => unknown>;
const archiveCall = <T>(name: string, args: Record<string, unknown>) => Promise.resolve(archiveAnswers[name]!(args) as T);
/** The JAR view's reads by handle, as the repository page hands them down; `open` is never asked for a generated file. */
const archive: ArchivePorts = {
  open: () => Promise.reject(new Error("a generated archive is listed through the made ports")),
  nested: (handle, entry) => archiveCall("archive_nested", { handle, entry }),
  entry: (handle, entry) => archiveCall("archive_entry", { handle, entry }),
  cls: (handle, entry) => archiveCall("archive_class", { handle, entry }),
  decompile: (handle, entry) => archiveCall("archive_decompile", { handle, entry }),
  download: (handle, entry) => archiveCall("archive_download", { handle, entry }),
  java: () => archiveCall("java_runtime", {}),
};

function folderPorts() {
  const asked: FolderAsk[] = [];
  const ports: FolderRenderPorts = {
    render: (at) => {
      asked.push(at);
      const answer: FolderRenderView = {
        renderer: at.renderer,
        folder: at.folder,
        drawn: { token, kind: { kind: "folder", files: 4, bytes: 30, left_out: 1 }, size: 30, commit: "c90d11aaaa", from: "clone", took_ms: 3200, drawn_at: "2026-11-20T08:00:00Z" },
        current: true,
        running: null,
        failure: null,
        from: "clone",
        commit: "c90d11aaaa",
      };
      return Promise.resolve(answer);
    },
    stop: () => Promise.resolve(),
    leave: () => Promise.resolve(),
    output: () => Promise.reject(new Error("a folder kind has no output")),
    frame: () => Promise.resolve("t"),
    listen: () => Promise.resolve(() => {}),
  };
  return { ports, asked };
}

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  views = 0;
  bytesOfRepo = 0;
  madeBytes = [];
  archivesOpened = [];
  token = "A";
  location.hash = "";
});
afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
  location.hash = "";
});

interface Place {
  at: string;
  path: string | null;
  rendered: { renderer: string; folder: string } | null;
  folderRender: { renderer: string; folder: string } | null;
  changed: Set<string>;
}

function draw(start: Partial<Place>, archives: ArchivePorts | null = archive) {
  const place: Place = $state({ at: REPO, path: null, rendered: null, folderRender: null, changed: new Set<string>(), ...start });
  const fp = folderPorts();
  app = mount(TfRepoFiles, {
    target,
    context: pageContext(() => true),
    props: {
      ports: {
        entityView,
        submitWrite: async () => ({}) as never,
        piped: async () => ({}) as never,
        fileBytes: async () => {
          bytesOfRepo += 1;
          return new Uint8Array();
        },
        ...(archives === null ? {} : { archive: archives }),
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
      get changed() {
        return place.changed;
      },
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
      folders: createFolderRenderers({ read: async () => savedRenderers, save: async () => [] }),
      folderMarks: () => Promise.resolve([]),
      folderPorts: fp.ports,
      madePorts,
    },
  });
  flushSync();
  return { place, asked: fp.asked };
}

async function settle() {
  for (let round = 0; round < 6; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const rowPaths = () => [...target.querySelectorAll<HTMLElement>(".tf-ftree [data-path]")].map((el) => el.dataset["path"]);
const band = () => target.querySelector<HTMLButtonElement>(".tf-ftree .tf-fband");
const glyph = (path: string) => target.querySelector<HTMLButtonElement>(`.tf-fdir[data-path="${path}"]`)?.nextElementSibling?.querySelector<HTMLAnchorElement>("a") ?? null;
const RENDERED = { renderer: "site", folder: "flows/orders" };

test("two renderers on a folder and no pick: the glyph opens a menu; choosing one keeps the pick and opens it (ux26)", async () => {
  savedRenderers = [SITE, FLOW];
  try {
    draw({});
    await settle();
    target.querySelector<HTMLButtonElement>('.tf-fdir[data-path="flows"]')!.click();
    await settle();
    const mark = target.querySelector<HTMLElement>('.tf-fdir[data-path="flows/orders"]')!.nextElementSibling!;
    expect(mark.querySelector("a"), "no link while the folder asks which").toBeNull();
    const button = mark.querySelector<HTMLButtonElement>("button")!;
    expect(button.getAttribute("aria-haspopup")).toBe("menu");
    button.click();
    flushSync();
    const items = [...document.querySelectorAll<HTMLButtonElement>(".tf-frpick-m button[role=menuitemradio]")];
    expect(items.map((b) => b.querySelector(".tf-rsw-nm")?.textContent)).toEqual(["Low-code site", "Flow diagram"]);
    // A program, never a command line (the 2026-11-19 rule).
    expect(items[1]!.querySelector(".tf-rsw-sub")?.getAttribute("title")).toBe(FLOW.command);
    items[1]!.click();
    flushSync();
    expect(location.hash).toBe(`${HASH}/render/flow/flows/orders`);
    expect(folderPicks.of(REPO, "flows/orders")).toBe("flow");
    await settle();
    expect(glyph("flows/orders")?.getAttribute("href"), "picked: a plain link to it again").toBe(`${HASH}/render/flow/flows/orders`);
  } finally {
    savedRenderers = [SITE];
  }
});

test("a folder of files renderer's glyph links to the generated folder's address", async () => {
  draw({});
  await settle();
  target.querySelector<HTMLButtonElement>('.tf-fdir[data-path="flows"]')!.click();
  await settle();
  expect(glyph("flows/orders")?.getAttribute("href")).toBe(`${HASH}/rendered/site/flows%2Forders`);
});

test("at its address the band and the generated rows stand under the folder's row, the real ones gone", async () => {
  const { asked } = draw({ rendered: RENDERED });
  await settle();
  expect(asked[0]).toMatchObject({ entityId: REPO, renderer: "site", folder: "flows/orders", ask: "open" });
  const rows = rowPaths();
  const at = rows.indexOf("flows/orders");
  expect(rows.slice(at, at + 6)).toEqual([
    "flows/orders",
    "flows/orders\u0000band",
    "flows/orders/docs",
    "flows/orders/img",
    "flows/orders/index.md",
    "flows/orders/README.md",
  ]);
  expect(rows).not.toContain("flows/orders/main.flow");
  expect(rows).toContain("flows/notes.txt");
  expect(band()?.textContent).toContain("Made by Low-code site");
  expect(band()?.textContent).toContain("Real files");
  // The generated rows carry the rail and no glyph; the folder's own glyph is pressed.
  const docs = target.querySelector<HTMLElement>('.tf-fdir[data-path="flows/orders/docs"]')!;
  expect(docs.hasAttribute("data-made")).toBe(true);
  expect(docs.classList.contains("tf-made")).toBe(true);
  expect(glyph("flows/orders/docs")).toBeNull();
  expect(glyph("flows/orders")?.classList.contains("tf-on")).toBe(true);
  // The made bar says who made it, from which commit, how many files.
  expect(target.querySelector(".tf-madebar [role='status']")?.textContent).toContain("Made by Low-code site at c90d11a, 4 files");
  // The landing opens the generated README, in the Markdown component, with its links among the generated files.
  expect(target.querySelector(".tf-rendered h1")?.textContent).toBe("Orders, generated");
  const links = [...target.querySelectorAll<HTMLAnchorElement>(".tf-rendered a")].map((a) => a.getAttribute("href"));
  expect(links).toContain(`${HASH}/rendered/site/flows%2Forders/files/docs/steps.md`);
  expect(links).toContain(`${HASH}/files/README.md`);
  // Its image is the generated one, read by the token, never the repository's.
  expect(madeBytes).toContain("A:img/flow.png");
  expect(bytesOfRepo).toBe(0);
  // No last commit, no Edit, no Pipeline over a generated file.
  expect(target.querySelector(".tf-lastc")).toBeNull();
  expect(target.textContent).not.toContain("Pipeline");
  expect(target.querySelector(".tf-fileview header [aria-label='More']")).toBeNull();
});

test("a generated file's rows link to its address, and a file both trees hold is not marked changed", async () => {
  draw({ rendered: { ...RENDERED }, changed: new Set(["flows/orders/README.md"]) });
  await settle();
  const readme = target.querySelector<HTMLAnchorElement>('.tf-ftree a[data-path="flows/orders/README.md"]')!;
  expect(readme.getAttribute("href")).toBe(`${HASH}/rendered/site/flows%2Forders/files/README.md`);
  expect(readme.classList.contains("tf-changed")).toBe(false);
  expect(target.textContent).not.toContain("Your checkout has an uncommitted change to this file.");
});

test("Real files brings the real rows back with no read of the source", async () => {
  draw({ rendered: RENDERED });
  await settle();
  const before = views;
  band()!.click();
  await settle();
  const rows = rowPaths();
  expect(rows).toContain("flows/orders/main.flow");
  expect(rows).not.toContain("flows/orders\u0000band");
  expect(views, "the real listing was kept, not read again").toBe(before);
  expect(location.hash).toBe(`${HASH}`);
});

test("the graft holds while a file elsewhere opens, and goes for a real file under its folder or a new branch", async () => {
  const { place } = draw({ rendered: RENDERED });
  await settle();
  place.rendered = null;
  place.path = "README.md";
  await settle();
  expect(rowPaths()).toContain("flows/orders\u0000band");
  place.path = "flows/orders/main.flow";
  await settle();
  expect(rowPaths()).not.toContain("flows/orders\u0000band");
  expect(rowPaths()).toContain("flows/orders/main.flow");

  place.path = null;
  place.rendered = RENDERED;
  await settle();
  expect(rowPaths()).toContain("flows/orders\u0000band");
  place.rendered = null;
  place.path = "README.md";
  place.at = `${REPO}@branch:feature`;
  await settle();
  expect(rowPaths()).not.toContain("flows/orders\u0000band");
});

test("the arrow keys step from the folder's row onto the band, and Enter there shows the real files", async () => {
  draw({ rendered: RENDERED });
  await settle();
  const folderRow = target.querySelector<HTMLButtonElement>('.tf-fdir[data-path="flows/orders"]')!;
  folderRow.focus();
  folderRow.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  flushSync();
  expect(document.activeElement).toBe(band());
  // A button's Enter is its click.
  band()!.click();
  await settle();
  expect(rowPaths()).toContain("flows/orders/main.flow");
});

test("a generated JAR opens in the JAR view, listed from the generated folder, never through the byte read", async () => {
  token = "J";
  draw({ rendered: RENDERED, path: "app.jar" });
  await vi.waitFor(
    () => {
      flushSync();
      expect(target.querySelector(".tf-jar-name")).not.toBeNull();
    },
    { timeout: 10_000, interval: 20 },
  );
  expect(archivesOpened).toEqual(["J:app.jar"]);
  expect(madeBytes).toEqual([]);
  expect(target.textContent).not.toContain("A binary file, not drawn.");
});

test("a generated JAR where no archive read is wired stays a binary file, not drawn", async () => {
  token = "J";
  draw({ rendered: RENDERED, path: "app.jar" }, null);
  await settle();
  expect(target.querySelector(".tf-jar-name")).toBeNull();
  expect(archivesOpened).toEqual([]);
  expect(target.textContent).toContain("A binary file, not drawn.");
});

test("a generated file the run did not make says so, with the way to the folder", async () => {
  draw({ rendered: RENDERED, path: "nope.md" });
  await settle();
  expect(target.textContent).toContain("Low-code site made no file at this path.");
});

test("a re-run with the same files keeps every row's node, and swaps in the new tree's files", async () => {
  const { place } = draw({ rendered: RENDERED, path: "docs/steps.md" });
  await settle();
  const before = target.querySelector('.tf-ftree a[data-path="flows/orders/index.md"]');
  expect(before).not.toBeNull();
  token = "B";
  place.rendered = { ...RENDERED };
  place.path = "README.md";
  await settle();
  expect(target.querySelector('.tf-ftree a[data-path="flows/orders/index.md"]')).toBe(before);
  expect(target.querySelector(".tf-rendered h1")?.textContent).toBe("Orders, generated again");
});

/**
 * Björn, 2026-11-23: "re-rendering with a new tool, i can't see a java-folder and its files inside the renderings but I
 * can select the .java file with the T search, but the folder stays invisible". The new run's files lie under folders
 * that hold folders alone (`src/main/java/demo`), which the tab's index named nowhere.
 */
test("a re-run that adds a folder under folders alone draws it, and Go to file opens the folders down to its file", async () => {
  MADE["D"] = { "README.md": "# Orders\n" };
  MADE["E"] = { "README.md": "# Orders\n", "src/main/java/demo/Greeter.java": "class Greeter {}\n" };
  try {
    token = "D";
    const { place } = draw({ rendered: RENDERED });
    await settle();
    expect(rowPaths().filter((p) => p!.startsWith("flows/orders/"))).toEqual(["flows/orders/README.md"]);

    // *Run again*, the tool now writing Java sources too.
    token = "E";
    target.querySelector<HTMLButtonElement>(".tf-madebar-run")!.click();
    await settle();
    expect(target.querySelector(".tf-madebar [role='status']")?.textContent).toContain("Made by Low-code site");
    const src = target.querySelector<HTMLButtonElement>('.tf-fdir[data-path="flows/orders/src"]');
    expect(src, "the new folder has its row under the band").not.toBeNull();
    expect(src!.getAttribute("aria-expanded")).toBe("false");
    expect(src!.hasAttribute("data-made")).toBe(true);

    // Go to file finds it, and choosing it opens every folder above it, its row the current one.
    const field = target.querySelector<HTMLInputElement>('input[aria-label="Go to file"]')!;
    field.value = "greeter";
    field.dispatchEvent(new Event("input", { bubbles: true }));
    await settle();
    const found = target.querySelector<HTMLAnchorElement>("a.tf-gotof")!;
    expect(found.getAttribute("href")).toBe(`${HASH}/rendered/site/flows%2Forders/files/src/main/java/demo/Greeter.java`);
    // The address it names, as the router hands it down.
    found.click();
    place.path = "src/main/java/demo/Greeter.java";
    await settle();
    const rows = rowPaths();
    const at = rows.indexOf("flows/orders");
    expect(rows.slice(at, at + 8)).toEqual([
      "flows/orders",
      "flows/orders\u0000band",
      "flows/orders/src",
      "flows/orders/src/main",
      "flows/orders/src/main/java",
      "flows/orders/src/main/java/demo",
      "flows/orders/src/main/java/demo/Greeter.java",
      "flows/orders/README.md",
    ]);
    expect(target.querySelector('.tf-ftree a[data-path="flows/orders/src/main/java/demo/Greeter.java"]')?.getAttribute("aria-current")).toBe("true");

    // Folding and unfolding a folder on the way is the reader's, as for any row.
    target.querySelector<HTMLButtonElement>('.tf-fdir[data-path="flows/orders/src/main"]')!.click();
    flushSync();
    expect(rowPaths()).not.toContain("flows/orders/src/main/java");
    expect(rowPaths()).toContain("flows/orders/src/main");
  } finally {
    delete MADE["D"];
    delete MADE["E"];
  }
});

test("a render/ address of a folder of files renderer is drawn as its generated folder, with no redirect", async () => {
  draw({ folderRender: RENDERED });
  await settle();
  expect(rowPaths()).toContain("flows/orders\u0000band");
  expect(target.querySelector(".tf-frender")).toBeNull();
  expect(location.hash).toBe("");
});

test("Go to file with a graft on finds the generated files at their own address, and not the real ones they hide", async () => {
  draw({ rendered: RENDERED });
  await settle();
  const field = target.querySelector<HTMLInputElement>('input[aria-label="Go to file"]')!;
  field.value = "index";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  const found = [...target.querySelectorAll<HTMLAnchorElement>("a.tf-gotof")].map((a) => a.getAttribute("href"));
  expect(found).toEqual([`${HASH}/rendered/site/flows%2Forders/files/index.md`]);
  field.value = "main.flow";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(target.querySelectorAll("a.tf-gotof")).toHaveLength(0);
});

test("a generated folder of 3,000 files mounts fewer than 200 rows", async () => {
  const bulk: Record<string, string> = {};
  for (let i = 0; i < 3000; i += 1) bulk[`bulk/part-${String(i).padStart(4, "0")}.txt`] = "x";
  MADE["C"] = { "README.md": "# Bulk\n", ...bulk };
  token = "C";
  const { place } = draw({ rendered: RENDERED });
  await settle();
  place.rendered = { ...RENDERED };
  target.querySelector<HTMLButtonElement>('.tf-fdir[data-path="flows/orders/bulk"]')!.click();
  await settle();
  expect(target.querySelectorAll(".tf-ftree [data-path]").length).toBeLessThan(200);
  delete MADE["C"];
});

/** Ports whose first answer is a run going, and whose run ends when `end()` is called (Fable's ruling, 2026-11-20). */
function runningPorts() {
  let ended = false;
  let heard: ((event: { payload: unknown }) => void) | null = null;
  const base = folderPorts().ports;
  const ports: FolderRenderPorts = {
    ...base,
    render: (at) =>
      base.render(at).then((view) => (ended ? view : { ...view, drawn: null, current: false, running: { state: "running", elapsed_ms: 0 } })),
    listen: (_event, handler) => {
      heard = handler;
      return Promise.resolve(() => {});
    },
  };
  const end = () => {
    ended = true;
    heard?.({ payload: { entity_id: REPO, renderer: "site", folder: "flows/orders", running: null, lane: "" } });
  };
  return { ports, end };
}

test("the first rows land in one write: the band and the real rows keep their nodes, and the bar's busy clears with them", async () => {
  const run = runningPorts();
  const place: Place = $state({ at: REPO, path: null, rendered: RENDERED, folderRender: null, changed: new Set<string>() });
  app = mount(TfRepoFiles, {
    target,
    context: pageContext(() => true),
    props: {
      ports: { entityView, submitWrite: async () => ({}) as never },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path: null,
      get rendered() {
        return place.rendered;
      },
      changed: new Set<string>(),
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
      folders: createFolderRenderers({ read: async () => [SITE], save: async () => [] }),
      folderMarks: () => Promise.resolve([]),
      folderPorts: run.ports,
      madePorts,
    },
  });
  await settle();
  const bandNode = band();
  const below = target.querySelector('.tf-ftree a[data-path="README.md"]');
  expect(bandNode?.textContent).toContain("is running");
  expect(target.querySelector(".tf-madebar")?.getAttribute("aria-busy")).toBe("true");
  // In every state the DOM reaches, a generated row and a busy bar never stand together, nor a free bar and no rows.
  const seen: string[] = [];
  const watch = new MutationObserver(() => {
    const rows = target.querySelector(".tf-ftree [data-made]") !== null;
    const busy = target.querySelector(".tf-madebar")?.getAttribute("aria-busy") === "true";
    if (rows === busy) seen.push(`rows ${rows}, busy ${busy}`);
  });
  watch.observe(target, { subtree: true, childList: true, attributes: true, attributeFilter: ["aria-busy"] });
  run.end();
  await settle();
  watch.disconnect();
  expect(seen).toEqual([]);
  expect(target.querySelector(".tf-ftree [data-made]")).not.toBeNull();
  expect(band(), "the band keeps its node").toBe(bandNode);
  expect(target.querySelector('.tf-ftree a[data-path="README.md"]'), "a real row below keeps its node").toBe(below);
});

test("the first rows keep the view when the band is scrolled above it", async () => {
  const run = runningPorts();
  // Layout for jsdom: each tree row 24 px down in the tree's order, less the tree's scroll; the tree's top at 0.
  const rect = Element.prototype.getBoundingClientRect;
  let scrolled = 0;
  Element.prototype.getBoundingClientRect = function (this: Element) {
    const box = this.closest(".tf-ftree");
    if (box !== null && this !== box && (this as HTMLElement).dataset?.["path"] !== undefined) {
      const at = [...box.querySelectorAll("[data-path]")].indexOf(this);
      const top = at * 24 - scrolled;
      return { top, bottom: top + 24, left: 0, right: 200, width: 200, height: 24, x: 0, y: top, toJSON: () => ({}) } as DOMRect;
    }
    return rect.call(this);
  };
  try {
    const place: Place = $state({ at: REPO, path: null, rendered: RENDERED, folderRender: null, changed: new Set<string>() });
    app = mount(TfRepoFiles, {
      target,
      context: pageContext(() => true),
      props: {
        ports: { entityView, submitWrite: async () => ({}) as never },
        repoId: REPO,
        at: REPO,
        branch: "main",
        path: null,
        get rendered() {
          return place.rendered;
        },
        changed: new Set<string>(),
        defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
        folders: createFolderRenderers({ read: async () => [SITE], save: async () => [] }),
        folderMarks: () => Promise.resolve([]),
        folderPorts: run.ports,
        madePorts,
      },
    });
    await settle();
    const tree = target.querySelector<HTMLElement>(".tf-ftree")!;
    Object.defineProperty(tree, "scrollTop", { get: () => scrolled, set: (v: number) => (scrolled = v), configurable: true });
    // The band is the third row; scrolled so the first row in view is the one after it, a real row below the graft.
    scrolled = 3 * 24 + 1;
    const anchor = [...tree.querySelectorAll<HTMLElement>("[data-path]")][3]!;
    const topBefore = anchor.getBoundingClientRect().top;
    run.end();
    await settle();
    expect(anchor.isConnected).toBe(true);
    expect(anchor.getBoundingClientRect().top, "the row in view stays where it was").toBe(topBefore);
  } finally {
    Element.prototype.getBoundingClientRect = rect;
  }
});

test("the bar's Run… runs on the generated folder knobas kept, its files and its token, never the real folder (Björn, 2026-11-25)", async () => {
  const { oneoffs } = await import("../../render/oneoff.svelte");
  const base = { stdin: "none" as const, folders: [], has: [], repo_holds: [], repos: [], shows: "auto" as const, start_in: "root" as const, path: [], env: [], colours: true, timeout_secs: 120, wsl_paths: false };
  const had = oneoffs.commands;
  oneoffs.commands = [
    { ...base, id: "mdlint", name: "Markdown lint", command: "mdlint {file}", files: ["*.md"] },
    { ...base, id: "tree", name: "Tree", command: "tree {dir}", files: [] },
    { ...base, id: "blame", name: "Blame", command: "blame {dir} {branch}", files: [] },
  ];
  try {
    draw({ rendered: RENDERED });
    await settle();
    expect(oneoffs.madeOf(REPO, "site", "flows/orders")).toMatchObject({ token: "A", dir: "/s/knobas-rendered/x/orders.site.A" });
    const face = target.querySelector<HTMLButtonElement>(".tf-madebar .tf-oo-runin button")!;
    expect(face.disabled).toBe(false);
    face.click();
    flushSync();
    const rows = [...document.querySelectorAll(".tf-oo-menu [role=menuitem]")].map((el) => el.textContent?.replace(/\s+/g, " ").trim());
    // The generated file shown's command first, then the folder's on the kept folder, then the file command on the
    // generated files; no command with a git name.
    expect(rows[0]).toBe("Markdown lintmdlint /s/knobas-rendered/x/orders.site.A/README.md");
    expect(rows[1]).toBe("Treetree /s/knobas-rendered/x/orders.site.A");
    expect(rows).toContain("Markdown lint on 3 generated filesOne run a file, four at a time");
    expect(rows.join(" ")).not.toContain("Blame");
    expect(rows.join(" ")).not.toContain("main.flow");
  } finally {
    oneoffs.commands = had;
  }
});

test("Alt+click on a grafted generated folder expands and collapses its entire generated subtree", async () => {
  draw({ rendered: RENDERED });
  await settle();
  const dirBtn = (path: string) => target.querySelector<HTMLButtonElement>(`.tf-ftree button.tf-fdir[data-path="${path}"]`)!;
  // Start by collapsing the grafted folder so Alt+click expands every nested generated folder inside it.
  dirBtn("flows/orders").click();
  flushSync();
  expect(dirBtn("flows/orders").getAttribute("aria-expanded")).toBe("false");

  dirBtn("flows/orders").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect(dirBtn("flows/orders").getAttribute("aria-expanded")).toBe("true");
  expect(dirBtn("flows/orders/docs").getAttribute("aria-expanded")).toBe("true");
  expect(target.querySelector('.tf-ftree a[data-path="flows/orders/docs/steps.md"]')).not.toBeNull();
  expect(dirBtn("flows/orders/img").getAttribute("aria-expanded")).toBe("true");

  // Alt+click again collapses the grafted folder and every nested generated folder beneath it.
  dirBtn("flows/orders").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect(dirBtn("flows/orders").getAttribute("aria-expanded")).toBe("false");
  dirBtn("flows/orders").click();
  flushSync();
  expect(dirBtn("flows/orders/docs").getAttribute("aria-expanded")).toBe("false");
});
