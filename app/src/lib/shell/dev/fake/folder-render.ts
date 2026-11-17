/**
 * `?fake-ipc`: folder renderers (ADR-0044). Answers `folder_renderers`, `set_folder_renderers`,
 * `folder_render`, `folder_render_stop`, `folder_render_leave`, `folder_render_output`,
 * `folder_render_frame` and `folder_render_marks`.
 *
 * None is saved until one is, so every other page draws as it did; `?fake-folder-render` starts with one,
 * *Folder listing*, which takes every folder. The fake runs no program: a run is queued, copies, runs for
 * 1.5 s and draws the folder's name and the renderer's command as text, each step told on
 * `folder-render:moved` as the backend tells it. A command holding `fail` fails with stderr, one holding
 * `png` draws a 1 × 1 PNG, and one holding `page` draws a long HTML page ({@link fakePage}). `?fake-folder-render=<command
 * words>` gives *Folder listing* that command (`?fake-folder-render=page+hidden` draws a page that hides its overflow).
 */
import type { FolderAsk, FolderDrawn, FolderRenderer, FolderRendererDraft, FolderRenderView } from "../../../ipc/app";
import type { FakeBridge, Handlers } from "../fake-tauri";
import { keepSeed, onFakeReset, seedOf } from "../fake-seed";

let bridge: FakeBridge | null = null;
let saved: FolderRenderer[] = [];
/** Drawings by slot, and the bytes by token. */
const drawn = new Map<string, FolderDrawn>();
const bytes = new Map<string, Uint8Array>();
const failed = new Map<string, { status: number; stderr: string }>();
const running = new Map<string, ReturnType<typeof setTimeout>>();
let tokens = 0;

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

const slotOf = (at: Pick<FolderAsk, "entityId" | "renderer" | "folder">) => `${at.entityId}\u0000${at.renderer}\u0000${at.folder}`;

function tell(at: Pick<FolderAsk, "entityId" | "renderer" | "folder">, state: FolderRenderView["running"]) {
  bridge?.emit("folder-render:moved", { entity_id: at.entityId, renderer: at.renderer, folder: at.folder, running: state });
}

function settle(drafts: FolderRendererDraft[]): FolderRenderer[] {
  return drafts.map((d, i) => {
    const name = d.name.trim();
    if (name === "") throw { code: "invalid", message: `folder renderer ${i + 1} has no name` };
    const command = d.command.trim();
    const other = [...command.matchAll(/\{([A-Za-z0-9_]+)\}/g)].map((m) => m[0]).find((p) => p !== "{path}" && p !== "{out}");
    if (other) throw { code: "invalid", message: `${name}: ${other} is not something a folder renderer is given` };
    if (!command.includes("{path}")) throw { code: "invalid", message: `${name}: the command needs {path} where the folder goes` };
    return {
      id: d.id ?? (name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || `folder-${i + 1}`),
      name,
      command,
      holds: d.holds.map((g) => g.trim()).filter(Boolean),
      folders: d.folders.map((g) => g.trim().replace(/^\/+|\/+$/g, "")).filter(Boolean),
      output: d.output,
      timeout_secs: Math.min(600, Math.max(1, d.timeout_secs ?? 30)),
    };
  });
}

/** The list the page's query started with. */
let seeded: FolderRenderer[] = [];

keepSeed(drawn, bytes, failed, running);
const atLoad = seedOf({ bridge, saved, seeded, tokens });
onFakeReset(() => {
  ({ bridge, saved, seeded, tokens } = atLoad());
});

export function seed(params: URLSearchParams): void {
  const words = params.get("fake-folder-render")?.trim() || "tree";
  seeded = params.has("fake-folder-render")
    ? [{ id: "folder-listing", name: "Folder listing", command: `${words} {path}`, holds: [], folders: [], output: words.split(/\s+/).includes("page") ? "html" : "text", timeout_secs: 30 }]
    : [];
  saved = seeded.map((r) => ({ ...r }));
}

export function attach(installed: FakeBridge): void {
  bridge = installed;
}

export function handlers(): Handlers {
  return {
    folder_renderers: () => saved,
    set_folder_renderers: (args) => {
      saved = settle(args["renderers"] as FolderRendererDraft[]);
      return saved;
    },
    folder_render: (args) => {
      const at = args as unknown as FolderAsk;
      const renderer = saved.find((r) => r.id === at.renderer);
      if (!renderer) throw { code: "invalid", message: `'${at.renderer}' is not one of your folder renderers: see Settings › Renderers` };
      const slot = slotOf(at);
      const view: FolderRenderView = { renderer: at.renderer, folder: at.folder, drawn: drawn.get(slot) ?? null, current: drawn.has(slot), running: null, failure: null, from: "clone", commit: at.commit };
      if (running.has(slot)) return { ...view, running: { state: "running", elapsed_ms: 0 } };
      const fault = failed.get(slot);
      const starts = at.ask === "again" || (at.ask === "open" && !drawn.has(slot) && !fault);
      if (!starts) return fault ? { ...view, failure: { why: "failed", status: fault.status, stderr: fault.stderr } } : view;
      failed.delete(slot);
      setTimeout(() => tell(at, { state: "copying", done: 0, total: 3 }), 20);
      setTimeout(() => tell(at, { state: "running", elapsed_ms: 0 }), 60);
      running.set(
        slot,
        setTimeout(() => {
          running.delete(slot);
          if (renderer.command.includes("fail")) {
            failed.set(slot, { status: 2, stderr: `${renderer.command.split(" ")[0]}: no project file in ${at.folder || "the top folder"}\n` });
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
              from: "clone",
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
  };
}
