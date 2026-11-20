/**
 * The Markdown component (#2255, spec #2234 step 5) mounted the way a page
 * mounts it: text in, a drawn and sanitized tree out, links that route inside
 * the app or go to the opener, and fences handed to the renderer registry.
 * The highlighter is a fake port; the sanitizer is the real DOMPurify on jsdom
 * (headless Chrome witnesses it again against the dev server).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { warmDiagrams } from "../../render/diagram";
import { diagramKey, forgetDiagramSizes, keepDiagramSize } from "../../render/diagram-sizes";
import type { DiagramAnswer, DiagramRequest, HighlightHandlers, HighlightRequest, RenderPort } from "../../render/port";
import { register } from "../../render/registry";
import { registerFence } from "../../render/markdown";
import { installCommands } from "../../render/command";
import type { RenderContent } from "../../ipc/app";
import TfMarkdown from "./TfMarkdown.svelte";
import { IMAGE_HOLD_MS, blankImage, forgetImageSizes, keptImageSize, reloadImageSizes } from "./image-sizes";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  forgetDiagramSizes();
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

/** A highlighter that answers every request with each line as one keyword token. */
function keywordPort(asked: HighlightRequest[] = []): RenderPort {
  return {
    highlight(request: HighlightRequest, handlers: HighlightHandlers) {
      asked.push(request);
      let live = true;
      queueMicrotask(() => {
        if (!live) return;
        const lines = request.text.split("\n").map((line) => [[line, "kw"]] as [string, "kw"][]);
        handlers.onLines(0, lines);
        handlers.onDone?.();
      });
      return { focus() {}, cancel: () => (live = false) };
    },
  };
}

const opened: string[] = [];

function draw(text: string, extra: Record<string, unknown> = {}) {
  opened.length = 0;
  view = mount(TfMarkdown, {
    target,
    props: {
      text,
      base: "docs",
      fileLink: (path: string, line: number | null) => `#/repo/r/files/${path}${line === null ? "" : `@${line}`}`,
      openExternal: async (url: string) => void opened.push(url),
      render: keywordPort(),
      ...extra,
    },
  });
  flushSync();
}

async function settle() {
  for (let round = 0; round < 3; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const md = () => target.querySelector(".tf-md")!;
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";

const README = [
  "# payout-service",
  "",
  "| Area | State |",
  "|---|---|",
  "| Card | live |",
  "| SEPA | ~~manual~~ automated |",
  "",
  "- [x] Retry with backoff",
  "- [ ] Dead-letter queue",
  "",
  "```rust",
  "fn main() {}",
  "```",
  "",
  "See [the runbook](runbook.md) and https://example.com/wiki.",
].join("\n");

test("a README's table, task list, strikethrough, fenced Rust block, relative link and autolink all draw", async () => {
  draw(README);
  await settle();

  expect(text(md().querySelector("h1"))).toBe("payout-service");
  expect([...md().querySelectorAll("table th")].map(text)).toEqual(["Area", "State"]);
  expect([...md().querySelectorAll("table tbody tr")].map((row) => [...row.children].map(text))).toEqual([
    ["Card", "live"],
    ["SEPA", "manual automated"],
  ]);
  expect(text(md().querySelector("td del"))).toBe("manual");

  const tasks = [...md().querySelectorAll("li.tf-task")];
  expect(tasks.map(text)).toEqual(["Retry with backoff", "Dead-letter queue"]);
  expect(tasks.map((li) => li.querySelector(".tf-box")?.classList.contains("tf-on"))).toEqual([true, false]);

  const fence = md().querySelector(".tf-fence")!;
  expect(text(fence.querySelector("header"))).toContain("rust");
  expect(text(fence.querySelector("pre"))).toBe("fn main() {}");
  expect(fence.querySelector("pre .tf-hl-kw")?.textContent).toBe("fn main() {}");

  const links = [...md().querySelectorAll("a")];
  expect(links.map((a) => a.getAttribute("href"))).toEqual(["#/repo/r/files/docs/runbook.md", "https://example.com/wiki"]);
  expect(links[1]!.classList.contains("tf-ext")).toBe(true);
});

test("a relative link resolves against the file's directory, `..` and `/` included, and keeps a line anchor", () => {
  draw("[up](../README.md) [deep](img/../guide/a.md#L12) [root](/LICENSE) [here](./b.md)");
  expect([...md().querySelectorAll("a")].map((a) => a.getAttribute("href"))).toEqual([
    "#/repo/r/files/README.md",
    "#/repo/r/files/docs/guide/a.md@12",
    "#/repo/r/files/LICENSE",
    "#/repo/r/files/docs/b.md",
  ]);
});

test("an external link goes to the opener and does not navigate the app", () => {
  draw("[wiki](https://example.com/wiki)");
  const link = md().querySelector("a")!;
  const click = new MouseEvent("click", { bubbles: true, cancelable: true });
  link.dispatchEvent(click);
  expect(click.defaultPrevented).toBe(true);
  expect(opened).toEqual(["https://example.com/wiki"]);
});

test("a middle click opens no window: an external link goes to the opener, a file link does nothing", () => {
  draw("[wiki](https://example.com/wiki) [file](src/a.rs)");
  const [external, file] = [...md().querySelectorAll("a")];
  const middle = () => new MouseEvent("auxclick", { bubbles: true, cancelable: true, button: 1 });
  const onExternal = middle();
  external!.dispatchEvent(onExternal);
  const onFile = middle();
  file!.dispatchEvent(onFile);
  expect([onExternal.defaultPrevented, onFile.defaultPrevented]).toEqual([true, true]);
  expect(opened).toEqual(["https://example.com/wiki"]);
});

test("a relative link where the surface has no repository draws as text, not a link", () => {
  draw("[runbook](runbook.md)", { fileLink: null });
  expect(md().querySelector("a")?.hasAttribute("href") ?? false).toBe(false);
  expect(text(md())).toBe("runbook");
});

test("an in-page anchor scrolls to its heading instead of changing the route", () => {
  draw("[jump](#getting-started)\n\n## Getting started");
  const heading = md().querySelector("h2")!;
  let scrolled = false;
  heading.scrollIntoView = () => void (scrolled = true);
  const click = new MouseEvent("click", { bubbles: true, cancelable: true });
  md().querySelector("a")!.dispatchEvent(click);
  expect(click.defaultPrevented).toBe(true);
  expect(scrolled).toBe(true);
});

test("raw HTML is sanitized: scripts, event handlers, javascript: links, styles and forms are gone", () => {
  draw(
    [
      "<script>window.pwned = 1</script>",
      '<img src="x.png" onerror="window.pwned = 2">',
      '<a href="javascript:window.pwned=3">click</a>',
      '<p style="color:red" class="tf-modal" onclick="window.pwned=4">styled</p>',
      '<form action="https://example.com"><input name="q"><button>go</button></form>',
      '<iframe src="https://example.com"></iframe>',
      '<svg><a href="https://example.com"><text>svg</text></a></svg><math><mi>math</mi></math>',
      '<video src="https://t.example/v.mp4" poster="https://t.example/p.png"><source src="https://t.example/s.mp4"></video>',
      '<table background="https://t.example/b.png"><tr><td>cell</td></tr></table>',
      '<map><area href="https://example.com/" shape="default"></map>',
      "",
      "[md link](javascript:alert(1))",
    ].join("\n"),
  );
  expect(md().querySelector("script, iframe, form, input, button, [onerror], [onclick], [style]")).toBeNull();
  const urls = [...md().querySelectorAll("*")].flatMap((el) => [...el.attributes].map((attr) => attr.value));
  expect(urls.filter((value) => /javascript:/i.test(value))).toEqual([]);
  expect(md().querySelector("p.tf-modal")).toBeNull();
  expect(md().querySelector("svg, math, [poster], [background], [href]:not(a), [src]:not(img)")).toBeNull();
  expect(text(md())).toContain("styled");
  expect((window as unknown as { pwned?: number }).pwned).toBeUndefined();
});

test("a fence whose language no renderer claims but Source draws highlighted; an unknown one stays plain", async () => {
  const asked: HighlightRequest[] = [];
  draw("```zzq\nnothing\n```\n\n```\nbare\n```", { render: keywordPort(asked) });
  await settle();
  expect(asked.map((request) => "lang" in request && request.lang)).toEqual(["zzq"]);
  expect([...md().querySelectorAll(".tf-fence pre")].map(text)).toEqual(["nothing", "bare"]);
});

test("a fence goes to the renderer the registry names for its language", async () => {
  const undoRenderer = register({ id: "boxes", name: "Boxes", patterns: [], fences: ["boxes"], input: "text", output: "svg", diff: false });
  const undoFence = registerFence("boxes", (code, into) => {
    const drawn = document.createElement("p");
    drawn.className = "drawn";
    drawn.textContent = code.toUpperCase();
    into.replaceChildren(drawn);
  });
  try {
    draw("```boxes\na -> b\n```");
    await settle();
    expect(text(md().querySelector(".tf-fence .drawn"))).toBe("A -> B");
  } finally {
    undoFence();
    undoRenderer();
  }
});

test("a fence in a saved command renderer's language is run by it and drawn in its sandboxed frame (#2262)", async () => {
  const ran: [string, RenderContent][] = [];
  installCommands(
    [{ id: "cat", name: "Cat", command: "cat", patterns: [], fences: ["cat"], input: "text", output: "html", timeout_secs: 10, diff: false }],
    {
      run: async (id, content) => {
        ran.push([id, content]);
        return { outcome: "drawn", output: "html", body: content.kind === "text" ? content.text : "" };
      },
    },
  );
  try {
    draw("Before\n\n```Cat\n<b>bold</b><script>alert(1)</script>\n```\n\n```rust\nfn main() {}\n```");
    await settle();

    expect(ran).toEqual([["cat", { kind: "text", text: "<b>bold</b><script>alert(1)</script>" }]]);
    const frames = md().querySelectorAll(".tf-fence iframe");
    expect(frames).toHaveLength(1);
    expect(frames[0]!.getAttribute("sandbox")).toBe("");
    expect(frames[0]!.getAttribute("srcdoc")).toContain("<b>bold</b>");
    expect(frames[0]!.getAttribute("srcdoc")).not.toMatch(/<script/i);
    expect(text(md().querySelectorAll(".tf-fence")[1])).toContain("fn main() {}");
  } finally {
    installCommands([]);
  }
});

test("new text draws over the old", () => {
  view = mount(TfMarkdown, { target, props: { text: "# one", base: "", fileLink: null, render: keywordPort() } });
  flushSync();
  expect(text(md())).toBe("one");
  unmount(view);
  view = null;
  target.replaceChildren();
  const props = $state({ text: "# one", base: "", fileLink: null, render: keywordPort() });
  view = mount(TfMarkdown, { target, props });
  flushSync();
  props.text = "# two";
  flushSync();
  expect(text(md())).toBe("two");
});

/** `URL.createObjectURL` held for one test: each URL handed out, its blob, and which were revoked. */
function blobUrls() {
  const made = new Map<string, Blob>();
  const revoked: string[] = [];
  const create = URL.createObjectURL;
  const revoke = URL.revokeObjectURL;
  URL.createObjectURL = (blob: Blob) => {
    const url = `blob:test/${made.size}`;
    made.set(url, blob);
    return url;
  };
  URL.revokeObjectURL = (url: string) => void revoked.push(url);
  return { made, revoked, restore: () => ((URL.createObjectURL = create), (URL.revokeObjectURL = revoke)) };
}

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

test("a relative image loads through the byte read as a blob URL, resolved against the file's directory (#2256)", async () => {
  const urls = blobUrls();
  const asked: string[] = [];
  try {
    draw("![logo](img/logo.png) ![remote](https://t.example/x.png)", {
      image: async (path: string) => (asked.push(path), PNG),
    });
    await settle();
    expect(asked).toEqual(["docs/img/logo.png"]);
    const [logo, remote] = [...md().querySelectorAll("img")];
    expect(logo!.getAttribute("src")).toBe("blob:test/0");
    expect(urls.made.get("blob:test/0")!.type).toBe("image/png");
    expect(new Uint8Array(await urls.made.get("blob:test/0")!.arrayBuffer())).toEqual(PNG);
    expect(remote!.hasAttribute("src")).toBe(false);
    unmount(view!);
    view = null;
    expect(urls.revoked).toEqual(["blob:test/0"]);
  } finally {
    urls.restore();
  }
});

test("an SVG image is sanitized before it is drawn: no script, no handler, its drawing kept (#2256)", async () => {
  const urls = blobUrls();
  const svg = [
    '<svg width="10" height="10" onload="window.pwned = 5">',
    "<script>window.pwned = 6</script>",
    '<circle cx="5" cy="5" r="4" fill="teal"/>',
    '<foreignObject><iframe src="https://t.example"></iframe></foreignObject>',
    "</svg>",
  ].join("");
  try {
    draw("![diagram](arch.svg)", { image: async () => new TextEncoder().encode(svg) });
    await settle();
    const blob = urls.made.get(md().querySelector("img")!.getAttribute("src")!)!;
    expect(blob.type).toBe("image/svg+xml");
    const drawn = await blob.text();
    expect(drawn).toContain("<circle");
    expect(drawn).toMatch(/^<svg[^>]* xmlns="[^"]+svg"/);
    expect(drawn).not.toMatch(/script|onload|foreignObject|iframe/i);
  } finally {
    urls.restore();
  }
});

test("an image whose name only looks like an SVG type is no SVG: the sanitizer cannot be stepped around (#2256)", async () => {
  const urls = blobUrls();
  try {
    draw("![x](x.svg+xml;charset=utf-8)", { image: async () => new TextEncoder().encode("<svg><script>1</script></svg>") });
    await settle();
    const blob = urls.made.get(md().querySelector("img")!.getAttribute("src")!)!;
    expect(blob.type).toBe("application/octet-stream");
  } finally {
    urls.restore();
  }
});

test("an image read that answers after the text is gone makes no blob URL (#2256)", async () => {
  const urls = blobUrls();
  let answer: (bytes: Uint8Array) => void = () => {};
  try {
    draw("![logo](logo.png)", { image: () => new Promise<Uint8Array>((resolve) => (answer = resolve)) });
    await settle();
    unmount(view!);
    view = null;
    answer(PNG);
    await settle();
    expect(urls.made.size).toBe(0);
  } finally {
    urls.restore();
  }
});

test("an image the byte read refuses draws its alt text and the reason, and nothing loads (#2256)", async () => {
  const urls = blobUrls();
  try {
    draw("![the big one](huge.png)", {
      image: async () => {
        throw { code: "invalid", message: "docs/huge.png is 63.2 MB; a byte read takes up to 50 MB.", source_id: null };
      },
    });
    await settle();
    expect(md().querySelector("img")).toBeNull();
    const missing = md().querySelector(".tf-md-noimg")!;
    expect(text(missing)).toBe("the big one: docs/huge.png is 63.2 MB; a byte read takes up to 50 MB.");
    expect(urls.made.size).toBe(0);
  } finally {
    urls.restore();
  }
});

test("on a surface with no byte read a relative image loads nothing", async () => {
  draw("![logo](logo.png)");
  await settle();
  expect(md().querySelector("img")!.hasAttribute("src")).toBe(false);
});

const DOT = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

/** An attachment read over `files` (id to data URL): a missing id is refused, as an unreachable source refuses it. */
function attachmentRead(files: Record<string, string>, asked: string[] = []) {
  return {
    load: async (id: string) => {
      asked.push(id);
      const url = files[id];
      if (url === undefined) throw new Error("unreachable");
      return url;
    },
    missing: (name: string) => ({
      text: `${name}, not reachable in Tidewater Jira right now.`,
      link: { href: "https://jira.tidewater.example/browse/PAY-231", label: "Open in Tidewater Jira" },
    }),
  };
}

test("an attachment image draws where the text put it, from the surface's data URL, its name the alt (#2406)", async () => {
  const asked: string[] = [];
  draw("Before ![psp-409-trace.png](attachment:50000) after", { attachments: attachmentRead({ "50000": DOT }, asked) });
  await settle();
  const img = md().querySelector("p > img")!;
  expect(asked).toEqual(["50000"]);
  expect(img.getAttribute("src")).toBe(DOT);
  expect(img.getAttribute("alt")).toBe("psp-409-trace.png");
  expect(img.classList.contains("tf-md-att")).toBe(true);
  expect(text(md().querySelector("p"))).toBe("Before after");
});

test("an attachment image that cannot be fetched is the placeholder with its name and a link to the ticket in the source (#2406)", async () => {
  draw("![psp-409-trace.png](attachment:50000)", { attachments: attachmentRead({}) });
  await settle();
  expect(md().querySelector("img")).toBeNull();
  const missing = md().querySelector(".tf-md-noimg")!;
  expect(text(missing)).toBe("psp-409-trace.png, not reachable in Tidewater Jira right now. Open in Tidewater Jira");
  const link = missing.querySelector<HTMLAnchorElement>("a.tf-ext")!;
  expect(link.getAttribute("href")).toBe("https://jira.tidewater.example/browse/PAY-231");
  link.click();
  expect(opened).toEqual(["https://jira.tidewater.example/browse/PAY-231"]);
});

test("an attachment read answering anything but a data: image URL is the placeholder, and loads nothing (#2406)", async () => {
  draw("![a.png](attachment:1)", { attachments: attachmentRead({ "1": "https://jira.tidewater.example/a.png" }) });
  await settle();
  expect(md().querySelector("img")).toBeNull();
  expect(md().querySelector(".tf-md-noimg")).not.toBeNull();
});

test("an attachment link and an attachment image open the surface's preview of that file (#2415)", async () => {
  const shown: string[] = [];
  draw("See [sepa-error-codes.pdf](attachment:50002) and ![psp-409-trace.png](attachment:50000)", {
    attachments: { ...attachmentRead({ "50000": DOT }), open: (id: string) => void shown.push(id) },
  });
  await settle();
  const link = [...md().querySelectorAll<HTMLAnchorElement>("a[href]")].find((a) => text(a) === "sepa-error-codes.pdf")!;
  const event = new MouseEvent("click", { bubbles: true, cancelable: true });
  link.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(true);
  const img = md().querySelector<HTMLImageElement>("img.tf-md-att")!;
  expect(img.getAttribute("src")).toBe(DOT);
  // The image is a link a keyboard reaches, named by the file's name.
  const holder = img.closest<HTMLAnchorElement>("a[href]")!;
  expect(holder).not.toBeNull();
  img.click();
  expect(shown).toEqual(["50002", "50000"]);
  expect(opened).toEqual([]);
});

test("an image that cannot be fetched is the placeholder that says why, not a link to the preview; its source link stays the source's (#2415)", async () => {
  const shown: string[] = [];
  draw("![psp-409-trace.png](attachment:50000)", { attachments: { ...attachmentRead({}), open: (id: string) => void shown.push(id) } });
  await settle();
  const missing = md().querySelector(".tf-md-noimg")!;
  expect(missing.closest("a")).toBeNull();
  missing.querySelector<HTMLAnchorElement>("a.tf-ext")!.click();
  expect(opened).toEqual(["https://jira.tidewater.example/browse/PAY-231"]);
  expect(shown).toEqual([]);
});

test("without a preview, an attachment link is its name as text and goes nowhere (#2415)", async () => {
  draw("See [a.pdf](attachment:1)", { attachments: attachmentRead({}) });
  await settle();
  expect(text(md().querySelector("p"))).toBe("See a.pdf");
  expect(md().querySelector("a[href]")).toBeNull();
});

test("on a surface with no attachment read an attachment image loads nothing (#2406)", async () => {
  draw("![a.png](attachment:1)");
  await settle();
  expect(md().querySelector("img")!.hasAttribute("src")).toBe(false);
});

/** A diagram engine that draws each diagram as one text naming its palette and code, and fails on `BROKEN`. */
function diagramPort(asked: DiagramRequest[] = []): RenderPort {
  return {
    ...keywordPort(),
    async diagram(request) {
      asked.push(request);
      if (request.code.includes("BROKEN")) return { error: "Parse error on line 2:\nExpecting 'NODE_STRING', got 'EOF'" };
      return { svg: `<svg viewBox="0 0 120 40"><text onclick="parent.hit = 1">${request.palette.dark ? "dark" : "light"}:${request.code}</text></svg>` };
    },
  };
}

const MERMAID_MD = "Intro\n\n```mermaid\nflowchart LR\n  a --> b\n```\n";

/** The SVG inside a frame's `<img>`, decoded. */
function drawingIn(frame: HTMLIFrameElement): string {
  const src = new DOMParser().parseFromString(frame.srcdoc, "text/html").querySelector("img")?.getAttribute("src") ?? "";
  return new TextDecoder().decode(Uint8Array.from(atob(src.split(",")[1] ?? ""), (c) => c.charCodeAt(0)));
}

test("a mermaid fence is drawn by the diagram engine, sanitized, in a sandboxed frame sized to the drawing", async () => {
  const asked: DiagramRequest[] = [];
  draw(MERMAID_MD, { render: diagramPort(asked) });
  await settle();
  expect(asked.map((request) => [request.engine, request.code])).toEqual([["mermaid", "flowchart LR\n  a --> b"]]);
  const frame = md().querySelector<HTMLIFrameElement>(".tf-fence .tf-diagram iframe")!;
  expect(frame.getAttribute("sandbox")).toBe("");
  expect([frame.width, frame.height]).toEqual(["120", "40"]);
  const drawing = drawingIn(frame);
  expect(drawing).toContain("light:flowchart LR");
  expect(drawing).not.toContain("onclick");
  expect(md().querySelector(".tf-fence pre")).toBeNull();
});

test("a diagram that does not parse keeps its source and shows the parser's error above it", async () => {
  draw("```mermaid\nflowchart TD\n  BROKEN -->\n```", { render: diagramPort() });
  await settle();
  const body = md().querySelector(".tf-fence-body")!;
  expect(text(body.querySelector(".tf-stepfail b"))).toBe("Mermaid could not draw this.");
  expect(body.querySelector(".tf-stepfail pre")?.textContent).toContain("Parse error on line 2");
  expect(body.querySelector("pre code")?.textContent).toBe("flowchart TD\n  BROKEN -->");
  expect(body.querySelector("iframe")).toBeNull();
});

test("switching theme draws the diagram again in the other palette", async () => {
  const html = document.documentElement;
  const asked: DiagramRequest[] = [];
  try {
    draw(MERMAID_MD, { render: diagramPort(asked) });
    await settle();
    html.style.setProperty("--sheet", "#1A2027");
    html.style.setProperty("--ink", "#E4E9EE");
    html.setAttribute("data-theme", "dark");
    await settle();
    expect(asked.map((request) => request.palette.dark)).toEqual([false, true]);
    expect(asked[1]!.palette.sheet).toBe("#1A2027");
    expect(drawingIn(md().querySelector<HTMLIFrameElement>(".tf-diagram iframe")!)).toContain("dark:flowchart");
  } finally {
    html.removeAttribute("data-theme");
    html.style.removeProperty("--sheet");
    html.style.removeProperty("--ink");
  }
});

test("d2, dot and graphviz fences go to their own engines, each drawn in a sandboxed frame (#2259)", async () => {
  const asked: DiagramRequest[] = [];
  draw("```d2\napi -> outbox\n```\n\n```dot\ndigraph { a -> b }\n```\n\n```graphviz\ngraph { c }\n```\n", { render: diagramPort(asked) });
  await settle();
  expect(asked.map((request) => [request.engine, request.code])).toEqual([
    ["d2", "api -> outbox"],
    ["graphviz", "digraph { a -> b }"],
    ["graphviz", "graph { c }"],
  ]);
  const frames = [...md().querySelectorAll<HTMLIFrameElement>(".tf-fence .tf-diagram iframe")];
  expect(frames.map((frame) => [frame.getAttribute("sandbox"), frame.title])).toEqual([
    ["", "D2 diagram"],
    ["", "Graphviz diagram"],
    ["", "Graphviz diagram"],
  ]);
  expect(drawingIn(frames[0]!)).toContain("light:api -&gt; outbox");
  expect(frames.map(drawingIn).join("")).not.toContain("onclick");
});

test("a D2 or DOT fence that does not parse keeps its source under its engine's error (#2259)", async () => {
  draw("```d2\nBROKEN -> {\n```\n\n```dot\ndigraph { BROKEN\n```\n", { render: diagramPort() });
  await settle();
  const fails = [...md().querySelectorAll(".tf-fence-body")].map((body) => [
    text(body.querySelector(".tf-stepfail b")),
    body.querySelector("pre code")?.textContent,
  ]);
  expect(fails).toEqual([
    ["D2 could not draw this.", "BROKEN -> {"],
    ["Graphviz could not draw this.", "digraph { BROKEN"],
  ]);
  expect(md().querySelector("iframe")).toBeNull();
});

test("where the port has no diagram engine the fence stays its source", async () => {
  draw(MERMAID_MD);
  await settle();
  expect(md().querySelector(".tf-fence pre code")?.textContent).toBe("flowchart LR\n  a --> b");
  expect(md().querySelector("iframe")).toBeNull();
});

/** A diagram engine whose drawings land when the test says, in the order asked. */
function heldDiagrams() {
  const waiting: ((answer: DiagramAnswer) => void)[] = [];
  const port: RenderPort = { ...keywordPort(), diagram: () => new Promise((resolve) => waiting.push(resolve)) };
  const land = async (w: number, h: number) => {
    waiting.shift()!({ svg: `<svg viewBox="0 0 ${w} ${h}"><text>drawn</text></svg>` });
    await settle();
  };
  return {
    port,
    land,
    /** The app's warm-up, run through: the engine has answered in this session. */
    async warm() {
      warmDiagrams(port, (go) => go());
      await land(10, 10);
      await land(10, 10);
    },
  };
}

/** Every fence's body laid out in view, 125 px high from 100 px down, and each paragraph under it, as jsdom lays nothing out. */
function fencesInView() {
  const at = (top: number, height: number) => ({ x: 0, y: top, top, left: 0, width: 900, height, right: 900, bottom: top + height, toJSON: () => ({}) }) as DOMRect;
  return vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains("tf-fence-body")) return at(100, 125);
    return this.tagName === "P" && this.previousElementSibling?.classList.contains("tf-fence") ? at(260, 24) : at(0, 0);
  });
}

const DIAGRAM_README = "Intro\n\n```mermaid\nflowchart TB\n  a --> b\n```\n\nUnder it.";
const heldBlocks = () => [...md().children].map((el) => el.classList.contains("tf-heldbelow"));
const diagramFrame = () => md().querySelector<HTMLIFrameElement>(".tf-fence .tf-diagram iframe");

test("a diagram never seen, its engine warm, holds what stands from it down unseen until its drawing is in (#2510)", async () => {
  const engine = heldDiagrams();
  await engine.warm();
  const told: boolean[] = [];
  const laid = fencesInView();
  try {
    draw(DIAGRAM_README, { render: engine.port, onheld: (held: boolean) => told.push(held) });
    await settle();
    expect(heldBlocks(), "the intro reads at once; the diagram's fence and what stands under it wait").toEqual([false, true, true]);
    expect(md().hasAttribute("data-md-sizing")).toBe(true);
    expect(told).toEqual([true]);
    await engine.land(200, 226);
    expect(heldBlocks()).toEqual([false, false, false]);
    expect(told).toEqual([true, false]);
    // It landed with nothing drawn under it: its own size, in a room of its own.
    expect([diagramFrame()!.width, diagramFrame()!.height]).toEqual(["200", "226"]);
    expect(md().querySelector<HTMLElement>(".tf-fence-body")!.style.height).toBe("");
    expect(md().querySelector(".tf-dfull")).toBeNull();
  } finally {
    laid.mockRestore();
  }
});

test("the hold for a diagram ends after IMAGE_HOLD_MS, and the drawing that lands later keeps its source's room (#2510)", async () => {
  const engine = heldDiagrams();
  await engine.warm();
  const laid = fencesInView();
  try {
    draw(DIAGRAM_README, { render: engine.port });
    await settle();
    expect(heldBlocks()).toEqual([false, true, true]);
    await new Promise((resolve) => setTimeout(resolve, IMAGE_HOLD_MS + 20));
    expect(heldBlocks(), "the page under the diagram is drawn, with the diagram's source").toEqual([false, false, false]);
    expect(md().querySelector(".tf-fence pre code")?.textContent).toBe("flowchart TB\n  a --> b");
    await engine.land(200, 226);
    const body = md().querySelector<HTMLElement>(".tf-fence-body")!;
    expect(body.style.height, "the room the source had").toBe("125px");
    expect([diagramFrame()!.width, diagramFrame()!.height], "whole, scaled to it").toEqual(["100", "113"]);
    const full = md().querySelector<HTMLButtonElement>(".tf-fence > header .tf-dfull")!;
    expect(text(full)).toBe("Show at full size");
    full.click();
    expect([diagramFrame()!.width, diagramFrame()!.height, body.style.height]).toEqual(["200", "226", ""]);
  } finally {
    laid.mockRestore();
  }
});

test("a text drawn again keeps the room its late diagram was fitted into, and one shown at full size stays so (#2510)", async () => {
  const engine = heldDiagrams();
  const laid = fencesInView();
  const two = "```mermaid\nflowchart TB\n  a --> b\n```\n\nBetween.\n\n```mermaid\nflowchart TB\n  c --> d\n```\n\nUnder it.";
  const props = $state({ text: two, base: "", fileLink: null, render: engine.port });
  try {
    view = mount(TfMarkdown, { target, props });
    flushSync();
    await settle();
    await engine.land(200, 226);
    await engine.land(200, 226);
    const bodies = () => [...md().querySelectorAll<HTMLElement>(".tf-fence-body")];
    const frames = () => bodies().map((body) => { const frame = body.querySelector("iframe")!; return [frame.width, frame.height, body.style.height]; });
    expect(frames()).toEqual([["100", "113", "125px"], ["100", "113", "125px"]]);
    // The reader shows the first at full size; the second stays fitted.
    md().querySelectorAll<HTMLButtonElement>(".tf-dfull")[0]!.click();
    expect(frames()).toEqual([["200", "226", ""], ["100", "113", "125px"]]);
    const before = bodies();
    // Its read answers again: the same text, drawn again.
    props.base = "docs";
    flushSync();
    expect(bodies()[0]).not.toBe(before[0]);
    expect(frames(), "each stands as it stood, in the task the text is drawn in").toEqual([["200", "226", ""], ["100", "113", "125px"]]);
    expect(md().querySelectorAll(".tf-dfull")).toHaveLength(1);
  } finally {
    laid.mockRestore();
  }
});

test("a diagram whose engine is cold holds nothing: its drawing could not land inside the hold (#2510)", async () => {
  const engine = heldDiagrams();
  const told: boolean[] = [];
  draw(DIAGRAM_README, { render: engine.port, onheld: (held: boolean) => told.push(held) });
  await settle();
  expect(heldBlocks()).toEqual([false, false, false]);
  expect(told).toEqual([]);
});

test("a diagram seen before holds nothing and stands at the size it drew at from the first frame (#2510)", async () => {
  const engine = heldDiagrams();
  await engine.warm();
  keepDiagramSize(diagramKey("mermaid", "flowchart TB\n  a --> b", ""), { w: 200, h: 226 });
  draw(DIAGRAM_README, { render: engine.port });
  expect(heldBlocks()).toEqual([false, false, false]);
  expect([diagramFrame()!.width, diagramFrame()!.height, diagramFrame()!.hasAttribute("srcdoc")]).toEqual(["200", "226", false]);
  await engine.land(200, 226);
  expect(diagramFrame()!.srcdoc).toContain("<img");
});

test("Copy on a fence whose diagram is drawn copies the diagram's source", async () => {
  const copied: string[] = [];
  draw(MERMAID_MD, { render: diagramPort(), copy: async (code: string) => void copied.push(code) });
  await settle();
  expect(md().querySelector(".tf-fence pre")).toBeNull();
  md().querySelector<HTMLButtonElement>(".tf-fence-copy")!.click();
  await settle();
  expect(copied).toEqual(["flowchart LR\n  a --> b"]);
});

/** A long text: `parts` sections of a heading, a paragraph of about 1,000 characters and a fence each. */
function long(parts: number): string {
  const words = "rollback weekly trace region dashboard retries window payout index primary ".repeat(13);
  return Array.from({ length: parts }, (_, i) => `## Part ${i}\n\n${words}\n\n\`\`\`ts\nconst part${i} = ${i};\n\`\`\``).join("\n\n");
}

/** An intersection observer that finds every mark near the view, as a reader scrolled to it does. */
class NearObserver {
  constructor(private readonly said: (entries: { isIntersecting: boolean }[]) => void) {}
  observe() {
    queueMicrotask(() => this.said([{ isIntersecting: true }]));
  }
  disconnect() {}
}

test("a long text draws its first blocks, the rest a piece a task as its end nears the view, and its fences as each piece lands", async () => {
  const asked: HighlightRequest[] = [];
  const was = globalThis.IntersectionObserver;
  globalThis.IntersectionObserver = NearObserver as unknown as typeof IntersectionObserver;
  try {
    draw(long(120), { render: keywordPort(asked) });
  } finally {
    globalThis.IntersectionObserver = was;
  }
  const first = md().querySelectorAll("h2").length;
  expect(first).toBeGreaterThan(0);
  expect(first).toBeLessThan(120);
  expect(md().hasAttribute("data-md-held")).toBe(true);
  expect(md().lastElementChild?.classList.contains("tf-md-more")).toBe(true);
  expect(asked.length).toBe(md().querySelectorAll(".tf-fence").length);
  expect(asked.length).toBeLessThan(120);
  for (let round = 0; round < 40 && md().hasAttribute("data-md-held"); round += 1) await settle();
  expect(md().querySelectorAll("h2").length).toBe(120);
  expect(md().querySelector(".tf-md-more")).toBeNull();
  expect(md().hasAttribute("data-md-held")).toBe(false);
  expect(asked.map((request) => request.text)).toEqual(Array.from({ length: 120 }, (_, i) => `const part${i} = ${i};`));
  expect(md().querySelectorAll(".tf-hl-kw").length).toBe(120);
});

test("a long text drawn again gives a fence of a later piece its kept room, counted across the pieces (#2510)", async () => {
  const was = globalThis.IntersectionObserver;
  globalThis.IntersectionObserver = NearObserver as unknown as typeof IntersectionObserver;
  try {
    const props = $state({ text: long(40), base: "", fileLink: null, render: keywordPort() });
    view = mount(TfMarkdown, { target, props });
    flushSync();
    const whole = async () => {
      for (let round = 0; round < 40 && md().hasAttribute("data-md-held"); round += 1) await settle();
      return [...md().querySelectorAll<HTMLElement>(".tf-fence-body")];
    };
    const bodies = await whole();
    expect(bodies).toHaveLength(40);
    // The last fence, in the text's last piece, stands in a kept room.
    bodies[39]!.style.height = "125px";
    bodies[39]!.classList.add("tf-roomkept");
    props.base = "docs";
    flushSync();
    expect(md().querySelectorAll(".tf-fence").length, "its first piece only").toBeLessThan(40);
    const again = await whole();
    expect(again[39]).not.toBe(bodies[39]);
    expect(again.map((body) => (body.classList.contains("tf-roomkept") ? body.style.height : "")).filter((height) => height !== "")).toEqual(["125px"]);
    expect(again[39]!.style.height).toBe("125px");
  } finally {
    globalThis.IntersectionObserver = was;
  }
});

test("a short text draws whole at once, and an anchor in a long one draws the rest before it scrolls", () => {
  draw("## One\n\nshort");
  expect(md().hasAttribute("data-md-held")).toBe(false);
  expect(md().querySelector(".tf-md-more")).toBeNull();
  unmount(view!);
  view = null;
  draw(`[to the end](#part-119)\n\n${long(120)}`);
  expect(md().querySelector("#user-content-part-119")).toBeNull();
  let scrolled: Element | null = null;
  const proto = Element.prototype as Element & { scrollIntoView: (arg?: unknown) => void };
  const was = proto.scrollIntoView;
  proto.scrollIntoView = function (this: Element) {
    scrolled = this;
  };
  try {
    md().querySelector("a")!.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
  } finally {
    proto.scrollIntoView = was;
  }
  expect(scrolled).toBe(md().querySelector("#user-content-part-119"));
  expect(md().hasAttribute("data-md-held")).toBe(false);
});

// The nothing-moves pass (2026-11-17): a README read again after a sync drew its badge from nothing, and its bytes
// landing pushed the text under it 15 px down.
test("an image drawn again holds the size it loaded at before, while its bytes are read", async () => {
  forgetImageSizes();
  const pending = () => new Promise<Uint8Array>(() => {});
  draw("![badge](badge.png)\n\nUnder it.", { image: pending });
  await settle();
  const first = target.querySelector<HTMLImageElement>("img")!;
  expect(first.hasAttribute("width"), "never seen: no size to hold").toBe(false);
  Object.defineProperty(first, "naturalWidth", { value: 96 });
  Object.defineProperty(first, "naturalHeight", { value: 32 });
  first.dispatchEvent(new Event("load"));
  unmount(view!);
  view = null;
  draw("![badge](badge.png)\n\nUnder it.", { image: pending });
  await settle();
  const again = target.querySelector<HTMLImageElement>("img")!;
  expect([again.getAttribute("width"), again.getAttribute("height")]).toEqual(["96", "32"]);
});

// The ux21 README-image pass (2026-11-19): a README's logo, never seen in the session, pushed the text under it 15 px
// down when its bytes loaded. Its size is now read from the head of its bytes before its `src`, what stands from it down
// waits unseen until then, and the size is kept between sessions.

/** A PNG's head of `w` by `h`: all a size needs. */
function pngOf(w: number, h: number): Uint8Array {
  const b = new Uint8Array(33);
  b.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52]);
  new DataView(b.buffer).setUint32(16, w);
  new DataView(b.buffer).setUint32(20, h);
  return b;
}

/** A byte read whose answers the test hands out: `answer(path, bytes)` or `refuse(path)`. */
function heldRead() {
  const waiting = new Map<string, { resolve: (b: Uint8Array) => void; reject: (e: unknown) => void }>();
  return {
    read: (path: string) => new Promise<Uint8Array>((resolve, reject) => void waiting.set(path, { resolve, reject })),
    answer: (path: string, bytes: Uint8Array) => waiting.get(path)!.resolve(bytes),
    refuse: (path: string) => waiting.get(path)!.reject(new Error("gone")),
  };
}

const unseen = (el: Element | null) => el?.classList.contains("tf-heldbelow") ?? false;
const LOGO_README = "# payout-service\n\nIntro.\n\n![logo](img/logo.png)\n\nUnder it.\n\n## Next";

test("an image never seen holds what stands from it down unseen until its bytes say its size, set before its src", async () => {
  forgetImageSizes();
  const urls = blobUrls();
  const told: boolean[] = [];
  try {
    const bytes = heldRead();
    draw(LOGO_README, { image: bytes.read, imagesOf: "repo-a", onheld: (held: boolean) => told.push(held) });
    await settle();
    const blocks = [...md().children];
    const img = md().querySelector("img")!;
    expect(blocks.map(unseen), "the heading and intro read at once; the image's paragraph and all under it wait").toEqual([false, false, true, true, true]);
    expect(md().hasAttribute("data-md-sizing")).toBe(true);
    expect(told).toEqual([true]);
    let sizedAtSrc: string[] = [];
    const set = Object.getOwnPropertyDescriptor(HTMLImageElement.prototype, "src")!.set!;
    Object.defineProperty(img, "src", { set(url: string) { sizedAtSrc = [img.getAttribute("width")!, img.getAttribute("height")!]; set.call(img, url); }, configurable: true });
    bytes.answer("docs/img/logo.png", pngOf(96, 32));
    await settle();
    expect(sizedAtSrc, "the room is set before the image loads").toEqual(["96", "32"]);
    expect(img.getAttribute("src")).toBe("blob:test/0");
    expect([...md().children].some(unseen), "nothing waits once the size is in").toBe(false);
    expect(md().hasAttribute("data-md-sizing")).toBe(false);
    expect(told).toEqual([true, false]);
  } finally {
    urls.restore();
  }
});

test("a hold for an image's size ends after IMAGE_HOLD_MS when the bytes are slower, and when the read is refused", async () => {
  forgetImageSizes();
  const bytes = heldRead();
  draw("![slow](slow.png)\n\nUnder it.\n\n![gone](gone.png)", { image: bytes.read, imagesOf: "repo-a" });
  await settle();
  expect(unseen(md().children[0]!)).toBe(true);
  bytes.refuse("docs/gone.png");
  await settle();
  expect(unseen(md().children[0]!), "the first still waits").toBe(true);
  await new Promise((resolve) => setTimeout(resolve, IMAGE_HOLD_MS + 20));
  expect([...md().children].some(unseen)).toBe(false);
});

test("an image whose text names its size keeps it and holds nothing; one that names one side gets the other from its shape", async () => {
  forgetImageSizes();
  const bytes = heldRead();
  draw('<img src="both.png" width="50" height="20">\n\n<img src="one.png" width="48">\n\nUnder.', { image: bytes.read, imagesOf: "repo-a" });
  await settle();
  const [both, one] = [...md().querySelectorAll("img")];
  expect(unseen(both!.closest(".tf-md > *")), "both sides named: nothing to wait for").toBe(false);
  expect(unseen(one!.closest(".tf-md > *")), "one side named: the other waits for the bytes").toBe(true);
  bytes.answer("docs/both.png", pngOf(500, 500));
  bytes.answer("docs/one.png", pngOf(96, 32));
  await settle();
  expect([both!.getAttribute("width"), both!.getAttribute("height")]).toEqual(["50", "20"]);
  expect([one!.getAttribute("width"), one!.getAttribute("height")]).toEqual(["48", "16"]);
});

test("an image's size is kept between sessions, for its repository alone", async () => {
  // This runner's jsdom has no storage (`test-no-storage.ts`): one of the test's own, as the webview's.
  const items = new Map<string, string>();
  const storage = { getItem: (k: string) => items.get(k) ?? null, setItem: (k: string, v: string) => void items.set(k, v), removeItem: (k: string) => void items.delete(k) };
  Object.defineProperty(window, "localStorage", { value: storage, configurable: true, writable: true });
  try {
    await keptBetweenSessions(items);
  } finally {
    Object.defineProperty(window, "localStorage", { value: undefined, configurable: true, writable: true });
  }
});

async function keptBetweenSessions(items: Map<string, string>) {
  forgetImageSizes();
  const bytes = heldRead();
  draw(LOGO_README, { image: bytes.read, imagesOf: "repo-a" });
  await settle();
  bytes.answer("docs/img/logo.png", pngOf(96, 32));
  await settle();
  unmount(view!);
  view = null;
  // Written a moment after, once for a burst of images.
  for (let waited = 0; !items.has("knobas.image-sizes") && waited < 3000; waited += 50) await new Promise((resolve) => setTimeout(resolve, 50));
  reloadImageSizes();
  expect(JSON.parse(items.get("knobas.image-sizes")!), "written to the store").toEqual([["repo-a|docs/img/logo.png", 96, 32]]);
  expect(keptImageSize("repo-a|docs/img/logo.png"), "read back from the store").toEqual({ w: 96, h: 32 });
  draw(LOGO_README, { image: heldRead().read, imagesOf: "repo-a" });
  flushSync();
  const img = md().querySelector("img")!;
  expect([img.getAttribute("width"), img.getAttribute("height")], "drawn at its size from the first frame").toEqual(["96", "32"]);
  expect([...md().children].some(unseen), "and nothing waits").toBe(false);
  unmount(view!);
  view = null;
  draw(LOGO_README, { image: heldRead().read, imagesOf: "repo-b" });
  flushSync();
  expect(md().querySelector("img")!.hasAttribute("width"), "another repository's logo at the same path is another image").toBe(false);
}

test("a kept size the bytes contradict is set right before the image loads", async () => {
  forgetImageSizes();
  const first = heldRead();
  draw("![logo](logo.png)", { image: first.read, imagesOf: "repo-a" });
  await settle();
  first.answer("docs/logo.png", pngOf(96, 32));
  await settle();
  unmount(view!);
  view = null;
  const again = heldRead();
  draw("![logo](logo.png)", { image: again.read, imagesOf: "repo-a" });
  await settle();
  again.answer("docs/logo.png", pngOf(200, 50));
  await settle();
  const img = md().querySelector("img")!;
  expect([img.getAttribute("width"), img.getAttribute("height")]).toEqual(["200", "50"]);
  expect(keptImageSize("repo-a|docs/logo.png")).toEqual({ w: 200, h: 50 });
});

test("an image the text carries whole has its size from the first frame, with nothing held", () => {
  forgetImageSizes();
  draw(`![dot](${DOT})\n\nUnder it.`);
  const img = md().querySelector("img")!;
  expect([img.getAttribute("width"), img.getAttribute("height")]).toEqual(["1", "1"]);
  expect([...md().children].some(unseen)).toBe(false);
});

test("an attachment image is sized from its data URL before its src, no taller than the figure draws it", async () => {
  forgetImageSizes();
  const tall = `data:image/png;base64,${btoa(String.fromCharCode(...pngOf(400, 800)))}`;
  let answer: (url: string) => void = () => {};
  const read = { ...attachmentRead({}), load: () => new Promise<string>((resolve) => (answer = resolve)) };
  draw("![trace.png](attachment:50000)\n\nUnder it.", { attachments: read, imagesOf: "ticket-1" });
  await settle();
  expect([...md().children].map(unseen)).toEqual([true, true]);
  answer(tall);
  await settle();
  const img = md().querySelector("img")!;
  expect([img.getAttribute("width"), img.getAttribute("height")], "fitted to the 280 px figure, its shape kept").toEqual(["140", "280"]);
  expect(img.getAttribute("src")).toBe(tall);
  expect([...md().children].some(unseen)).toBe(false);
  expect(keptImageSize("ticket-1|attachment|50000")).toEqual({ w: 400, h: 800 });
});

test("an image whose room is known before its bytes stands as a blank of that size, not as its alt text, until they are in", async () => {
  forgetImageSizes();
  const urls = blobUrls();
  try {
    const first = heldRead();
    draw("![logo](logo.png)", { image: first.read, imagesOf: "repo-a" });
    await settle();
    expect(md().querySelector("img")!.hasAttribute("src"), "never seen: no size, so no stand-in").toBe(false);
    first.answer("docs/logo.png", pngOf(96, 32));
    await settle();
    unmount(view!);
    view = null;
    const again = heldRead();
    draw('![logo](logo.png)\n\n<img src="own.png" width="20" height="10" alt="own">', { image: again.read, imagesOf: "repo-a" });
    flushSync();
    const [logo, own] = [...md().querySelectorAll("img")];
    expect(logo!.getAttribute("src"), "the kept size").toBe(blankImage("96", "32"));
    expect(own!.getAttribute("src"), "the sides its text names").toBe(blankImage("20", "10"));
    // The stand-in's load says nothing of the image: nothing kept from it.
    Object.defineProperty(logo!, "naturalWidth", { value: 1, configurable: true });
    Object.defineProperty(logo!, "naturalHeight", { value: 1, configurable: true });
    logo!.dispatchEvent(new Event("load"));
    expect(keptImageSize("repo-a|docs/logo.png")).toEqual({ w: 96, h: 32 });
    again.answer("docs/logo.png", pngOf(96, 32));
    again.answer("docs/own.png", pngOf(500, 500));
    await settle();
    expect([logo!.getAttribute("src"), own!.getAttribute("src")]).toEqual(["blob:test/1", "blob:test/2"]);
    expect(logo!.hasAttribute("data-md-standin")).toBe(false);
  } finally {
    urls.restore();
  }
});

test("a blank stand-in is an SVG an <img> loads: its namespace on its root, its sides as given", () => {
  const url = blankImage("96", "32");
  expect(url.startsWith("data:image/svg+xml,")).toBe(true);
  const svg = new DOMParser().parseFromString(decodeURIComponent(url.slice("data:image/svg+xml,".length)), "image/svg+xml").documentElement;
  // The SVG namespace, as the HTML parser gives an <svg>: without it the webview loads nothing and draws the alt text.
  const parsed = document.createElement("div");
  parsed.innerHTML = "<svg></svg>";
  expect(svg.localName).toBe("svg");
  expect(svg.namespaceURI).toBe(parsed.firstElementChild!.namespaceURI);
  expect([svg.getAttribute("width"), svg.getAttribute("height")]).toEqual(["96", "32"]);
});
