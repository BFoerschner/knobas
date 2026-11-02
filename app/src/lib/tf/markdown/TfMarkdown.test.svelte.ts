/**
 * The Markdown component (#2255, spec #2234 step 5) mounted the way a page
 * mounts it: text in, a drawn and sanitized tree out, links that route inside
 * the app or go to the opener, and fences handed to the renderer registry.
 * The highlighter is a fake port; the sanitizer is the real DOMPurify on jsdom
 * (headless Chrome witnesses it again against the dev server).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiagramRequest, HighlightHandlers, HighlightRequest, RenderPort } from "../../render/port";
import { register } from "../../render/registry";
import { registerFence } from "../../render/markdown";
import { installCommands } from "../../render/command";
import type { RenderContent } from "../../ipc/app";
import TfMarkdown from "./TfMarkdown.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
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
