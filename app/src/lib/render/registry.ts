/**
 * The renderer registry (spec #2234 step 5, #2254): the one way any view
 * turns content into something drawn. A renderer declares what the spec's
 * table names, and nothing else:
 *
 * | Field      | Meaning                                                      |
 * |------------|--------------------------------------------------------------|
 * | `name`     | what the switcher shows                                      |
 * | `patterns` | accepted file patterns: where it applies in the file viewer  |
 * | `fences`   | accepted fence languages: where it applies inside Markdown   |
 * | `input`    | text or bytes                                                |
 * | `output`   | SVG, HTML or image; `lines` for *Source*, see below          |
 * | `diff`     | whether it can render a diff (if not: before and after side by side) |
 *
 * `id` is the registry's key, what a default per pattern (#2263) stores.
 * *Source* outputs `lines`: highlighted lines the viewer draws itself from
 * the worker's tokens, never markup from outside, so nothing needs the
 * sanitizer or the sandboxed frame that HTML and SVG output go through.
 *
 * The engines behind a renderer (Shiki now; Mermaid, D2, Graphviz, pdf.js
 * later) sit behind the one renderer port (`port.ts`), so jsdom tests use a
 * fake. How to add one: `notes/renderer-registry.md` on the spec's branch,
 * and `register` below.
 */

export type RendererInput = "text" | "bytes";
export type RendererOutput = "svg" | "html" | "image" | "lines";

export interface Renderer {
  readonly id: string;
  readonly name: string;
  /** Globs on the file's name (`*.png`), or on its path when they hold a `/`; `*` takes every file. */
  readonly patterns: readonly string[];
  /** Markdown fence languages, lowercase (`mermaid`); `*` takes every fence. */
  readonly fences: readonly string[];
  readonly input: RendererInput;
  readonly output: RendererOutput;
  readonly diff: boolean;
}

/** Image: drawn from the file's bytes as an `<img>`, which runs no script even for SVG. */
export const IMAGE: Renderer = {
  id: "image",
  name: "Image",
  patterns: ["*.png", "*.jpg", "*.jpeg", "*.gif", "*.webp", "*.avif", "*.bmp", "*.ico", "*.svg"],
  fences: [],
  input: "bytes",
  output: "image",
  diff: false,
};

/**
 * Markdown (#2255): drawn by the Markdown component (`tf/markdown/TfMarkdown.svelte`),
 * its HTML through the sanitizer. A diff of it is drawn before and after (spec story 71).
 */
export const MARKDOWN: Renderer = {
  id: "markdown",
  name: "Markdown",
  patterns: ["*.md", "*.markdown", "*.mdown", "*.mkd"],
  fences: [],
  input: "text",
  output: "html",
  diff: true,
};

/** Source: the text, highlighted. Takes every text file and every fence, so it is always the last resort. */
export const SOURCE: Renderer = {
  id: "source",
  name: "Source",
  patterns: ["*"],
  fences: ["*"],
  input: "text",
  output: "lines",
  diff: true,
};

/** Built-ins first, in the order that makes the first that accepts the default; Source last. */
const renderers: Renderer[] = [IMAGE, MARKDOWN, SOURCE];

/**
 * Adds a renderer ahead of Source, so it becomes the default for what it
 * accepts until a per-pattern default (#2263) says otherwise. Returns the
 * registry to what it was (tests).
 */
export function register(renderer: Renderer): () => void {
  if (renderers.some((r) => r.id === renderer.id)) throw new Error(`renderer ${renderer.id} is registered already`);
  renderers.splice(renderers.indexOf(SOURCE), 0, renderer);
  return () => {
    const at = renderers.indexOf(renderer);
    if (at !== -1) renderers.splice(at, 1);
  };
}

function glob(pattern: string): RegExp {
  const body = pattern.replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".");
  return new RegExp(`^${body}$`, "i");
}

/** Whether `pattern` takes the file at `path`. */
export function accepts(pattern: string, path: string): boolean {
  const target = pattern.includes("/") ? path : path.slice(path.lastIndexOf("/") + 1);
  return glob(pattern).test(target);
}

/**
 * Every renderer that accepts the file, the default first. A binary file
 * (no text) is accepted only by renderers that read bytes.
 */
export function renderersFor(file: { path: string; binary: boolean }): Renderer[] {
  return renderers.filter(
    (r) => (!file.binary || r.input === "bytes") && r.patterns.some((pattern) => accepts(pattern, file.path)),
  );
}

/** Every renderer that accepts a Markdown fence of `lang`, the default first. */
export function renderersForFence(lang: string): Renderer[] {
  const wanted = lang.trim().toLowerCase();
  return renderers.filter((r) => r.fences.includes(wanted) || r.fences.includes("*"));
}

/** A data URL for an image's bytes (CSP takes `data:` images, not `blob:`). */
export function imageUrl(bytes: Uint8Array, path: string): string {
  const ext = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
  const type = ext === "svg" ? "image/svg+xml" : ext === "jpg" ? "image/jpeg" : ext === "ico" ? "image/x-icon" : `image/${ext}`;
  let binary = "";
  for (let at = 0; at < bytes.length; at += 0x8000) binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000));
  return `data:${type};base64,${btoa(binary)}`;
}
