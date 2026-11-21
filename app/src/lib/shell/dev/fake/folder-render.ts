/**
 * `?fake-ipc`: folder renderers (ADR-0044). Answers `folder_renderers`, `set_folder_renderers`,
 * `folder_render`, `folder_render_stop`, `folder_render_leave`, `folder_render_output`,
 * `folder_render_frame` and `folder_render_marks`; for generated folders (ADR-0046) `folder_render_tree`,
 * `folder_render_file`, `folder_render_file_bytes`, `folder_render_store`, `set_folder_render_store` and
 * `clear_folder_render_store`.
 *
 * None is saved until one is, so every other page draws as it did; `?fake-folder-render` starts with one,
 * *Folder listing*, which takes every folder. The fake runs no program: a run is queued, copies, runs for
 * 1.5 s and draws the folder's name and the renderer's command as text, each step told on
 * `folder-render:moved` as the backend tells it. A command holding `fail` fails with stderr, one holding
 * `png` draws a 1 × 1 PNG, and one holding `page` draws a long HTML page ({@link fakePage}). `?fake-folder-render=<command
 * words>` gives *Folder listing* that command (`?fake-folder-render=page+hidden` draws a page that hides its overflow).
 *
 * `?fake-rerender` (ADR-0046) adds *Low-code site*, a folder of files renderer taking the folders `docs` and `src/*`.
 * Its generated folder ({@link madeFiles}) holds a README with a relative image, a link to `docs/steps.md` and a Mermaid
 * fence, that page, a PNG, a PDF, an HTML page, an OpenAPI file and `bulk/` with 3,000 files, and leaves out two
 * entries. *Run again* answers a second tree with `CHANGELOG.md` added and `docs/old.md` gone. With `?fake-long-paths`
 * its name is 60 characters and the folder holds a 140-character path.
 *
 * A comparison's sides (ux26) run in lanes of their own (`cmp:<end>`), each a slot of its own. The working tree's lane
 * makes a generated folder that differs from a commit's in three files, adds one and holds one that differs only in
 * line endings; two commits' lanes make the same. `folder_render_compare` compares two generated folders as the backend
 * does. `?fake-rerender-same` makes the working tree's the same too, `?fake-rerender-fail` fails the working tree's
 * lane, and `?fake-rerender-two` adds a second renderer on `docs`, *Docs diagram*, so the folder's pick has a choice.
 */
import type { FolderAsk, FolderDrawn, FolderRenderer, FolderRendererDraft, FolderRenderView, MadeStore, MadeTree } from "../../../ipc/app";
import type { ViewBlock } from "../../../ipc/generated/entity";
import { SPEC_PDF } from "../../../render/pdf-fixture";
import { PETS_30_YAML } from "../../../render/openapi-fixture";
import type { FakeBridge, Handlers } from "../fake-tauri";
import { keepSeed, onFakeReset, seedOf } from "../fake-seed";

let bridge: FakeBridge | null = null;
let saved: FolderRenderer[] = [];
let picks: { repo: string; folder: string; renderer: string }[] = [];
/** Drawings by slot, and the bytes by token. */
const drawn = new Map<string, FolderDrawn>();
const bytes = new Map<string, Uint8Array>();
const failed = new Map<string, { status: number; stderr: string }>();
const running = new Map<string, ReturnType<typeof setTimeout>>();
/** Generated folders by token: each file's bytes, and the tree. */
const made = new Map<string, { files: Map<string, Uint8Array>; tree: MadeTree }>();
/** How many runs each slot of a folder kind has made: the second and later answer the re-run's tree. */
const runsOf = new Map<string, number>();
let tokens = 0;
let storeSet: string | null = null;
/** `?fake-long-paths` with `?fake-rerender`: the long renderer name and path. */
let longMade = false;
/** `?fake-rerender-files=<n>`: how many files `bulk/` holds (3,000 by default), for the timing of a big generated folder. */
let bulkFiles = 3000;
/** `?fake-rerender-same` and `?fake-rerender-fail` (ux26): the working tree's lane makes the same as a commit's, or fails. */
let laneSame = false;
let laneFails = false;

const enc = (text: string) => new TextEncoder().encode(text);

/** The generated folder of run `n` (from 1) of *Low-code site* on `folder`: path to bytes. */
export function madeFiles(folder: string, n: number, long: boolean, working = false): Map<string, Uint8Array> {
  const files = new Map<string, Uint8Array>([
    [
      "README.md",
      enc(
        `# ${folder || "The top folder"}, generated\n\nMade by *Low-code site* from the folder's project.\n\n![The flow](img/flow.png)\n\n` +
          "See [the steps](docs/steps.md), [the report](report.pdf) and [the repository's README](../README.md).\n\n" +
          "```mermaid\ngraph LR\n  Order --> Payment --> Shipped\n```\n",
      ),
    ],
    ["docs/steps.md", enc("# Steps\n\n1. Take the order.\n2. Take the payment.\n3. Ship it.\n\nBack to [the start](../README.md).\n")],
    ["img/flow.png", PNG],
    ["report.pdf", SPEC_PDF()],
    ["page.html", enc(fakePage(folder, "page"))],
    ["openapi.yaml", enc(PETS_30_YAML)],
  ]);
  if (n === 1) files.set("docs/old.md", enc("# Old\n\nThis page is gone after a re-run.\n"));
  else files.set("CHANGELOG.md", enc(`# Changes\n\nRun ${n}.\n`));
  for (let i = 1; i <= bulkFiles; i++) files.set(`bulk/part-${String(i).padStart(4, "0")}.txt`, enc(`part ${i} of the bulk\n`));
  if (long) files.set(`docs/${"a-very-long-generated-folder-name-for-the-long-paths-check/".repeat(2)}and-a-long-file-name-at-the-end.md`.slice(0, 140), enc("# Long\n"));
  if (working) {
    // The working tree's generated folder (ux26): three files differ, one is new, one differs only in line endings.
    files.set("docs/steps.md", enc("# Steps\n\n1. Take the order.\n2. Take the payment.\n3. Retry a failed payment three times.\n4. Ship it.\n\nBack to [the start](../README.md).\n"));
    files.set("bulk/part-0001.txt", enc("part 1 of the bulk, regenerated\n"));
    files.set("img/flow.png", Uint8Array.from([...PNG.slice(0, -1), 0x83]));
    files.set("docs/alert.md", enc("# Alert\n\nThe payments channel hears when the last retry fails.\n"));
    files.set("bulk/part-0002.txt", enc("part 2 of the bulk\r\n"));
    // `?fake-long-paths`: the 140-character generated path differs too, so a comparison lists it.
    if (long) files.set(`docs/${"a-very-long-generated-folder-name-for-the-long-paths-check/".repeat(2)}and-a-long-file-name-at-the-end.md`.slice(0, 140), enc("# Long, regenerated\n"));
  }
  return files;
}

function madeTree(token: string, files: Map<string, Uint8Array>): MadeTree {
  const sorted = [...files.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  return {
    dir: `${clonesRoot()}${longMade ? "\\knobas-rendered\\0123456789ab\\" : "/knobas-rendered/0123456789ab/"}folder.site.${token.slice(-8)}`,
    files: sorted.map(([path, b]) => ({ path, size: b.length })),
    bytes: sorted.reduce((sum, [, b]) => sum + b.length, 0),
    left_out: [
      { path: ".git", why: "git" },
      { path: "latest -> ../site", why: "link" },
    ],
    left_out_total: 2,
  };
}

function gone(): never {
  throw { code: "not_found", message: "that generated folder is no longer kept: run the renderer again" };
}

/** The clones root the fixture's store sits beside: a long Windows one under `?fake-long-paths`. */
const clonesRoot = () => (longMade ? "C:\\Users\\someone with a long name\\source\\repos\\clones for the payments team" : "/Users/someone/code");

function storeView(): MadeStore {
  const folders = made.size;
  const bytesKept = [...made.values()].reduce((sum, m) => sum + m.tree.bytes, 0);
  return {
    dir: `${storeSet ?? clonesRoot()}${longMade ? "\\" : "/"}knobas-rendered`,
    set: storeSet,
    default_dir: clonesRoot(),
    place: storeSet === null ? "clones" : "set",
    folders,
    bytes: bytesKept,
    cap_bytes: 4 * 1024 * 1024 * 1024,
  };
}

const PNG = Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII="), (c) => c.charCodeAt(0));

/**
 * The HTML a command holding `page` draws: 200 rows down the folder, a heading over them, and by the command's other
 * words a page as generators write them: `wide` 3,000 px wide, `hidden` with `html, body { overflow: hidden }`, `vh`
 * with a body of `height: 100vh` that clips, `fixed` with a header fixed at the top, `inner` scrolling a box of its own.
 */
export function fakePage(folder: string, command: string): string {
  const has = (word: string) => command.split(/\s+/).includes(word);
  const css = [
    "body { font: 14px/1.5 system-ui, sans-serif; margin: 0; }",
    "h1 { margin: 0; padding: 12px 16px; background: #1f6feb; color: #fff; font-size: 18px; }",
    "li { padding: 2px 0; }",
    has("wide") ? ".tf-wide { width: 3000px; height: 24px; background: linear-gradient(90deg, #fde68a, #93c5fd); }" : "",
    has("hidden") ? "html, body { overflow: hidden; }" : "",
    has("vh") ? "html, body { height: 100vh; overflow: hidden; }" : "",
    has("fixed") ? "h1 { position: fixed; top: 0; left: 0; right: 0; } ol { margin-top: 56px; }" : "",
    has("inner") ? "body { height: 100vh; display: flex; flex-direction: column; } main { flex: 1; overflow: auto; }" : "",
  ].join("\n");
  const rows = Array.from({ length: 200 }, (_, i) => `<li>${folder || "(the top folder)"} row ${i + 1}</li>`).join("");
  return `<style>${css}</style><h1>${folder || "The top folder"}</h1><main>${has("wide") ? '<div class="tf-wide"></div>' : ""}<ol id="rows">${rows}</ol><p id="end">The end.</p></main>`;
}

/** A slot by its lane too (ux26): a comparison's sides never share the Files tab's. */
const slotOf = (at: Pick<FolderAsk, "entityId" | "renderer" | "folder" | "lane">) => `${at.entityId}\u0000${at.renderer}\u0000${at.folder}\u0000${at.lane ?? ""}`;

function tell(at: Pick<FolderAsk, "entityId" | "renderer" | "folder" | "lane">, state: FolderRenderView["running"]) {
  bridge?.emit("folder-render:moved", { entity_id: at.entityId, renderer: at.renderer, folder: at.folder, running: state, lane: at.lane ?? "" });
}

function settle(drafts: FolderRendererDraft[]): FolderRenderer[] {
  return drafts.map((d, i) => {
    const name = d.name.trim();
    if (name === "") throw { code: "invalid", message: `folder renderer ${i + 1} has no name` };
    const command = d.command.trim();
    const other = [...command.matchAll(/\{([A-Za-z0-9_]+)\}/g)].map((m) => m[0]).find((p) => p !== "{path}" && p !== "{out}" && p !== "{outdir}");
    if (other) throw { code: "invalid", message: `${name}: ${other} is not something a folder renderer is given` };
    if (!command.includes("{path}")) throw { code: "invalid", message: `${name}: the command needs {path} where the folder goes` };
    const folderKind = command.includes("{outdir}");
    if (folderKind && command.includes("{out}")) throw { code: "invalid", message: `${name}: {out} is one file and {outdir} is a folder of files: use one of them` };
    if (!folderKind && d.output === "folder") throw { code: "invalid", message: `${name}: a folder of files needs {outdir} where the tool takes its output folder` };
    return {
      id: d.id ?? (name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || `folder-${i + 1}`),
      name,
      command,
      holds: d.holds.map((g) => g.trim()).filter(Boolean),
      folders: d.folders.map((g) => g.trim().replace(/^\/+|\/+$/g, "")).filter(Boolean),
      output: folderKind ? "folder" : d.output,
      timeout_secs: Math.min(600, Math.max(1, d.timeout_secs ?? 30)),
    };
  });
}

/** The list the page's query started with. */
let seeded: FolderRenderer[] = [];

keepSeed(drawn, bytes, failed, running, made, runsOf);
const atLoad = seedOf({ bridge, saved, seeded, tokens, storeSet, longMade, bulkFiles, laneSame, laneFails });
onFakeReset(() => {
  ({ bridge, saved, seeded, tokens, storeSet, longMade, bulkFiles, laneSame, laneFails } = atLoad());
  picks = [];
});

export function seed(params: URLSearchParams): void {
  const words = params.get("fake-folder-render")?.trim() || "tree";
  seeded = params.has("fake-folder-render")
    ? [{ id: "folder-listing", name: "Folder listing", command: `${words} {path}`, holds: [], folders: [], output: words.split(/\s+/).includes("page") ? "html" : "text", timeout_secs: 30 }]
    : [];
  // `?fake-long-paths` (the render-as pass, 2026-11-19): a folder renderer with a long name and a long Windows program,
  // which draws, and one on a Mac, which fails with its program on stderr.
  if (params.has("fake-long-paths")) seeded = [...seeded, ...LONG_FOLDER_RENDERERS];
  longMade = params.has("fake-long-paths");
  bulkFiles = Number(params.get("fake-rerender-files") ?? 3000) || 3000;
  if (params.has("fake-rerender")) seeded = [...seeded, { ...SITE, name: longMade ? "Low-code site generator for the payments team's order flows" : SITE.name }];
  if (params.has("fake-rerender-two")) seeded = [...seeded, DIAGRAM];
  laneSame = params.has("fake-rerender-same");
  laneFails = params.has("fake-rerender-fail");
  // `?fake-rerender-kept=<folder>`: that folder of payout-service already made, as a store holds it across a restart.
  const kept = params.get("fake-rerender-kept");
  if (kept !== null) {
    const token = `fake-made-${String(++tokens).padStart(8, "0")}`;
    const files = madeFiles(kept, 1, longMade);
    const tree = madeTree(token, files);
    made.set(token, { files, tree });
    runsOf.set(`mock:tidewater/payout-service\u0000site\u0000${kept}\u0000`, 1);
    drawn.set(`mock:tidewater/payout-service\u0000site\u0000${kept}\u0000`, {
      token,
      kind: { kind: "folder", files: tree.files.length, bytes: tree.bytes, left_out: tree.left_out_total },
      size: tree.bytes,
      commit: null,
      from: "clone",
      took_ms: 3200,
      drawn_at: new Date().toISOString(),
    });
  }
  saved = seeded.map((r) => ({ ...r }));
}

/** `?fake-rerender`'s folder of files renderer. */
const SITE: FolderRenderer = {
  id: "site",
  name: "Low-code site",
  command: "lowcode site {path} --out {outdir}",
  holds: [],
  folders: ["docs", "src/*"],
  output: "folder",
  timeout_secs: 60,
};

/** `?fake-rerender-two`'s second renderer on `docs` (ux26): a folder two renderers take, for the pick. */
const DIAGRAM: FolderRenderer = {
  id: "docs-diagram",
  name: "Docs diagram",
  command: "/Users/someone/tools/flowviz/bin/flowviz {path} --out {outdir}",
  holds: [],
  folders: ["docs"],
  output: "folder",
  timeout_secs: 30,
};

const LONG_FOLDER_RENDERERS: FolderRenderer[] = [
  {
    id: "orchestra-map",
    name: "Orchestra folder map for the payments team's services",
    command: String.raw`"C:\Program Files\Some Vendor\Orchestra Renderer 2026\bin\orchestra-map.exe" --out {out} "{path}"`,
    holds: [],
    folders: [],
    output: "text",
    timeout_secs: 30,
  },
  {
    id: "orchestra-map-mac",
    name: "Orchestra map on the Mac",
    command: `"/Users/someone with a long name/Library/Application Support/Orchestra Renderer/2026/bin/orchestra-map" --fail-on-empty "{path}"`,
    holds: [],
    folders: [],
    output: "text",
    timeout_secs: 30,
  },  // Long lists of what it takes, and a long command that makes a folder (the renderers-columns pass, 2026-11-20): Settings ›
  // Renderers holds its row to one line. No folder of the fixture holds or is named one of these, so it draws none.
  {
    id: "orchestra-flows",
    name: "Orchestra flows site for the payments team's order services",
    command: String.raw`"C:\Program Files\Some Vendor\Orchestra Renderer 2026\bin\orchestra-site.exe" build --profile "C:\Users\someone with a long name\AppData\Roaming\Orchestra Renderer\profiles\payments team default.json" --source "{path}" --out {outdir} --theme dark`,
    holds: ["*.orcproj", "*.orchestra.json", "orchestra.config.yaml", "flows.manifest", "*.flowset", "score-index.json", "*.orcpkg", "build.orchestra", "*.orclock", "orchestra.lock"],
    folders: ["flows", "apps/*/flows", "services/payments/order-flows", "docs/architecture/sequence diagrams", "orchestra"],
    output: "folder",
    timeout_secs: 120,
  },
];

/** Two generated folders compared as `folder_render::made::compare` does: only what differs, the rest counted. */
function compareMade(before: string | null, after: string | null, eolAside: boolean) {
  const a = before === null ? new Map<string, Uint8Array>() : (made.get(before) ?? gone()).files;
  const b = after === null ? new Map<string, Uint8Array>() : (made.get(after) ?? gone()).files;
  const binary = (x: Uint8Array | undefined) => x !== undefined && x.slice(0, 8000).includes(0);
  const same = (x: Uint8Array, y: Uint8Array) => x.length === y.length && x.every((v, i) => v === y[i]);
  const crlf = (x: Uint8Array) => new TextDecoder().decode(x).replace(/\r\n/g, "\n");
  const files: { path: string; change: "added" | "deleted" | "modified"; before_size: number | null; after_size: number | null; binary: boolean }[] = [];
  let sameCount = 0;
  let eol = 0;
  for (const path of [...new Set([...a.keys(), ...b.keys()])].sort()) {
    const x = a.get(path);
    const y = b.get(path);
    const bin = binary(x) || binary(y) || /\.(png|pdf)$/.test(path);
    if (x !== undefined && y !== undefined && same(x, y)) sameCount += 1;
    else if (x !== undefined && y !== undefined && eolAside && !bin && crlf(x) === crlf(y)) eol += 1;
    else files.push({ path, change: x === undefined ? "added" : y === undefined ? "deleted" : "modified", before_size: x?.length ?? null, after_size: y?.length ?? null, binary: bin });
  }
  return { files, same: sameCount, eol_only: eol, before_files: a.size, after_files: b.size };
}

export function attach(installed: FakeBridge): void {
  bridge = installed;
}

export function handlers(): Handlers {
  return {
    folder_renderers: () => saved,
    // The pick per folder (ux26): kept for the session, the repository named as the backend resolves it (a branch's id
    // names its repository).
    // Two generated folders compared (ux26): nothing differs in the fixture's own runs.
    folder_render_compare: (args) => compareMade((args["before"] as string | null) ?? null, (args["after"] as string | null) ?? null, args["eolAside"] === true),
    folder_render_picks: () => picks,
    set_folder_render_pick: (args) => {
      const id = String(args["entityId"] ?? "");
      const repo = id.includes("@") ? id.slice(0, id.indexOf("@")) : id;
      const folder = String(args["folder"] ?? "").replace(/^\/+|\/+$/g, "");
      const renderer = (args["renderer"] as string | null | undefined) ?? null;
      picks = [...picks.filter((p) => !(p.repo === repo && p.folder === folder)), ...(renderer === null ? [] : [{ repo, folder, renderer }])];
      return picks;
    },
    set_folder_renderers: (args) => {
      saved = settle(args["renderers"] as FolderRendererDraft[]);
      return saved;
    },
    folder_render: (args) => {
      const at = args as unknown as FolderAsk;
      const renderer = saved.find((r) => r.id === at.renderer);
      if (!renderer) throw { code: "invalid", message: `'${at.renderer}' is not one of your folder renderers: see Settings › Renderers` };
      const slot = slotOf(at);
      // Where the copy is read from, as the backend says it: the working tree's at `~working-tree` (ux29).
      const from = at.commit === "~working-tree" ? "working_tree" : "clone";
      const view: FolderRenderView = { renderer: at.renderer, folder: at.folder, drawn: drawn.get(slot) ?? null, current: drawn.has(slot), running: null, failure: null, from, commit: at.commit };
      if (running.has(slot)) return { ...view, running: { state: "running", elapsed_ms: 0 } };
      const fault = failed.get(slot);
      const starts = at.ask === "again" || (at.ask === "open" && !drawn.has(slot) && !fault);
      if (!starts) return fault ? { ...view, failure: { why: "failed", status: fault.status, stderr: fault.stderr } } : view;
      failed.delete(slot);
      setTimeout(() => tell(at, { state: "copying", done: 0, total: 3 }), 20);
      setTimeout(() => tell(at, { state: "running", elapsed_ms: 0 }), 60);
      if (renderer.output === "folder") setTimeout(() => tell(at, { state: "keeping", done: 1200, total: 3008 }), 1200);
      running.set(
        slot,
        setTimeout(() => {
          running.delete(slot);
          if (renderer.command.includes("fail") || (laneFails && at.lane === "cmp:~working-tree")) {
            failed.set(slot, { status: 2, stderr: `${/^"([^"]*)"|^(\S+)/.exec(renderer.command)?.slice(1).find((w) => w !== undefined) ?? ""}: no project file in ${at.folder || "the top folder"}\n` });
          } else if (renderer.output === "folder") {
            const token = `fake-made-${String(++tokens).padStart(8, "0")}`;
            const n = (runsOf.get(slot) ?? 0) + 1;
            runsOf.set(slot, n);
            // A comparison's side makes its end's folder: the working tree's differs, every commit's is the first run's.
            const lane = at.lane ?? "";
            const files = lane === "" ? madeFiles(at.folder, n, longMade) : madeFiles(at.folder, 1, longMade, lane === "cmp:~working-tree" && !laneSame);
            const tree = madeTree(token, files);
            made.set(token, { files, tree });
            drawn.set(slot, {
              token,
              kind: { kind: "folder", files: tree.files.length, bytes: tree.bytes, left_out: tree.left_out_total },
              size: tree.bytes,
              commit: at.commit,
              from,
              took_ms: 3200,
              drawn_at: new Date().toISOString(),
            });
          } else {
            const token = `fake-${++tokens}`;
            const png = renderer.command.includes("png");
            const page = renderer.command.split(/\s+/).includes("page");
            const made = png
              ? PNG
              : new TextEncoder().encode(page ? fakePage(at.folder, renderer.command) : `${at.folder || "(the top folder)"}\n  drawn by ${renderer.name}: ${renderer.command}\n`);
            bytes.set(token, made);
            drawn.set(slot, {
              token,
              kind: png ? { kind: "image", mime: "image/png" } : page ? { kind: "html" } : { kind: "text" },
              size: made.length,
              commit: at.commit,
              from,
              took_ms: 1500,
              drawn_at: new Date().toISOString(),
            });
          }
          tell(at, null);
        }, 1500),
      );
      return { ...view, running: { state: "queued" } };
    },
    folder_render_stop: (args) => {
      const slot = slotOf(args as unknown as FolderAsk);
      const timer = running.get(slot);
      if (timer !== undefined) clearTimeout(timer);
      running.delete(slot);
      tell(args as unknown as FolderAsk, null);
    },
    folder_render_leave: () => undefined,
    folder_render_output: (args) => {
      const found = bytes.get(String(args["token"]));
      if (!found) throw { code: "not_found", message: "that drawing is no longer kept: run the renderer again" };
      return found;
    },
    folder_render_frame: () => `fake-frame-${++tokens}`,
    folder_render_marks: () => saved.filter((r) => r.holds.length > 0).map((r) => ({ renderer: r.id, folders: [] })),
    folder_render_tree: (args) => made.get(String(args["token"]))?.tree ?? gone(),
    folder_render_file: (args): ViewBlock => {
      const kept = made.get(String(args["token"])) ?? gone();
      const path = String(args["path"]);
      const b = kept.files.get(path);
      if (!b) throw { code: "not_found", message: `the generated folder holds no file ${path}` };
      const binary = b.slice(0, 8000).includes(0) || /\.(png|pdf)$/.test(path);
      const ext = path.slice(path.lastIndexOf(".") + 1);
      const language = ({ md: "markdown", html: "html", yaml: "yaml", txt: null } as Record<string, string | null>)[ext] ?? null;
      return { type: "file", trail: [], path, text: binary ? "" : new TextDecoder().decode(b), language, size: b.length, truncated: false, binary, url: null, sha: null, commit: null };
    },
    folder_render_file_bytes: (args) => {
      const kept = made.get(String(args["token"])) ?? gone();
      const b = kept.files.get(String(args["path"]));
      if (!b) throw { code: "not_found", message: `the generated folder holds no file ${String(args["path"])}` };
      return b;
    },
    folder_render_open: (args) => {
      const kept = made.get(String(args["token"])) ?? gone();
      const file = (args["file"] as string | null) ?? null;
      if (file !== null && !kept.files.has(file)) throw { code: "invalid", message: `the generated folder holds no file ${file}` };
      return ["code", file === null ? kept.tree.dir : `${kept.tree.dir}/${file}`];
    },
    folder_render_store: () => storeView(),
    set_folder_render_store: (args) => {
      const path = (args["path"] as string | null)?.trim() || null;
      if (path !== null && /\/(code|src)\/[^/]+\/?/.test(path) && !path.endsWith("/code")) {
        throw { code: "invalid", message: `${path} is inside the repository at ${path.replace(/(\/(code|src)\/[^/]+).*/, "$1")}: generated files there would show as untracked changes. Pick a folder outside every checkout.` };
      }
      if (path !== storeSet) {
        made.clear();
        drawn.clear();
      }
      storeSet = path;
      return storeView();
    },
    clear_folder_render_store: () => {
      made.clear();
      for (const [slot, d] of drawn) if (d.kind.kind === "folder") drawn.delete(slot);
      return storeView();
    },
  };
}
