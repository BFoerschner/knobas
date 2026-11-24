/**
 * Rendered diffs (#2279; ux26's stage) in the one diff component, mounted as its callers mount it: a file one of whose
 * renderers draws both sides offers *Lines | Rendered*, which draws the file before and after in a stage whose height
 * the renderer's kind sets; an image also swipes and onion-skins. The engines are the jsdom ones (Markdown for real, a
 * fake command port); headless Chrome witnesses the drawn stage.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { RenderContent, RenderRun, UserRenderer } from "../../ipc/app";
import type { DiffFile, DiffLine } from "../../ipc/entity";
import { installCommands, type CommandPort } from "../../render/command";
import { createDiffLayout } from "./diff-layout.svelte";
import { imageMode } from "./image-mode.svelte";
import TfDiffView from "./TfDiffView.svelte";

const ctx = (old: number, nu: number, text: string): DiffLine => ({ kind: "context", old, new: nu, text });
const del = (old: number, text: string): DiffLine => ({ kind: "del", old, new: null, text });
const add = (nu: number, text: string): DiffLine => ({ kind: "add", old: null, new: nu, text });

const file = (path: string, over: Partial<DiffFile> = {}): DiffFile => ({
  path,
  old_path: null,
  change: "modified",
  additions: 1,
  deletions: 1,
  hunks: [{ header: "@@ -1,2 +1,2 @@", lines: [del(1, "# Payouts"), add(1, "# Payout service"), ctx(2, 2, "")] }],
  truncated: false,
  threads: [],
  ...over,
});

const README = file("README.md");
const BEFORE = "# Payouts\n\nRetries **three** times.";
const AFTER = "# Payout service\n\nRetries **five** times.";
const PNG = file("assets/logo.png", { change: "binary", additions: 0, deletions: 0, hunks: [] });
/** A PNG's first bytes: enough for a data URL, not for a picture. */
const pngBytes = (tag: number) => new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, tag]);

let target: HTMLElement;
let views: ReturnType<typeof mount>[] = [];

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

/** `URL.createObjectURL` held for each test: each URL handed out with its blob, and which were revoked. */
let blobs: { made: Map<string, Blob>; revoked: string[]; restore: () => void };
function holdBlobs() {
  const made = new Map<string, Blob>();
  const revoked: string[] = [];
  const [create, revoke] = [URL.createObjectURL, URL.revokeObjectURL];
  URL.createObjectURL = (blob: Blob) => {
    const url = `blob:test/${made.size}`;
    made.set(url, blob);
    return url;
  };
  URL.revokeObjectURL = (url: string) => void revoked.push(url);
  return { made, revoked, restore: () => ((URL.createObjectURL = create), (URL.revokeObjectURL = revoke)) };
}
beforeEach(() => (blobs = holdBlobs()));

afterEach(() => {
  views.forEach((view) => unmount(view));
  views = [];
  target.remove();
  installCommands([]);
  blobs.restore();
  imageMode.set("sides");
});

function draw(props: Record<string, unknown>) {
  views.push(mount(TfDiffView, { target, props: { layout: createDiffLayout(() => null), defaults: { list: [] }, ...props } as never }));
  flushSync();
}

const settle = async () => {
  for (let n = 0; n < 8; n += 1) await Promise.resolve();
  flushSync();
};

const enc = (text: string) => new TextEncoder().encode(text);

/**
 * Reads each side as its bytes, as every caller's reader does (the clone first), and its text whole for expanded
 * context; counts the reads of each.
 */
function reader(texts: Record<string, [old: string | null, now: string | null]>, raw: Record<string, [old: Uint8Array | null, now: Uint8Array | null]> = {}) {
  const asked: string[] = [];
  const bytesOf = (f: DiffFile, side: "old" | "new"): Uint8Array | null => {
    const at = side === "old" ? 0 : 1;
    if (raw[f.path]) return raw[f.path]![at];
    const text = texts[f.path]?.[at];
    return text === null || text === undefined ? null : enc(text);
  };
  return {
    asked,
    context: {
      text: (f: DiffFile, side: "old" | "new") => (asked.push(`${side}:${f.path}`), Promise.resolve(texts[f.path]?.[side === "old" ? 0 : 1] ?? null)),
      bytes: (f: DiffFile, side: "old" | "new") => (asked.push(`bytes ${side}:${f.path}`), Promise.resolve(bytesOf(f, side))),
    },
  };
}

/** The card for `path`, its header's mode buttons, and the stage's parts. */
const card = (path: string) => [...target.querySelectorAll(".tf-diff")].find((el) => el.querySelector(".tf-fh .tf-mono")?.textContent === path)!;
const modes = (path: string) => [...card(path).querySelectorAll<HTMLButtonElement>(".tf-fh .tf-seg button")].map((b) => `${b.textContent}${b.getAttribute("aria-pressed") === "true" ? "*" : ""}`);
const mode = (path: string, name: string) => [...card(path).querySelectorAll<HTMLButtonElement>(".tf-fh .tf-seg button")].find((b) => b.textContent === name)!.click();
const sides = (path: string) => [...card(path).querySelectorAll(".tf-rcap-side")].map((side) => side.textContent?.trim());
const side = (path: string, which: "old" | "new") => card(path).querySelector<HTMLElement>(`.tf-rside-${which}`)!;
const stage = (path: string) => card(path).querySelector<HTMLElement>(".tf-rstage")!;
const imageModes = (path: string) => [...card(path).querySelectorAll<HTMLButtonElement>(".tf-rcap .tf-seg button")].map((b) => `${b.textContent}${b.getAttribute("aria-pressed") === "true" ? "*" : ""}`);
const imageModeTo = (path: string, name: string) => [...card(path).querySelectorAll<HTMLButtonElement>(".tf-rcap .tf-seg button")].find((b) => b.textContent === name)!.click();
const showAll = (path: string) => card(path).querySelector<HTMLButtonElement>(".tf-rfoot-all")!;

test("a changed README shows Lines first, and Rendered draws it before and after in the stage", async () => {
  const { context } = reader({ "README.md": [BEFORE, AFTER] });
  draw({ files: [README], context });
  expect(modes("README.md")).toEqual(["Lines*", "Rendered"]);
  expect(card("README.md").querySelector(".tf-dbody")).not.toBeNull();

  mode("README.md", "Rendered");
  await settle();
  expect(modes("README.md")).toEqual(["Lines", "Rendered*"]);
  expect(card("README.md").querySelector(".tf-dbody")).toBeNull();
  expect(sides("README.md")).toEqual(["before", "after"]);
  expect(side("README.md", "old").querySelector("h1")?.textContent).toBe("Payouts");
  expect(side("README.md", "old").querySelector("strong")?.textContent).toBe("three");
  expect(side("README.md", "new").querySelector("h1")?.textContent).toBe("Payout service");
  expect(side("README.md", "new").querySelector("strong")?.textContent).toBe("five");

  mode("README.md", "Lines");
  flushSync();
  expect(card("README.md").querySelector(".tf-rstage")).toBeNull();
  expect(card("README.md").querySelector(".tf-dbody")).not.toBeNull();
});

test("the caption, the stage and the foot are drawn in the flush that turns the card, before any side answers", async () => {
  const late: Record<string, (bytes: Uint8Array | null) => void> = {};
  const context = {
    text: () => Promise.resolve(null),
    bytes: (_: DiffFile, which: "old" | "new") => new Promise<Uint8Array | null>((done) => (late[which] = done)),
  };
  draw({ files: [README], context, sideNames: () => ({ old: "main at 4f2a91c", new: "working tree" }) });
  mode("README.md", "Rendered");
  flushSync();
  const box = stage("README.md");
  expect(card("README.md").querySelector(".tf-rcap")).not.toBeNull();
  expect(card("README.md").querySelector(".tf-rfoot")).not.toBeNull();
  expect(sides("README.md")).toEqual(["before main at 4f2a91c", "after working tree"]);
  expect([...box.classList].sort()).toEqual(["tf-rstage"]);
  expect(side("README.md", "old").querySelectorAll(".tf-skel-bar")).toHaveLength(3);
  expect(card("README.md").querySelector(".tf-rfoot-say")?.textContent).toBe("Reading both sides…");
  expect(showAll("README.md").getAttribute("aria-disabled")).toBe("true");

  late["old"]!(enc(BEFORE));
  await settle();
  // The answer lands inside its half: the stage is the same node, its classes as they were.
  expect(stage("README.md")).toBe(box);
  expect([...box.classList].sort()).toEqual(["tf-rstage"]);
  expect(side("README.md", "old").querySelector("h1")?.textContent).toBe("Payouts");
  expect(side("README.md", "new").querySelectorAll(".tf-skel-bar")).toHaveLength(3);
  expect(card("README.md").querySelector(".tf-rfoot-say")?.textContent).toBe("Reading the working tree's side…");
  showAll("README.md").click();
  flushSync();
  expect(box.classList.contains("tf-rstage-open")).toBe(false);

  late["new"]!(enc(AFTER));
  await settle();
  expect(stage("README.md")).toBe(box);
  expect(showAll("README.md").getAttribute("aria-disabled")).toBeNull();
  showAll("README.md").click();
  flushSync();
  expect(box.classList.contains("tf-rstage-open")).toBe(true);
  expect(showAll("README.md").textContent).toBe("Show less");
});

test("a text side is read as bytes, never through the source's text read, and a second time asks nothing", async () => {
  const { asked, context } = reader({ "README.md": [BEFORE, AFTER] });
  draw({ files: [README], context });
  mode("README.md", "Rendered");
  await settle();
  mode("README.md", "Lines");
  flushSync();
  mode("README.md", "Rendered");
  await settle();
  expect(asked.sort()).toEqual(["bytes new:README.md", "bytes old:README.md"]);
  expect(sides("README.md")).toEqual(["before", "after"]);
});

test("an added README shows its after side only, and a deleted one its before side, from their hunks", async () => {
  const added = file("docs/new.md", { change: "added", deletions: 0, hunks: [{ header: "@@ -0,0 +1 @@", lines: [add(1, "# Fresh")] }] });
  const gone = file("docs/old.md", { change: "deleted", additions: 0, hunks: [{ header: "@@ -1 +0,0 @@", lines: [del(1, "# Stale")] }] });
  draw({ files: [added, gone] });
  mode("docs/new.md", "Rendered");
  mode("docs/old.md", "Rendered");
  await settle();
  expect(sides("docs/new.md")).toEqual(["after"]);
  expect(stage("docs/new.md").classList.contains("tf-rstage-one")).toBe(true);
  expect(card("docs/new.md").querySelector("h1")?.textContent).toBe("Fresh");
  expect(sides("docs/old.md")).toEqual(["before"]);
  expect(card("docs/old.md").querySelector("h1")?.textContent).toBe("Stale");
});

test("a changed README with no reader offers no rendered mode: its hunks are not the file", () => {
  draw({ files: [README] });
  expect(modes("README.md")).toEqual([]);
});

test("a source file offers no rendered mode, and its header reads as before", () => {
  const { context } = reader({});
  draw({ files: [file("src/retry.rs")], context });
  expect(modes("src/retry.rs")).toEqual([]);
  expect(card("src/retry.rs").querySelector(".tf-fh")?.textContent?.replace(/\s+/g, " ").trim()).toBe("M src/retry.rs +1 −1");
});

test("a side that cannot be read says why in its half, beside the side that was", async () => {
  const { context } = reader({ "README.md": [BEFORE, AFTER] });
  draw({ files: [README], context: { ...context, bytes: (f: DiffFile, which: "old" | "new") => (which === "old" ? Promise.reject(new Error("gone")) : context.bytes(f, which)) } });
  mode("README.md", "Rendered");
  await settle();
  expect(side("README.md", "old").textContent).toContain("The side before could not be read: gone");
  expect(side("README.md", "new").querySelector("h1")?.textContent).toBe("Payout service");
});

test("an .html file, which opens as Source, offers Rendered and draws each side as a page in a sandboxed frame", async () => {
  const { context } = reader({ "site/index.html": ["<h1>Old</h1>", "<h1>New</h1>"] });
  draw({ files: [file("site/index.html")], context, frame: () => Promise.resolve("t") });
  expect(modes("site/index.html")).toEqual(["Lines*", "Rendered"]);
  mode("site/index.html", "Rendered");
  await settle();
  const frames = [...stage("site/index.html").querySelectorAll("iframe")];
  expect(frames.map((f) => f.getAttribute("sandbox"))).toEqual(["", ""]);
  expect(frames.map((f) => f.getAttribute("srcdoc")?.includes("Old"))).toEqual([true, false]);
  expect(frames.every((f) => f.classList.contains("tf-rstage-frame"))).toBe(true);
});

const PAGE: UserRenderer = { id: "pager", name: "Pager", command: "pager", patterns: ["*.page"], fences: [], input: "text", output: "html", timeout_secs: 10, diff: true };

function commands() {
  const ran: string[] = [];
  const port: CommandPort = {
    run: (id: string, content: RenderContent): Promise<RenderRun> => {
      const text = content.kind === "text" ? content.text : "";
      ran.push(`${id}:${text}`);
      return Promise.resolve({ outcome: "drawn", output: "html", body: `<p>${text}</p>` });
    },
  };
  return { ran, port };
}

test("a user renderer that declares diff support draws each side through its command", async () => {
  const { ran, port } = commands();
  installCommands([PAGE, { ...PAGE, id: "plain", name: "Plain", patterns: ["*.txt"], diff: false }], port);
  const { context } = reader({ "site/home.page": ["old page", "new page"], "notes.txt": ["a", "b"] });
  draw({ files: [file("site/home.page"), file("notes.txt")], context, commands: port });
  expect(modes("notes.txt")).toEqual([]);
  expect(modes("site/home.page")).toEqual(["Lines*", "Rendered"]);

  mode("site/home.page", "Rendered");
  await settle();
  expect(ran.sort()).toEqual(["pager:new page", "pager:old page"]);
  const frames = [...stage("site/home.page").querySelectorAll("iframe")];
  expect(frames.map((f) => f.getAttribute("sandbox"))).toEqual(["", ""]);
  expect(frames.map((f) => f.getAttribute("srcdoc")?.includes("<p>old page</p>"))).toEqual([true, false]);
});

test("a changed PNG opens rendered, side by side from each side's bytes, then swipes and onion-skins", async () => {
  const { asked, context } = reader({}, { "assets/logo.png": [pngBytes(1), pngBytes(2)] });
  draw({ files: [PNG], context });
  expect(modes("assets/logo.png")).toEqual(["Bytes", "Rendered*"]);
  await settle();
  expect(stage("assets/logo.png").classList.contains("tf-rstage-img")).toBe(true);
  expect(imageModes("assets/logo.png")).toEqual(["Side by side*", "Swipe", "Onion"]);
  expect(sides("assets/logo.png")).toEqual(["before", "after"]);
  const srcs = [...stage("assets/logo.png").querySelectorAll<HTMLImageElement>(".tf-rside img")].map((img) => img.getAttribute("src"));
  expect(srcs).toHaveLength(2);
  expect(srcs.map((src) => blobs.made.get(src!)?.type)).toEqual(["image/png", "image/png"]);
  expect(new Uint8Array(await blobs.made.get(srcs[0]!)!.arrayBuffer()).at(-1)).toBe(1);
  expect(new Uint8Array(await blobs.made.get(srcs[1]!)!.arrayBuffer()).at(-1)).toBe(2);

  imageModeTo("assets/logo.png", "Swipe");
  await settle();
  const box = stage("assets/logo.png");
  const swipe = box.querySelector(".tf-swipe")!;
  expect([...swipe.querySelectorAll("img")].map((img) => img.getAttribute("src"))).toEqual(srcs);
  const range = swipe.querySelector<HTMLInputElement>("input[type=range]")!;
  const over = swipe.querySelector<HTMLElement>(".tf-swipe-after")!;
  expect(over.style.getPropertyValue("clip-path")).toBe("inset(0 0 0 50%)");
  range.value = "20";
  range.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(over.style.getPropertyValue("clip-path")).toBe("inset(0 0 0 20%)");

  imageModeTo("assets/logo.png", "Onion");
  flushSync();
  const onion = stage("assets/logo.png").querySelector<HTMLElement>(".tf-swipe-after")!;
  expect(onion.classList.contains("tf-onion")).toBe(true);
  expect(onion.style.getPropertyValue("clip-path")).toBe("");
  expect(onion.style.getPropertyValue("opacity")).toBe("0.2");
  range.value = "75";
  range.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(onion.style.getPropertyValue("opacity")).toBe("0.75");
  expect(stage("assets/logo.png")).toBe(box);
  expect(asked.sort()).toEqual(["bytes new:assets/logo.png", "bytes old:assets/logo.png"]);

  mode("assets/logo.png", "Bytes");
  flushSync();
  expect(card("assets/logo.png").textContent).toContain("A binary file: no lines to draw.");
});

test("the image's mode is one choice app-wide: a second image opens the way the first was left", async () => {
  const OTHER = file("assets/mark.png", { change: "binary", additions: 0, deletions: 0, hunks: [] });
  const { context } = reader({}, { "assets/logo.png": [pngBytes(1), pngBytes(2)], "assets/mark.png": [pngBytes(3), pngBytes(4)] });
  draw({ files: [PNG, OTHER], context });
  await settle();
  imageModeTo("assets/logo.png", "Onion");
  await settle();
  expect(imageModes("assets/mark.png")).toEqual(["Side by side", "Swipe", "Onion*"]);
});

test("a changed D2 and Graphviz file draw before and after through the diagram engine", async () => {
  const { context } = reader({ "docs/flow.d2": ["a -> b", "a -> c"], "docs/flow.gv": ["digraph { a -> b }", "digraph { a -> c }"] });
  draw({ files: [file("docs/flow.d2"), file("docs/flow.gv")], context });
  for (const path of ["docs/flow.d2", "docs/flow.gv"]) {
    expect(modes(path)).toEqual(["Lines*", "Rendered"]);
    mode(path, "Rendered");
  }
  await settle();
  for (const [path, codes] of [
    ["docs/flow.d2", ["a -> b", "a -> c"]],
    ["docs/flow.gv", ["digraph { a -> b }", "digraph { a -> c }"]],
  ] as const) {
    const panes = [...stage(path).querySelectorAll(".tf-rside .tf-fdiagram")];
    // jsdom's port has no engine, so each side keeps its source (numbered lines).
    expect(panes).toHaveLength(2);
    codes.forEach((code, at) => expect(panes[at]!.textContent).toContain(code));
    expect(panes[0]!.textContent).not.toContain(codes[1]);
  }
});

test("a swipe whose side cannot be read says so instead of reading on", async () => {
  const { context } = reader({}, { "assets/logo.png": [pngBytes(1), pngBytes(2)] });
  const failing = { ...context, bytes: (f: DiffFile, which: "old" | "new") => (which === "new" ? Promise.reject(new Error("refused")) : context.bytes(f, which)) };
  draw({ files: [PNG], context: failing });
  await settle();
  imageModeTo("assets/logo.png", "Swipe");
  flushSync();
  const box = stage("assets/logo.png");
  expect(box.textContent).toContain("The side after could not be read: refused");
  expect(box.querySelector(".tf-skel")).toBeNull();
});

test("an image's URLs are let go with the file, and an SVG side is sanitized", async () => {
  const svg = (word: string) => new TextEncoder().encode(`<svg onload="alert(1)"><script>alert(2)</script><text>${word}</text></svg>`);
  const SVG = file("assets/mark.svg", { change: "binary", additions: 0, deletions: 0, hunks: [] });
  const { context } = reader({}, { "assets/mark.svg": [svg("old"), svg("new")] });
  draw({ files: [SVG], context });
  await settle();
  const made = [...blobs.made.entries()];
  expect(made).toHaveLength(2);
  const texts = await Promise.all(made.map(([, blob]) => blob.text()));
  expect(made.map(([, blob]) => blob.type)).toEqual(["image/svg+xml", "image/svg+xml"]);
  expect(texts.some((text) => /script|onload/i.test(text))).toBe(false);
  expect(texts.map((text) => text.includes(">old<"))).toEqual([true, false]);

  views.forEach((view) => unmount(view));
  views = [];
  expect(blobs.revoked.sort()).toEqual(made.map(([url]) => url).sort());
});

test("a PNG with no side before it (added) shows its after side only, with no modes of its own", async () => {
  const { context } = reader({}, { "assets/logo.png": [null, pngBytes(2)] });
  draw({ files: [PNG], context });
  await settle();
  expect(sides("assets/logo.png")).toEqual(["after"]);
  expect(modes("assets/logo.png")).toEqual(["Bytes", "Rendered*"]);
  expect(imageModes("assets/logo.png")).toEqual([]);
});

test("a PNG where no bytes reader is offered stays a binary file", () => {
  const { context } = reader({});
  draw({ files: [PNG], context: { text: context.text } });
  expect(modes("assets/logo.png")).toEqual([]);
  expect(card("assets/logo.png").textContent).toContain("A binary file: no lines to draw.");
});


// A rendered Markdown file's changed blocks (ux26, ruling 3): each top-level block holding a changed line is marked on
// its own side; the stage opens at the first, and `n` steps to the next, both columns with it.
const DOC_BEFORE = "# Retrying a payout\n\nA payout that fails is sent again.\n\n## Steps\n\n- Validate\n- Retry once\n\n## Inputs\n\nThe mandate.\n";
const DOC_AFTER = "# Retrying a payout\n\nA payout that fails is sent again with backoff.\n\n## Steps\n\n- Validate\n- Retry three times\n\n## Inputs\n\nThe mandate.\n\n## Alerts\n\nThe channel hears.\n";
const DOC = file("docs/retry.md", {
  additions: 6,
  deletions: 2,
  hunks: [
    { header: "@@ -1,8 +1,8 @@", lines: [ctx(1, 1, "# Retrying a payout"), ctx(2, 2, ""), del(3, "A payout that fails is sent again."), add(3, "A payout that fails is sent again with backoff."), ctx(4, 4, ""), ctx(5, 5, "## Steps"), ctx(6, 6, ""), ctx(7, 7, "- Validate"), del(8, "- Retry once"), add(8, "- Retry three times")] },
    { header: "@@ -12 +12,5 @@", lines: [ctx(12, 12, "The mandate."), add(13, ""), add(14, "## Alerts"), add(15, ""), add(16, "The channel hears.")] },
  ],
});
const marked = (path: string, at: "old" | "new") => [...side(path, at).querySelectorAll(".tf-md > .tf-rchg")].map((el) => el.tagName.toLowerCase() + " " + el.textContent!.trim().slice(0, 20).trim());

test("a Markdown file's changed blocks are marked on their own side, and n steps to the next change", async () => {
  const { context } = reader({ "docs/retry.md": [DOC_BEFORE, DOC_AFTER] });
  // Where a marked block stands in the stage: its first line's number, 20 px a line (jsdom lays nothing out).
  const real = Element.prototype.getBoundingClientRect;
  Element.prototype.getBoundingClientRect = function (this: Element) {
    const lines = /^(\d+)-/.exec(this.getAttribute("data-md-lines") ?? "");
    // As laid out, a block moves up as its stage scrolls.
    const scrolled = this.closest(".tf-rstage")?.scrollTop ?? 0;
    return { top: lines ? Number(lines[1]) * 20 - scrolled : 0, bottom: 0, left: 0, right: 0, width: 0, height: 0, x: 0, y: 0, toJSON: () => ({}) } as DOMRect;
  };
  try {
    draw({ files: [DOC], context });
    mode("docs/retry.md", "Rendered");
    await settle();
    expect(marked("docs/retry.md", "old")).toEqual(["p A payout that fails", "ul Validate\nRetry once"]);
    expect(marked("docs/retry.md", "new")).toEqual(["p A payout that fails", "ul Validate\nRetry three", "h2 Alerts", "p The channel hears."]);
    const stage = card("docs/retry.md").querySelector<HTMLElement>(".tf-rstage")!;
    const foot = () => card("docs/retry.md").querySelector(".tf-rfoot-say")!.textContent;
    // The paragraph (line 3) on both sides, the list (7), the new heading (14) and its paragraph (16): four changes.
    expect(foot()).toBe("Change 1 of 4");
    expect(stage.scrollTop, "opened at the first change").toBe(3 * 20 - 12);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "n", bubbles: true }));
    flushSync();
    expect(stage.scrollTop).toBe(7 * 20 - 12);
    expect(foot()).toBe("Change 2 of 4");
  } finally {
    Element.prototype.getBoundingClientRect = real;
  }
});

// -- An API spec (ux31, section 13) ------------------------------------------------

const specOf = (summary: string, required: string[], reordered = false) => {
  const paths = {
    "/pets": { get: { summary: "List pets", responses: { "200": { description: "ok", content: { "application/json": { schema: { type: "array", items: { $ref: "#/components/schemas/Pet" } } } } } } } },
    "/health": { get: { summary, responses: { "200": { description: "ok" } } } },
  };
  const doc = { openapi: "3.0.3", info: { title: "Pets", version: "1" }, paths, components: { schemas: { Pet: { type: "object", required, properties: { id: { type: "integer" }, name: { type: "string" } } } } } };
  return JSON.stringify(reordered ? { components: doc.components, paths: { "/health": paths["/health"], "/pets": paths["/pets"] }, info: doc.info, openapi: doc.openapi } : doc, null, 2);
};
/** A spec whose name says nothing and whose change is far from its top: `specHint` cannot tell, the sniff can. */
const SPEC = file("api/payouts.json", { hunks: [{ header: "@@ -9,1 +9,1 @@", lines: [del(9, '"summary": "Health"'), add(9, '"summary": "Is it up"')] }] });

/** A reader whose sides are read at the checkout (Local changes, a comparison ending at the working tree). */
function localReader(texts: Record<string, [old: string | null, now: string | null]>) {
  const r = reader(texts);
  return { ...r, context: { ...r.context, local: true } };
}

test("a plain-named spec changed mid-file offers Rendered once its drawn card's side is sniffed; Lines stays the default", async () => {
  const { context, asked } = localReader({ "api/payouts.json": [specOf("Health", ["id"]), specOf("Is it up", ["id", "name"])] });
  draw({ files: [SPEC], context });
  // The side is read once, to know the file for a spec; the stage takes the same read.
  await vi.waitFor(() => expect(modes("api/payouts.json")).toEqual(["Lines*", "Rendered"]));
  expect(asked).toEqual(["bytes new:api/payouts.json"]);
  expect(card("api/payouts.json").querySelector(".tf-dbody")).not.toBeNull();
});

test("Rendered draws the inline form on both sides, held to what changed and marked; n opens the first change on both", async () => {
  const { context } = localReader({ "api/payouts.json": [specOf("Health", ["id"]), specOf("Is it up", ["id", "name"])] });
  draw({ files: [SPEC], context });
  await vi.waitFor(() => expect(modes("api/payouts.json")).toContain("Rendered"));
  mode("api/payouts.json", "Rendered");
  await vi.waitFor(() => expect(card("api/payouts.json").querySelectorAll(".tf-oas-inline .tf-oas-ibtn").length).toBe(6));
  const rows = (label: string) => [...card("api/payouts.json").querySelectorAll(`.tf-oas-inline[aria-label="${label}"] .tf-oas-ibtn`)].map((b) => b.textContent?.replace(/\s+/g, " ").trim());
  expect(rows("API before")).toEqual(["GET /pets List pets Changed through Pet", "GET /health Health Changed", "Schema Pet Changed"]);
  expect(rows("API after")).toEqual(["GET /pets List pets Changed through Pet", "GET /health Is it up Changed", "Schema Pet Changed"]);
  const foot = () => card("api/payouts.json").querySelector(".tf-rfoot-say")!.textContent;
  expect(foot()).toBe("Change 1 of 3");

  window.dispatchEvent(new KeyboardEvent("keydown", { key: "n", bubbles: true }));
  flushSync();
  // The first change opens before and after at once.
  const open = () => [...card("api/payouts.json").querySelectorAll('.tf-oas-ibtn[aria-expanded="true"]')].map((b) => b.closest(".tf-oas-inline")!.getAttribute("aria-label"));
  expect(open()).toEqual(["API before", "API after"]);
  expect(foot()).toBe("Change 1 of 3");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "n", bubbles: true }));
  flushSync();
  expect(foot()).toBe("Change 2 of 3");
});

test("a spec reordered and nothing else says it is the same API on both sides", async () => {
  const { context } = localReader({ "api/payouts.json": [specOf("Health", ["id"]), specOf("Health", ["id"], true)] });
  draw({ files: [SPEC], context });
  await vi.waitFor(() => expect(modes("api/payouts.json")).toContain("Rendered"));
  mode("api/payouts.json", "Rendered");
  await vi.waitFor(() => expect(card("api/payouts.json").querySelector(".tf-rfoot-say")!.textContent).toBe("The same API on both sides"));
  expect(card("api/payouts.json").querySelectorAll(".tf-oas-ibtn")).toHaveLength(0);
});

test("a YAML file that is no spec is read once and offers no rendered mode", async () => {
  const compose = file("deploy/compose.yaml");
  // A file of another kind is never read for the sniff.
  const code = file("src/payout.rs");
  const { context, asked } = localReader({ "deploy/compose.yaml": ["services: {}", "services:\n  web: {}"], "src/payout.rs": ["fn a() {}", "fn b() {}"] });
  draw({ files: [compose, code], context });
  await vi.waitFor(() => expect(asked).toEqual(["bytes new:deploy/compose.yaml"]));
  await settle();
  expect(modes("deploy/compose.yaml")).toEqual([]);
});

test("a sniffed spec gains the switch and nothing else: the band's Rendered leaves it on Lines", async () => {
  const { context } = localReader({ "api/payouts.json": [specOf("Health", ["id"]), specOf("Is it up", ["id"])] });
  draw({ files: [SPEC], context, rendering: { on: true, set: () => {}, reset: () => {} } });
  await vi.waitFor(() => expect(modes("api/payouts.json")).toEqual(["Lines*", "Rendered"]));
});

test("where a side may be a forge's request, a plain-named spec is not read for the sniff and keeps Lines", async () => {
  const { context, asked } = reader({ "api/payouts.json": [specOf("Health", ["id"]), specOf("Is it up", ["id"])] });
  draw({ files: [SPEC], context });
  await settle();
  expect(asked).toEqual([]);
  // Read by its name alone it is a JSON file, compared by element (C1); never read for a sniff.
  expect(modes("api/payouts.json")).toEqual(["Lines*", "Rendered"]);
});

test("a spec on one side only lists everything there, the other side says it is no spec, and the foot counts them added", async () => {
  const { context } = reader({ "api/openapi.json": ['{"a": 1}', specOf("Health", ["id"])] });
  draw({ files: [file("api/openapi.json", { hunks: [{ header: "@@ -1,1 +1,1 @@", lines: [del(1, '{"a": 1}'), add(1, "{")] }] })], context });
  mode("api/openapi.json", "Rendered");
  await vi.waitFor(() => expect(card("api/openapi.json").querySelectorAll(".tf-oas-ibtn").length).toBe(3));
  expect(stage("api/openapi.json").querySelector('[data-side="old"]')?.textContent).toBe("Not an OpenAPI or Swagger spec on this side.");
  expect(card("api/openapi.json").querySelector(".tf-oas-imark")).toBeNull();
  expect(card("api/openapi.json").querySelector(".tf-rfoot-say")!.textContent).toBe("2 operations, all added");
});

/** A one-line change in a YAML or JSON file, as a forge's diff gives it. */
const changedAt = (path: string, n: number, was: string, now: string): DiffFile => ({
  path,
  old_path: null,
  change: "modified",
  additions: 1,
  deletions: 1,
  hunks: [{ header: `@@ -${n} +${n} @@`, lines: [{ kind: "del", old: n, new: null, text: was }, { kind: "add", old: null, new: n, text: now }] }],
  truncated: false,
  threads: [],
});

test("an OpenAPI spec offers Rendered where its diff or its name shows it one; a plain config file does not (2026-11-22)", () => {
  const { context } = reader({});
  draw({
    files: [
      changedAt("api/openapi.yaml", 9, "a", "b"),
      // A plain file of the same name first: what one name plans is not kept for the other.
      changedAt("old/spec.yaml", 9, "a", "b"),
      changedAt("api/spec.yaml", 1, "openapi: 3.0.0", "openapi: 3.1.0"),
      changedAt("config/app.json", 3, '"a": 1', '"a": 2'),
      changedAt("docker-compose.yml", 9, "a", "b"),
    ],
    context,
  });
  expect(modes("old/spec.yaml")).toEqual([]);
  expect(modes("api/openapi.yaml")).toEqual(["Lines*", "Rendered"]);
  expect(modes("api/spec.yaml")).toEqual(["Lines*", "Rendered"]);
  // A plain JSON file is compared by element (C1), not as an API.
  expect(modes("config/app.json")).toEqual(["Lines*", "Rendered"]);
  expect(modes("docker-compose.yml")).toEqual([]);
});

test("an OpenAPI comparison says which side is no spec, and once where neither is", async () => {
  const { context } = reader({ "api/openapi.yaml": ["title: not a spec\n", "openapi: 3.0.0\ninfo:\n  title: x\n"], "api/swagger.json": ['{"a": 1}', '{"a": 2}'] });
  draw({ files: [changedAt("api/openapi.yaml", 1, "title: not a spec", "openapi: 3.0.0"), changedAt("api/swagger.json", 1, '{"a": 1}', '{"a": 2}')], context });
  mode("api/openapi.yaml", "Rendered");
  mode("api/swagger.json", "Rendered");
  await settle();
  // The marked inline form is the stage (Fable's amendment): the side that is no spec says so in its column.
  await vi.waitFor(() => expect(stage("api/openapi.yaml").querySelector('[data-side="old"]')?.textContent).toBe("Not an OpenAPI or Swagger spec on this side."));
  expect(stage("api/openapi.yaml").querySelector('[data-side="new"]')).toBeNull();
  expect(stage("api/swagger.json").textContent?.trim()).toBe("Neither side is an OpenAPI or Swagger spec: Lines has the change.");
  expect(stage("api/swagger.json").classList.contains("tf-rstage-one")).toBe(true);
});
