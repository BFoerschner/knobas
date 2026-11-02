/**
 * A page's embed in the Docs reader (#2440, spec #2432 *Embeds in the
 * shell*), drawn through `TfDocs` as a reader opens it. What the backend
 * serves the frame under is `renderers.rs`'s test; the document and the
 * reporter are `render/embed.test.ts`'s.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DocPage, DocsTree } from "../../ipc/entity";
import { EMBED_SANDBOX } from "../../render/embed";
import { EMBED_REPORT_MS } from "./docs";
import TfDocs, { type DocsPorts } from "./TfDocs.svelte";

const TREE: DocsTree = {
  spaces: [{ source_id: "confluence", key: "ENG", name: "Engineering", pages: [{ entity_id: "confluence:5", title: "Payments API", parent: null }] }],
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
  expect(f.getAttribute("title")).toBe("Embedded HTML from Confluence");
  // No bridge in a test: the document is a srcdoc, the author's HTML untouched in it.
  expect(f.getAttribute("srcdoc")).toContain(HTML);
  expect(target.querySelector(".tf-doc .tf-macro")).toBeNull();
});

test("the frame's height follows what its reporter says, up to the viewport", async () => {
  await draw();
  vi.spyOn(window, "innerHeight", "get").mockReturnValue(700);
  say({ knobas: "embed", height: 312 });
  expect(frame()!.style.height).toBe("312px");
  say({ knobas: "embed", height: 4000 });
  expect(frame()!.style.height).toBe("700px");
  // Another window saying the same is not the frame.
  say({ knobas: "embed", height: 50 }, window);
  expect(frame()!.style.height).toBe("700px");
});

test("a link or popup from the frame opens through the external-open path", async () => {
  const io = await draw();
  say({ knobas: "embed", open: "https://api.example/docs" });
  await settle();
  expect(io.openExternal).toHaveBeenCalledWith("https://api.example/docs");
  // Only from the frame itself.
  say({ knobas: "embed", open: "https://evil.example/" }, window);
  await settle();
  expect(io.openExternal).toHaveBeenCalledTimes(1);
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
