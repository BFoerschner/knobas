/**
 * A page's embed in the Docs reader (#2440, spec #2432 *Embeds in the
 * shell*), drawn through `TfDocs` as a reader opens it. What the backend
 * serves the frame under is `renderers.rs`'s test; the document and the
 * reporter are `render/embed.test.ts`'s.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DocPage, DocsTree } from "../../ipc/entity";
import { EMBED_SANDBOX, EMBED_URL_REFERRER, EMBED_URL_SANDBOX } from "../../render/embed";
import { EMBED_REPORT_MS, EMBED_URL_HEIGHT } from "./docs";
import TfDocs, { type DocsPorts } from "./TfDocs.svelte";

const TREE: DocsTree = {
  spaces: [{ source_id: "confluence", source_name: "confluence", key: "ENG", name: "Engineering", pages: [{ entity_id: "confluence:5", title: "Payments API", parent: null }] }],
  last_read: "confluence:5",
  for_you: [],
};

const HTML = `<div id="swagger-ui"></div><script src="https://api.example/swagger-ui-bundle.js"></script><script>SwaggerUIBundle({ url: "https://api.example/openapi.json", dom_id: "#swagger-ui" })</script>`;

function page(over: Partial<DocPage> = {}): DocPage {
  return {
    entity_id: "confluence:5",
    source_id: "confluence",
    source_name: "Confluence",
    title: "Payments API",
    space: { key: "ENG", name: "Engineering" },
    ancestors: [],
    editor: null,
    editor_name: null,
    updated_at: null,
    web_url: "http://wiki.example/pages/5",
    readable: true,
    version: 3,
    labels: [],
    blocks: [
      { type: "paragraph", text: [{ type: "text", text: "Try it below.", strong: false, emphasis: false, code: false }] },
      { type: "embed", name: "html", content: { kind: "html", html: HTML } },
    ],
    comments: [],
    sent_comments: [],
    me: [],
    sections: [],
    history: [],
    backlinks: [],
    work: [],
    writes: [],
    run_embeds: true,
    ...over,
  };
}

let target: HTMLDivElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

function ports(doc: DocPage): DocsPorts {
  return {
    docsTree: vi.fn(async () => TREE),
    docsPage: vi.fn(async () => doc),
    docsSearch: vi.fn(async () => null),
    docsExportMarkdown: vi.fn(async () => ({ file: "x.md", folder: "/d" })),
    docsEditSection: vi.fn(),
    docsCreatePage: vi.fn(),
    submitWrite: vi.fn(),
    visitEntity: vi.fn(async () => null),
    jobReach: vi.fn(async () => []),
    getEntity: vi.fn(async (id: string) => Promise.reject({ code: "not_found", message: `${id} is not in the mirror`, source_id: null })),
    openExternal: vi.fn(async () => {}),
    copy: vi.fn(async () => {}),
    searchPause: 0,
  } as unknown as DocsPorts;
}

async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await tick();
    await Promise.resolve();
  }
  flushSync();
}

async function draw(doc = page()): Promise<DocsPorts> {
  const io = ports(doc);
  view = mount(TfDocs, { target, props: { ready: true, rest: ["confluence:5"], job: null, ports: io } });
  await settle();
  return io;
}

const frame = () => target.querySelector<HTMLIFrameElement>(".tf-doc iframe");
const button = (name: string) => [...target.querySelectorAll("button")].find((b) => b.textContent === name);

/** The frame saying something, as its reporter does: a message whose source is the frame's window. */
function say(data: unknown, source: Window | null = frame()!.contentWindow): void {
  window.dispatchEvent(new MessageEvent("message", { data, source }));
  flushSync();
}

test("an embed runs in a sandboxed frame with no same origin and no top navigation, its HTML as written", async () => {
  await draw();
  const f = frame()!;
  expect(f.getAttribute("sandbox")).toBe(EMBED_SANDBOX);
  expect(f.getAttribute("sandbox")).not.toMatch(/allow-same-origin|allow-top-navigation/);
  expect(f.getAttribute("referrerpolicy")).toBe("no-referrer");
  expect(f.getAttribute("title")).toBe("Embedded HTML from Confluence");
  // No bridge in a test: the document is a srcdoc, the author's HTML untouched in it.
  expect(f.getAttribute("srcdoc")).toContain(HTML);
  expect(target.querySelector(".tf-doc .tf-macro")).toBeNull();
});

test("the frame's height follows what its reporter says, up to the viewport", async () => {
  await draw();
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(700);
  // The report is the document's height; the frame's 1px border sits outside
  // it (#2459), so a document that fits draws no scrollbar.
  say({ knobas: "embed", height: 312 });
  expect(frame()!.getAttribute("height")).toBe("314");
  say({ knobas: "embed", height: 4000 });
  expect(frame()!.getAttribute("height")).toBe("700");
  // Another window saying the same is not the frame.
  say({ knobas: "embed", height: 50 }, window);
  expect(frame()!.getAttribute("height")).toBe("700");
});

/** The window resized to `height`, as the browser reports it. */
function resize(height: number): void {
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(height);
  window.dispatchEvent(new Event("resize"));
  flushSync();
}

test("the frame's height follows the window as it resizes, back up to what its reporter said", async () => {
  await draw();
  resize(700);
  say({ knobas: "embed", height: 600 });
  expect(frame()!.getAttribute("height")).toBe("602");
  resize(400);
  expect(frame()!.getAttribute("height")).toBe("400");
  resize(1000);
  expect(frame()!.getAttribute("height")).toBe("602");
});

/** The reader's transient activation, as the browser reports it to the app's window (#2454). */
function clicked(active: boolean): void {
  Object.defineProperty(navigator, "userActivation", { configurable: true, get: () => ({ isActive: active, hasBeenActive: active }) });
}
afterEach(() => {
  delete (navigator as { userActivation?: unknown }).userActivation;
});

test("a link or popup from the frame, behind the reader's click, opens through the external-open path", async () => {
  const io = await draw();
  clicked(true);
  say({ knobas: "embed", open: "https://api.example/docs" });
  await settle();
  expect(io.openExternal).toHaveBeenCalledWith("https://api.example/docs");
  // Only from the frame itself.
  say({ knobas: "embed", open: "https://evil.example/" }, window);
  await settle();
  expect(io.openExternal).toHaveBeenCalledTimes(1);
});

test("an open with no click behind it opens nothing until the reader says so, and a loop asks once", async () => {
  const io = await draw();
  clicked(false);
  for (let i = 0; i < 20; i++) say({ knobas: "embed", open: `https://spam.example/${i}` });
  await settle();
  expect(io.openExternal).not.toHaveBeenCalled();
  const asked = target.querySelector(".tf-doc .tf-embed .tf-macro");
  expect(asked?.textContent).toContain("wants to open https://spam.example/0");
  expect(frame()).not.toBeNull();
  button("Open")!.click();
  await settle();
  expect(io.openExternal).toHaveBeenCalledTimes(1);
  expect(io.openExternal).toHaveBeenCalledWith("https://spam.example/0");
  expect(target.querySelector(".tf-doc .tf-embed .tf-macro")).toBeNull();
  // Dismissed, it opens nothing.
  say({ knobas: "embed", open: "https://spam.example/again" });
  button("Dismiss")!.click();
  await settle();
  expect(io.openExternal).toHaveBeenCalledTimes(1);
  expect(target.querySelector(".tf-doc .tf-embed .tf-macro")).toBeNull();
});

test("one click opens one tab: a second open on it is asked", async () => {
  const io = await draw();
  clicked(true);
  say({ knobas: "embed", open: "https://api.example/a" });
  say({ knobas: "embed", open: "https://api.example/b" });
  await settle();
  expect(io.openExternal).toHaveBeenCalledTimes(1);
  expect(target.querySelector(".tf-doc .tf-embed .tf-macro")?.textContent).toContain("wants to open https://api.example/b");
});

test("one click opens one tab across the page: a second embed's open on it is asked", async () => {
  const two = page();
  two.blocks = [...two.blocks, { type: "embed", name: "html", content: { kind: "html", html: "<p>second</p>" } }];
  const io = await draw(two);
  const frames = [...target.querySelectorAll<HTMLIFrameElement>(".tf-doc iframe")];
  expect(frames).toHaveLength(2);
  clicked(true);
  say({ knobas: "embed", open: "https://api.example/a" }, frames[0]!.contentWindow);
  say({ knobas: "embed", open: "https://api.example/b" }, frames[1]!.contentWindow);
  await settle();
  expect(io.openExternal).toHaveBeenCalledTimes(1);
  expect(io.openExternal).toHaveBeenCalledWith("https://api.example/a");
  expect(target.querySelector(".tf-doc .tf-embed .tf-macro")?.textContent).toContain("wants to open https://api.example/b");
});

test("a click stays active for seconds, and opens one tab for all of them", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  const io = await draw();
  clicked(true);
  say({ knobas: "embed", open: "https://api.example/a" });
  // Transient activation lasts 5 s in WebKit and Chromium: a script waiting out a short gap gets no second tab.
  vi.setSystemTime(Date.now() + 1_500);
  say({ knobas: "embed", open: "https://api.example/b" });
  await settle();
  expect(io.openExternal).toHaveBeenCalledTimes(1);
  expect(target.querySelector(".tf-doc .tf-embed .tf-macro")?.textContent).toContain("wants to open https://api.example/b");
});

// -- A URL embed (#2441) -------------------------------------------------------

const URL_EMBED = "https://grafana.example/d/abc?orgId=1";
const urlPage = (over: Partial<DocPage> = {}, url = URL_EMBED) =>
  page({
    blocks: [{ type: "embed", name: "iframe", content: { kind: "url", url } }],
    ...over,
  });

test("a URL embed is the same sandboxed frame with the URL as its source, and offers Open in browser beside it", async () => {
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(2000);
  const io = await draw(urlPage());
  const f = frame()!;
  // Its own origin and a referrer (#2460): YouTube's player needs both.
  expect(f.getAttribute("sandbox")).toBe(EMBED_URL_SANDBOX);
  expect(f.getAttribute("referrerpolicy")).toBe(EMBED_URL_REFERRER);
  expect(f.getAttribute("src")).toBe(URL_EMBED);
  expect(f.hasAttribute("srcdoc")).toBe(false);
  expect(f.getAttribute("title")).toBe("Embedded page from grafana.example");
  expect(f.getAttribute("height")).toBe(String(EMBED_URL_HEIGHT));
  // A site that refuses to be framed cannot be told from the app: the way out is always there.
  button("Open in browser")!.click();
  await settle();
  expect(io.openExternal).toHaveBeenCalledWith(URL_EMBED);
});

test("a URL embed is no taller than the window", async () => {
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(300);
  await draw(urlPage());
  expect(frame()!.getAttribute("height")).toBe("300");
});

test("a URL embed follows the window as it resizes", async () => {
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(300);
  await draw(urlPage());
  resize(2000);
  expect(frame()!.getAttribute("height")).toBe(String(EMBED_URL_HEIGHT));
  resize(250);
  expect(frame()!.getAttribute("height")).toBe("250");
});

test("a URL embed that never loads offers Open in browser for its URL; one that loaded stays", async () => {
  vi.useFakeTimers();
  const io = await draw(urlPage());
  vi.advanceTimersByTime(EMBED_REPORT_MS + 1);
  await settle();
  expect(frame()).toBeNull();
  expect(target.querySelector(".tf-doc .tf-macro")?.textContent).toContain("This embedded page did not load.");
  button("Open in browser")!.click();
  await settle();
  expect(io.openExternal).toHaveBeenCalledWith(URL_EMBED);

  if (view) unmount(view);
  target.replaceChildren();
  await draw(urlPage());
  frame()!.dispatchEvent(new Event("load"));
  vi.advanceTimersByTime(EMBED_REPORT_MS * 3);
  await settle();
  expect(frame()).not.toBeNull();
});

test("with Run embedded HTML off, or offline, a URL embed is a placeholder with Open in Confluence", async () => {
  const io = await draw(urlPage({ run_embeds: false }));
  expect(frame()).toBeNull();
  expect(target.querySelector(".tf-doc .tf-macro")?.textContent).toContain("This part is an embedded page. Run embedded HTML is off for Confluence");
  button("Open in Confluence")!.click();
  await settle();
  expect(io.openExternal).toHaveBeenCalledWith("http://wiki.example/pages/5");

  if (view) unmount(view);
  target.replaceChildren();
  vi.spyOn(navigator, "onLine", "get").mockReturnValue(false);
  await draw(urlPage());
  expect(frame()).toBeNull();
  expect(target.querySelector(".tf-doc .tf-macro")?.textContent).toContain("knobas is offline");
  expect(button("Open in Confluence")).toBeDefined();
});

test("a URL embed of another scheme, from any adapter, is never framed", async () => {
  await draw(urlPage({}, "javascript:alert(1)"));
  expect(frame()).toBeNull();
  expect(target.querySelector(".tf-doc .tf-macro")?.textContent).toContain("This embedded page did not load.");
});

test("a URL embed of the app's own origin is never framed: its own origin would be the app's (#2460)", async () => {
  for (const url of [`${window.location.origin}/#/docs`, "http://tauri.localhost/", "http://ipc.localhost/"]) {
    if (view) unmount(view);
    target.replaceChildren();
    await draw(urlPage({}, url));
    expect(frame(), url).toBeNull();
    expect(target.querySelector(".tf-doc .tf-macro")?.textContent).toContain("This embedded page did not load.");
  }
});

test("with Run embedded HTML off for its source, the embed is a placeholder with Open in Confluence", async () => {
  const io = await draw(page({ run_embeds: false }));
  expect(frame()).toBeNull();
  expect(target.querySelector(".tf-doc .tf-macro")?.textContent).toContain("Run embedded HTML is off for Confluence");
  button("Open in Confluence")!.click();
  await settle();
  expect(io.openExternal).toHaveBeenCalledWith("http://wiki.example/pages/5");
});

test("offline, the embed is a placeholder with Open in Confluence, and runs once the network is back", async () => {
  const online = vi.spyOn(navigator, "onLine", "get").mockReturnValue(false);
  await draw();
  expect(frame()).toBeNull();
  expect(target.querySelector(".tf-doc .tf-macro")?.textContent).toContain("knobas is offline");
  expect(button("Open in Confluence")).toBeDefined();
  online.mockReturnValue(true);
  window.dispatchEvent(new Event("online"));
  await settle();
  expect(frame()).not.toBeNull();
});

test("a frame that never says it loaded offers Open in browser", async () => {
  vi.useFakeTimers();
  const io = await draw();
  expect(frame()).not.toBeNull();
  vi.advanceTimersByTime(EMBED_REPORT_MS + 1);
  await settle();
  expect(frame()).toBeNull();
  expect(target.querySelector(".tf-doc .tf-macro")?.textContent).toContain("did not load");
  button("Open in browser")!.click();
  await settle();
  expect(io.openExternal).toHaveBeenCalledWith("http://wiki.example/pages/5");
});

test("a frame that reported stays, however long it then takes", async () => {
  vi.useFakeTimers();
  await draw();
  say({ knobas: "embed", height: 200 });
  vi.advanceTimersByTime(EMBED_REPORT_MS * 3);
  await settle();
  expect(frame()).not.toBeNull();
});
