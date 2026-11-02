/**
 * The Markdown component's engine (#2255, spec #2234 step 5): Markdown text
 * in, a sanitized DOM fragment out, built on markdown-it and DOMPurify.
 * `tf/markdown/TfMarkdown.svelte` draws it; every surface that draws Markdown
 * goes through it (#2261).
 *
 * - **GitHub-flavoured**: tables and strikethrough are markdown-it's own,
 *   autolinks its `linkify`, task lists the `tf-tasks` rule below.
 * - **Raw HTML** is parsed, then everything goes through DOMPurify with
 *   {@link SANITIZE}: no script, no handler, no `javascript:` URL, no form,
 *   frame, style or class of the author's. The classes the fragment carries
 *   are set after the sanitizer, from markers only this module writes and the
 *   sanitizer lets through (`data-md-fence`, `data-md-task`, `data-md-attachment`); an author who
 *   writes a marker by hand gets a fence or a task box, nothing more.
 * - **Links** are classified by {@link resolveLink}: repository-relative ones
 *   resolve against the file's directory and take the href the surface gives
 *   (`fileLink`), or draw as text where it gives none; `http(s)` ones are
 *   `tf-ext`, which the component hands to the opener; `#anchor` ones scroll
 *   to their heading (ids are `user-content-<slug>`, as GitHub's). Any other
 *   scheme loses its href.
 * - **Images** lose their `src` unless it is a `data:` URL (the CSP refuses
 *   remote ones); a relative one keeps its resolved path in `data-md-path`
 *   for the byte read (#2256), and an `attachment:<id>` one (the Jira wiki's
 *   `!name!`, #2406) keeps its id in `data-md-attachment` for the surface's
 *   attachment read. No element but `<a>` keeps an `href` and none
 *   but `<img>` a `src` (`<area>`, `<video>`, `<audio>`, `<source>`, `<track>`).
 * - **Fences** are `.tf-fence` blocks. {@link drawFences} hands each to the
 *   renderer the registry names for its language (`renderersForFence`) that
 *   has a fence drawer ({@link registerFence}, #2258's Mermaid), else
 *   highlights it through the renderer port.
 */
import DOMPurify, { type Config } from "dompurify";
import MarkdownIt from "markdown-it";

import { drawDiagram } from "./diagram";
import type { LineTokens, RenderPort } from "./port";
import { D2, GRAPHVIZ, MERMAID, renderersForFence } from "./registry";

/**
 * A surface's ticket attachments, for the text's `attachment:<id>` images
 * (#2406): `load` answers the image as a `data:image/…` URL or refuses, and
 * `missing` says what a refused one draws instead, by its name.
 */
export interface AttachmentImages {
  /**
   * Where the surface fetches an image only once it is in view (#2405): asked
   * for each image before `load`, and answers how to stop watching. Without
   * it, `load` asks at once.
   */
  watch?(node: Element, id: string): () => void;
  load(id: string): Promise<string>;
  missing(name: string, error: unknown): { text: string; link: { href: string; label: string } | null };
}

/** Where a link in the Markdown goes. */
export type MarkdownLink =
  | { kind: "file"; path: string; line: number | null }
  | { kind: "anchor"; id: string }
  | { kind: "external"; url: string }
  | { kind: "none" };

export interface MarkdownLinks {
  /** The repository directory relative links resolve against, `""` the top. */
  base: string;
  /** The href that opens a repository file (at a line), or `null` where the surface has no repository. */
  fileLink: ((path: string, line: number | null) => string) | null;
}

const SCHEME = /^[a-z][a-z0-9+.-]*:/i;

/** Where `href`, written in a Markdown file in `base`, goes. */
export function resolveLink(href: string, base: string): MarkdownLink {
  const raw = href.trim();
  if (raw.startsWith("#")) return raw.length > 1 ? { kind: "anchor", id: slug(decode(raw.slice(1))) } : { kind: "none" };
  if (SCHEME.test(raw)) {
    let url: URL;
    try {
      url = new URL(raw);
    } catch {
      return { kind: "none" };
    }
    return url.protocol === "http:" || url.protocol === "https:" ? { kind: "external", url: raw } : { kind: "none" };
  }
  if (raw.startsWith("//") || raw === "") return { kind: "none" };
  const hashAt = raw.indexOf("#");
  const fragment = hashAt === -1 ? "" : raw.slice(hashAt + 1);
  let rest = hashAt === -1 ? raw : raw.slice(0, hashAt);
  const queryAt = rest.indexOf("?");
  if (queryAt !== -1) rest = rest.slice(0, queryAt);
  if (rest === "") return { kind: "none" };
  const parts = rest.startsWith("/") ? [] : base.split("/").filter((part) => part !== "");
  for (const part of rest.split("/").map(decode)) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  if (parts.length === 0) return { kind: "none" };
  const line = /^L(\d+)/.exec(fragment);
  return { kind: "file", path: parts.join("/"), line: line ? Number(line[1]) : null };
}

function decode(part: string): string {
  try {
    return decodeURIComponent(part);
  } catch {
    return part;
  }
}

/** A heading's anchor as GitHub writes it: lowercase, punctuation dropped, spaces as `-`. */
export function slug(heading: string): string {
  return heading
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, "")
    .replace(/\s/g, "-");
}

const engine = new MarkdownIt({ html: true, linkify: true, typographer: false });

const escape = engine.utils.escapeHtml;
const fence = (lang: string, code: string) =>
  `<pre data-md-fence="${escape(lang)}"><code>${escape(code.replace(/\n$/, ""))}</code></pre>\n`;
engine.renderer.rules.fence = (tokens, idx) => fence(tokens[idx]!.info.trim().split(/\s+/)[0] ?? "", tokens[idx]!.content);
engine.renderer.rules.code_block = (tokens, idx) => fence("", tokens[idx]!.content);
// `~~struck~~` is `<del>`, as GitHub draws it.
engine.renderer.rules.s_open = () => "<del>";
engine.renderer.rules.s_close = () => "</del>";

// An `attachment:<id>` image (#2406) carries its id as a marker: the sanitizer drops a src of a scheme it does not know.
const image = engine.renderer.rules.image!;
engine.renderer.rules.image = (tokens, idx, options, env, self) => {
  const token = tokens[idx]!;
  const attachment = /^attachment:(.+)$/i.exec(String(token.attrGet("src") ?? "").trim());
  if (attachment) {
    token.attrSet("src", "");
    token.attrSet("data-md-attachment", decode(attachment[1]!));
  }
  return image(tokens, idx, options, env, self);
};

// Task lists: a list item whose text starts `[ ] ` or `[x] ` is marked, and the box drawn after the sanitizer.
engine.core.ruler.push("tf-tasks", (state) => {
  const tokens = state.tokens;
  for (let i = 2; i < tokens.length; i += 1) {
    const inline = tokens[i]!;
    if (inline.type !== "inline" || tokens[i - 1]!.type !== "paragraph_open" || tokens[i - 2]!.type !== "list_item_open") continue;
    const box = /^\[([ xX])\][ \t]+/.exec(inline.content);
    const first = inline.children?.[0];
    if (box === null || first?.type !== "text" || !first.content.startsWith(box[0])) continue;
    tokens[i - 2]!.attrSet("data-md-task", box[1] === " " ? "open" : "done");
    first.content = first.content.slice(box[0].length);
  }
  return true;
});

/** What raw HTML may keep (#2255; spec story 22). */
export const SANITIZE: Config = {
  USE_PROFILES: { html: true },
  FORBID_TAGS: [
    "style", "link", "meta", "base", "form", "input", "button", "textarea", "select", "option",
    "iframe", "frame", "frameset", "object", "embed", "dialog", "template",
  ],
  FORBID_ATTR: [
    "style", "class", "id", "name", "target", "form", "formaction", "srcset", "ping",
    "poster", "background", "usemap", "download",
  ],
  ALLOW_DATA_ATTR: false,
  ADD_ATTR: ["data-md-fence", "data-md-task", "data-md-attachment"],
};

/** `text` drawn as Markdown: sanitized, its links, fences and task boxes in place. */
export function markdownFragment(text: string, links: MarkdownLinks): DocumentFragment {
  const html = engine.render(text);
  const fragment = DOMPurify.sanitize(html, { ...SANITIZE, RETURN_DOM_FRAGMENT: true });
  finish(fragment, links);
  return fragment;
}

function finish(root: DocumentFragment, links: MarkdownLinks) {
  const doc = root.ownerDocument;
  const seen = new Map<string, number>();
  for (const heading of root.querySelectorAll("h1, h2, h3, h4, h5, h6")) {
    const base = slug(heading.textContent);
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    heading.id = `user-content-${count === 0 ? base : `${base}-${count}`}`;
  }

  for (const anchor of root.querySelectorAll("a")) {
    const href = anchor.getAttribute("href");
    const to = href === null ? ({ kind: "none" } as const) : resolveLink(href, links.base);
    anchor.removeAttribute("href");
    if (to.kind === "external") {
      anchor.setAttribute("href", to.url);
      anchor.className = "tf-ext";
      anchor.rel = "noreferrer";
    } else if (to.kind === "anchor") {
      anchor.setAttribute("href", `#${to.id}`);
      anchor.dataset["mdAnchor"] = to.id;
    } else if (to.kind === "file" && links.fileLink !== null) {
      anchor.setAttribute("href", links.fileLink(to.path, to.line));
    }
  }

  // Only `<a>` links and only `<img>` loads: `<area href>` would navigate the webview, a media `src` would fetch.
  for (const other of root.querySelectorAll("[href]:not(a)")) other.removeAttribute("href");
  for (const other of root.querySelectorAll("[src]:not(img)")) other.removeAttribute("src");

  for (const image of root.querySelectorAll("img")) {
    const src = image.getAttribute("src") ?? "";
    if (/^data:image\//i.test(src)) continue;
    image.removeAttribute("src");
    if (image.dataset["mdAttachment"] !== undefined) {
      image.classList.add("tf-md-att");
      continue;
    }
    const to = resolveLink(src, links.base);
    if (to.kind === "file") image.dataset["mdPath"] = to.path;
  }

  for (const item of root.querySelectorAll<HTMLElement>("li[data-md-task]")) {
    const done = item.dataset["mdTask"] === "done";
    const box = doc.createElement("span");
    box.className = done ? "tf-box tf-on" : "tf-box";
    box.setAttribute("role", "img");
    box.setAttribute("aria-label", done ? "Done" : "Not done");
    item.classList.add("tf-task");
    item.prepend(box);
  }

  for (const pre of root.querySelectorAll<HTMLElement>("pre[data-md-fence]")) {
    const lang = pre.dataset["mdFence"] ?? "";
    const block = doc.createElement("div");
    block.className = "tf-fence";
    block.dataset["lang"] = lang;
    const header = doc.createElement("header");
    const label = doc.createElement("span");
    label.textContent = lang === "" ? "text" : lang;
    const copy = doc.createElement("button");
    copy.type = "button";
    copy.className = "tf-btn tf-sm tf-ghost tf-fence-copy";
    copy.textContent = "Copy";
    header.append(label, copy);
    const body = doc.createElement("div");
    body.className = "tf-fence-body";
    pre.replaceWith(block);
    body.append(pre);
    block.append(header, body);
  }
}

/**
 * Draws a fence's code into `into` (its `.tf-fence-body`, holding the plain
 * `<pre>` until then); returns how to stop. The code is the fence's text as
 * written: a drawer that makes HTML or SVG of it sanitizes what it inserts.
 */
export type FenceDraw = (code: string, into: HTMLElement, port: RenderPort) => (() => void) | void;

const fenceDraws = new Map<string, FenceDraw>();

/** Gives renderer `id` a drawer for the Markdown fences it accepts; returns the undo (tests). */
export function registerFence(id: string, draw: FenceDraw): () => void {
  if (fenceDraws.has(id)) throw new Error(`fence drawer ${id} is registered already`);
  fenceDraws.set(id, draw);
  return () => {
    if (fenceDraws.get(id) === draw) fenceDraws.delete(id);
  };
}

/** The fence's code as written. */
export function fenceCode(fence: Element): string {
  return fence.querySelector("pre code")?.textContent ?? "";
}

/**
 * Draws every `.tf-fence` under `root`: by the first renderer for its language
 * with a drawer, else highlighted (a fence with no language stays plain).
 * Returns how to stop every drawing still going.
 */
export function drawFences(root: ParentNode, port: RenderPort): () => void {
  const stops: (() => void)[] = [];
  for (const block of root.querySelectorAll<HTMLElement>(".tf-fence")) {
    const lang = block.dataset["lang"] ?? "";
    const code = fenceCode(block);
    const body = block.querySelector<HTMLElement>(".tf-fence-body");
    if (lang === "" || body === null) continue;
    const draw = renderersForFence(lang)
      .map((renderer) => fenceDraws.get(renderer.id))
      .find((found) => found !== undefined);
    if (draw !== undefined) {
      const stop = draw(code, body, port);
      if (stop) stops.push(stop);
      continue;
    }
    const into = body.querySelector("code");
    if (into !== null) stops.push(highlightInto(into, code, lang, port));
  }
  return () => stops.forEach((stop) => stop());
}

/** Colours `code`'s lines into `into` as the highlighter answers; plain until then, and for no language. */
function highlightInto(into: Element, code: string, lang: string, port: RenderPort): () => void {
  const lines = code.split("\n");
  const tokens: (LineTokens | undefined)[] = [];
  const job = port.highlight(
    { text: code, lang },
    {
      onLines(from, got) {
        got.forEach((line, i) => (tokens[from + i] = line));
        const doc = into.ownerDocument;
        const nodes: Node[] = [];
        lines.forEach((text, i) => {
          if (i > 0) nodes.push(doc.createTextNode("\n"));
          const line = tokens[i];
          if (line === undefined) return void nodes.push(doc.createTextNode(text));
          for (const [part, kind] of line) {
            if (kind === "") nodes.push(doc.createTextNode(part));
            else {
              const span = doc.createElement("span");
              span.className = `tf-hl-${kind}`;
              span.textContent = part;
              nodes.push(span);
            }
          }
        });
        into.replaceChildren(...nodes);
      },
    },
  );
  return () => job.cancel();
}

// Mermaid's fences (#2258): the built-in diagram drawer, through the port's lazily loaded engine.
registerFence(MERMAID.id, (code, into, port) => drawDiagram("mermaid", code, into, port));
registerFence(D2.id, (code, into, port) => drawDiagram("d2", code, into, port));
registerFence(GRAPHVIZ.id, (code, into, port) => drawDiagram("graphviz", code, into, port));
