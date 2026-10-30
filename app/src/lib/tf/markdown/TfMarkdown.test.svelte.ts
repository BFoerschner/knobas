/**
 * The Markdown component (#2255, spec #2234 step 5) mounted the way a page
 * mounts it: text in, a drawn and sanitized tree out, links that route inside
 * the app or go to the opener, and fences handed to the renderer registry.
 * The highlighter is a fake port; the sanitizer is the real DOMPurify on jsdom
 * (headless Chrome witnesses it again against the dev server).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { HighlightHandlers, HighlightRequest, RenderPort } from "../../render/port";
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
    [{ id: "cat", name: "Cat", command: "cat", patterns: [], fences: ["cat"], input: "text", output: "html", timeout_secs: 10 }],
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
