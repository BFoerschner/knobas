/**
 * A folder drawn by a folder renderer (ADR-0044): what it asks and when, each
 * kind of drawing, the sandbox an HTML page is drawn in, a failure and its
 * stderr, *Run again*, *Stop*, the run's events and the leave.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { FolderAsk, FolderRenderer, FolderRenderView } from "../../ipc/app";
import type { FrameBridge } from "../../render/frame";
import TfFolderRender, { type FolderRenderPorts } from "./TfFolderRender.svelte";

const LISTER: FolderRenderer = { id: "lister", name: "Lister", command: "tree {path}", holds: [], folders: [], output: "detect", timeout_secs: 30 };
const SHA = "a".repeat(40);
const NOW = new Date("2026-11-16T12:00:00Z");

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

const view = (over: Partial<FolderRenderView> = {}): FolderRenderView => ({
  renderer: "lister",
  folder: "flows/app",
  drawn: null,
  current: false,
  running: null,
  failure: null,
  from: "clone",
  commit: SHA,
  ...over,
});

const drawnAs = (kind: NonNullable<FolderRenderView["drawn"]>["kind"], token = "t1") => ({
  token,
  kind,
  size: 12,
  commit: SHA,
  from: "clone" as const,
  took_ms: 1234,
  drawn_at: "2026-11-16T11:00:00Z",
});

/** Ports answering `answers` in turn (the last again), recording what was asked. */
function fake(answers: FolderRenderView[], output: Uint8Array = new TextEncoder().encode("line one\nline two")) {
  const asked: FolderAsk[] = [];
  const stopped: string[] = [];
  const left: string[] = [];
  const frames: string[] = [];
  let heard: ((event: { payload: unknown }) => void) | null = null;
  const ports: FolderRenderPorts = {
    render: (at) => {
      asked.push(at);
      return Promise.resolve(answers[Math.min(asked.length - 1, answers.length - 1)]!);
    },
    stop: (_e, r, f) => (stopped.push(`${r}:${f}`), Promise.resolve()),
    leave: (_e, r, f) => (left.push(`${r}:${f}`), Promise.resolve()),
    output: () => Promise.resolve(output),
    frame: (doc) => (frames.push(doc), Promise.resolve("frame-token")),
    listen: (_event, handler) => {
      heard = handler;
      return Promise.resolve(() => (heard = null));
    },
  };
  return { ports, asked, stopped, left, frames, tell: (payload: unknown) => heard?.({ payload }) };
}

const NO_TAURI: FrameBridge = { isTauri: () => false, convertFileSrc: (p, s) => `${s}://localhost/${p}` };
const TAURI: FrameBridge = { isTauri: () => true, convertFileSrc: (p, s) => `${s}://localhost/${p}` };

async function settle() {
  for (let i = 0; i < 6; i++) await Promise.resolve();
  flushSync();
}

async function draw(ports: FolderRenderPorts, props: Record<string, unknown> = {}) {
  app = mount(TfFolderRender, { target, props: { entityId: "gitea:repo:o/r", renderer: LISTER, folder: "flows/app", commit: SHA, ports, bridge: NO_TAURI, now: () => NOW, ...props } });
  await settle();
}

test("a shown page opens the render; a hidden one only looks", async () => {
  const shown = fake([view({ running: { state: "queued" } })]);
  await draw(shown.ports);
  expect(shown.asked.map((a) => a.ask)).toEqual(["open"]);
  expect(shown.asked[0]).toMatchObject({ entityId: "gitea:repo:o/r", renderer: "lister", folder: "flows/app", commit: SHA, paths: null });
  expect(target.querySelector(".tf-banner.tf-info")?.textContent).toContain("Lister waits for another folder renderer");
  unmount(app!);
  app = null;
  expect(shown.left).toEqual(["lister:flows/app"]);

  const hidden = fake([view()]);
  await draw(hidden.ports, { start: false });
  expect(hidden.asked.map((a) => a.ask)).toEqual(["look"]);
  expect(target.textContent).toContain("Lister has not drawn this folder yet.");
});

test("text is drawn in the code view with its lines, and the header says when and how long", async () => {
  const f = fake([view({ drawn: drawnAs({ kind: "text" }), current: true })]);
  await draw(f.ports);
  const lines = [...target.querySelectorAll(".tf-code .tf-l")].map((l) => l.textContent);
  expect(lines).toEqual(["1line one", "2line two"]);
  const meta = target.querySelector(".tf-fmeta")!.textContent!.replace(/\s+/g, " ");
  expect(meta).toContain("Drawn by Lister at aaaaaaa, 1.2s");
  expect(target.querySelector("header .tf-p")!.textContent).toBe("flows/app");
});

test("an image is drawn fitted, and the zoom steps from there", async () => {
  const f = fake([view({ drawn: drawnAs({ kind: "image", mime: "image/png" }), current: true })], Uint8Array.from([0x89, 0x50, 0x4e, 0x47]));
  await draw(f.ports);
  const img = target.querySelector<HTMLImageElement>(".tf-frender-img img")!;
  expect(img.src).toMatch(/^blob:/);
  expect(target.querySelector(".tf-frender-img")!.classList.contains("tf-fit")).toBe(true);
  const actual = [...target.querySelectorAll<HTMLButtonElement>(".tf-seg button")].find((b) => b.textContent === "Actual size")!;
  actual.click();
  flushSync();
  expect(target.querySelector(".tf-frender-img")!.classList.contains("tf-fit")).toBe(false);
  expect(target.querySelector(".tf-pdf-zoom")!.textContent).toBe("100%");
  target.querySelector<HTMLButtonElement>('[aria-label="Zoom in"]')!.click();
  flushSync();
  expect(target.querySelector(".tf-pdf-zoom")!.textContent).toBe("150%");
});

/**
 * The sandbox: an HTML page that tries to reach the app has its script taken out, and is drawn in a frame that runs
 * none (`sandbox=""`, no `allow-scripts`, no `allow-same-origin`), served in the app from the render scheme by a token
 * under a policy with no script, never from the app's own origin.
 */
test("an HTML page that tries to reach the app is drawn sanitized in a frame that runs no script", async () => {
  const page = `<h1>Flows</h1><script>parent.__TAURI_INTERNALS__.invoke("delete_source", {id: "x"})</script><img src=x onerror="top.location='https://evil.example'"><a href="javascript:alert(1)">go</a>`;
  const f = fake([view({ drawn: drawnAs({ kind: "html" }), current: true })], new TextEncoder().encode(page));
  await draw(f.ports, { bridge: TAURI });
  await settle();
  const frame = target.querySelector("iframe")!;
  expect(frame.getAttribute("sandbox")).toBe("");
  expect(frame.getAttribute("src")).toBe("knobas-render://localhost/folder.frame-token");
  expect(frame.hasAttribute("srcdoc")).toBe(false);
  expect(f.frames).toHaveLength(1);
  const doc = f.frames[0]!;
  expect(doc).toContain("<h1>Flows</h1>");
  expect(doc).not.toMatch(/<script|onerror|javascript:|__TAURI/i);

  // Without the app's bridge (dev, tests) the sanitized page is the frame's own document, still sandboxed.
  unmount(app!);
  const dev = fake([view({ drawn: drawnAs({ kind: "html" }), current: true })], new TextEncoder().encode(page));
  await draw(dev.ports);
  const local = target.querySelector("iframe")!;
  expect(local.getAttribute("sandbox")).toBe("");
  expect(local.getAttribute("srcdoc")).not.toMatch(/<script|onerror/i);
});

test("Markdown is drawn by the Markdown component, its relative links opening files at the folder", async () => {
  const f = fake([view({ drawn: drawnAs({ kind: "markdown" }), current: true })], new TextEncoder().encode("# Orders\n\nSee [the flow](main.flow)."));
  await draw(f.ports, { fileLink: (to: string) => `#/file/${to}` });
  await settle();
  expect(target.querySelector(".tf-rendered h1")?.textContent).toBe("Orders");
  expect(target.querySelector<HTMLAnchorElement>(".tf-rendered a")?.getAttribute("href")).toBe("#/file/flows/app/main.flow");
});

test("a failure says what the command said, and Run again runs it", async () => {
  const f = fake([
    view({ failure: { why: "failed", status: 3, stderr: "no project file\n" } }),
    view({ running: { state: "copying", done: 1, total: 4 } }),
  ]);
  await draw(f.ports);
  const alert = target.querySelector('[role="alert"]')!;
  expect(alert.textContent).toContain("Lister could not draw this folder.");
  expect(alert.textContent).toContain("The command exited with status 3.");
  expect(alert.querySelector("pre")!.textContent).toBe("no project file\n");
  const run = target.querySelector<HTMLButtonElement>(".tf-frender-run")!;
  expect(run.textContent).toBe("Run again");
  run.click();
  flushSync();
  // The press answers in its own frame: the failure goes and the run is said, before the backend answers.
  expect(run.textContent).toBe("Stop");
  expect(target.querySelector('[role="alert"]')).toBeNull();
  await settle();
  expect(f.asked.map((a) => a.ask)).toEqual(["open", "again"]);
  // The same button, now Stop, keeps focus; a second press inside the hold stops nothing.
  expect(target.querySelector(".tf-frender-run")).toBe(run);
  expect(run.textContent).toBe("Stop");
  run.click();
  expect(f.stopped).toEqual([]);
  expect(target.querySelector(".tf-banner.tf-info")!.textContent).toContain("Copying 1 of 4 files from your clone…");
});

test("Stop stops the run; an event moves the state and an ending asks again", async () => {
  const f = fake([view({ running: { state: "running", elapsed_ms: 0 } }), view({ drawn: drawnAs({ kind: "text" }), current: true })]);
  await draw(f.ports);
  const run = target.querySelector<HTMLButtonElement>(".tf-frender-run")!;
  expect(run.textContent).toBe("Stop");
  run.click();
  flushSync();
  // The press answers at once, before the backend does.
  expect(target.querySelector(".tf-banner.tf-info")).toBeNull();
  // A double press is one press (the press rule).
  run.click();
  expect(f.stopped).toEqual(["lister:flows/app"]);

  // Another folder's event is not this one's.
  f.tell({ entity_id: "gitea:repo:o/r", renderer: "lister", folder: "other", running: null });
  await settle();
  expect(f.asked).toHaveLength(1);
  f.tell({ entity_id: "gitea:repo:o/r", renderer: "lister", folder: "flows/app", running: { state: "copying", done: 2, total: 9 } });
  flushSync();
  expect(target.querySelector(".tf-banner.tf-info")!.textContent).toContain("Copying 2 of 9 files");
  expect(f.asked).toHaveLength(1);
  f.tell({ entity_id: "gitea:repo:o/r", renderer: "lister", folder: "flows/app", running: null });
  await settle();
  expect(f.asked.map((a) => a.ask)).toEqual(["open", "look"]);
  expect(target.querySelector(".tf-code")?.textContent).toContain("line one");
});

test("a drawing of an earlier commit is shown under a line saying so, while the folder runs at its new one", async () => {
  const older = { ...drawnAs({ kind: "text" }), commit: "b".repeat(40) };
  const f = fake([view({ drawn: older, current: false, running: { state: "running", elapsed_ms: 0 } })]);
  await draw(f.ports, { changedInside: true });
  const meta = target.querySelector(".tf-fmeta")!.textContent!.replace(/\s+/g, " ");
  expect(meta).toContain("Drawn at bbbbbbb");
  expect(meta).toContain("the folder is at aaaaaaa now");
  expect(target.querySelector(".tf-frender-out")!.classList.contains("tf-frender-held")).toBe(true);
  expect(target.querySelector(".tf-banner.tf-warn")!.textContent).toContain("what your checkout changed in this folder is not in it");
});

test("a folder too big to copy says by how much, and from where", async () => {
  const f = fake([view({ failure: { why: "too_big", files: 1500, bytes: 80 * 1024 * 1024, cap_files: 1000, cap_bytes: 64 * 1024 * 1024, from: "forge" } })]);
  await draw(f.ports);
  expect(target.querySelector('[role="alert"]')!.textContent).toContain("The folder holds 1,500 files and 80 MB; knobas copies up to 1,000 files and 64 MB from the source.");
});

test("the switch offers the other renderers that take the folder", async () => {
  const other: FolderRenderer = { ...LISTER, id: "diagram", name: "Diagram" };
  const f = fake([view()]);
  const asked: string[] = [];
  await draw(f.ports, { renderers: [LISTER, other], switchTo: (id: string) => (asked.push(id), `#/x/${id}`) });
  const select = target.querySelector<HTMLSelectElement>(".tf-frender-as select")!;
  expect([...select.options].map((o) => o.textContent)).toEqual(["Lister", "Diagram"]);
  select.value = "diagram";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  expect(asked).toEqual(["diagram"]);
});
