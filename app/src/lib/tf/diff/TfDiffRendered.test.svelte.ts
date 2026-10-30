/**
 * Rendered diffs (#2279) in the one diff component, mounted as its callers
 * mount it: a file whose renderer draws diffs offers *Rendered* beside its
 * lines, which draws the file before and after with that renderer; an image
 * also swipes. The engines are the jsdom ones (Markdown for real, a fake
 * command port); headless Chrome witnesses the drawn panes.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { RenderContent, RenderRun, UserRenderer } from "../../ipc/app";
import type { DiffFile, DiffLine } from "../../ipc/entity";
import { installCommands, type CommandPort } from "../../render/command";
import { createDiffLayout } from "./diff-layout.svelte";
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
});

function draw(props: Record<string, unknown>) {
  views.push(mount(TfDiffView, { target, props: { layout: createDiffLayout(() => null), defaults: { list: [] }, ...props } as never }));
  flushSync();
}

const settle = async () => {
  for (let n = 0; n < 8; n += 1) await Promise.resolve();
  flushSync();
};

/** Reads each side whole, as a commit's reader does, and counts the reads. */
function reader(texts: Record<string, [old: string, now: string]>, bytes?: Record<string, [old: Uint8Array | null, now: Uint8Array | null]>) {
  const asked: string[] = [];
  return {
    asked,
    context: {
      text: (f: DiffFile, side: "old" | "new") => (asked.push(`${side}:${f.path}`), Promise.resolve(texts[f.path]?.[side === "old" ? 0 : 1] ?? null)),
      ...(bytes
        ? { bytes: (f: DiffFile, side: "old" | "new") => (asked.push(`bytes ${side}:${f.path}`), Promise.resolve(bytes[f.path]?.[side === "old" ? 0 : 1] ?? null)) }
        : {}),
    },
  };
}

/** The mode buttons in the file header of the card for `path`. */
const card = (path: string) => [...target.querySelectorAll(".tf-diff")].find((el) => el.querySelector(".tf-fh .tf-mono")?.textContent === path)!;
const modes = (path: string) => [...card(path).querySelectorAll<HTMLButtonElement>(".tf-fh .tf-seg button")].map((b) => `${b.textContent}${b.getAttribute("aria-pressed") === "true" ? "*" : ""}`);
const mode = (path: string, name: string) => [...card(path).querySelectorAll<HTMLButtonElement>(".tf-fh .tf-seg button")].find((b) => b.textContent === name)!.click();
const sides = (path: string) => [...card(path).querySelectorAll(".tf-ba > figure")].map((side) => side.querySelector("figcaption")?.textContent?.trim());

test("a changed README shows Lines first, and Rendered draws it before and after", async () => {
  const { context } = reader({ "README.md": [BEFORE, AFTER] });
  draw({ files: [README], context });
  expect(modes("README.md")).toEqual(["Rendered", "Lines*"]);
  expect(card("README.md").querySelector(".tf-dbody")).not.toBeNull();

  mode("README.md", "Rendered");
  await settle();
  expect(modes("README.md")).toEqual(["Rendered*", "Lines"]);
  expect(card("README.md").querySelector(".tf-dbody")).toBeNull();
  expect(sides("README.md")).toEqual(["before", "after"]);
  const [old, now] = [...card("README.md").querySelectorAll(".tf-ba > figure")];
  expect(old!.querySelector("h1")?.textContent).toBe("Payouts");
  expect(old!.querySelector("strong")?.textContent).toBe("three");
  expect(now!.querySelector("h1")?.textContent).toBe("Payout service");
  expect(now!.querySelector("strong")?.textContent).toBe("five");

  mode("README.md", "Lines");
  flushSync();
  expect(card("README.md").querySelector(".tf-ba")).toBeNull();
  expect(card("README.md").querySelector(".tf-dbody")).not.toBeNull();
});

test("the reads Rendered makes are the ones expanded context keeps: a second time asks nothing", async () => {
  const { asked, context } = reader({ "README.md": [BEFORE, AFTER] });
  draw({ files: [README], context });
  mode("README.md", "Rendered");
  await settle();
  mode("README.md", "Lines");
  flushSync();
  mode("README.md", "Rendered");
  await settle();
  expect(asked.sort()).toEqual(["new:README.md", "old:README.md"]);
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

test("a side that cannot be read says so, beside the side that was", async () => {
  const { context } = reader({ "README.md": [BEFORE, AFTER] });
  draw({ files: [README], context: { text: (f: DiffFile, side: "old" | "new") => (side === "old" ? Promise.reject(new Error("gone")) : context.text(f, side)) } });
  mode("README.md", "Rendered");
  await settle();
  const [old, now] = [...card("README.md").querySelectorAll(".tf-ba > figure")];
  expect(old!.textContent).toContain("could not be read");
  expect(now!.querySelector("h1")?.textContent).toBe("Payout service");
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
  expect(modes("site/home.page")).toEqual(["Rendered", "Lines*"]);

  mode("site/home.page", "Rendered");
  await settle();
  expect(ran.sort()).toEqual(["pager:new page", "pager:old page"]);
  const frames = [...card("site/home.page").querySelectorAll(".tf-ba > figure iframe")];
  expect(frames.map((f) => f.getAttribute("sandbox"))).toEqual(["", ""]);
  expect(frames.map((f) => f.getAttribute("srcdoc")?.includes("<p>old page</p>"))).toEqual([true, false]);
});

test("a changed PNG opens rendered, side by side from each side's bytes, and swipes", async () => {
  const { asked, context } = reader({}, { "assets/logo.png": [pngBytes(1), pngBytes(2)] });
  draw({ files: [PNG], context });
  expect(modes("assets/logo.png")).toEqual(["Rendered*", "Swipe", "Bytes"]);
  await settle();
  expect(sides("assets/logo.png")).toEqual(["before", "after"]);
  const srcs = [...card("assets/logo.png").querySelectorAll<HTMLImageElement>(".tf-ba img")].map((img) => img.getAttribute("src"));
  expect(srcs).toHaveLength(2);
  expect(srcs.map((src) => blobs.made.get(src!)?.type)).toEqual(["image/png", "image/png"]);
  expect(new Uint8Array(await blobs.made.get(srcs[0]!)!.arrayBuffer()).at(-1)).toBe(1);
  expect(new Uint8Array(await blobs.made.get(srcs[1]!)!.arrayBuffer()).at(-1)).toBe(2);

  mode("assets/logo.png", "Swipe");
  await settle();
  const swipe = card("assets/logo.png").querySelector(".tf-swipe")!;
  expect([...swipe.querySelectorAll("img")].map((img) => img.getAttribute("src"))).toEqual(srcs);
  const range = swipe.querySelector<HTMLInputElement>("input[type=range]")!;
  const over = swipe.querySelector<HTMLElement>(".tf-swipe-after")!;
  expect(over.style.getPropertyValue("clip-path")).toBe("inset(0 0 0 50%)");
  range.value = "20";
  range.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(over.style.getPropertyValue("clip-path")).toBe("inset(0 0 0 20%)");
  expect(asked.sort()).toEqual(["bytes new:assets/logo.png", "bytes old:assets/logo.png"]);

  mode("assets/logo.png", "Bytes");
  flushSync();
  expect(card("assets/logo.png").textContent).toContain("A binary file: no lines to draw.");
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

test("a PNG with no side before it (added) shows its after side only and does not swipe", async () => {
  const { context } = reader({}, { "assets/logo.png": [null, pngBytes(2)] });
  draw({ files: [PNG], context });
  await settle();
  expect(sides("assets/logo.png")).toEqual(["after"]);
  expect(modes("assets/logo.png")).toEqual(["Rendered*", "Bytes"]);
});

test("a PNG where no bytes reader is offered stays a binary file", () => {
  const { context } = reader({});
  draw({ files: [PNG], context });
  expect(modes("assets/logo.png")).toEqual([]);
  expect(card("assets/logo.png").textContent).toContain("A binary file: no lines to draw.");
});
