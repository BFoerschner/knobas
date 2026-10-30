/**
 * The default renderer per file pattern (#2263, spec #2234 stories 42–43):
 * one global list of `{ pattern, renderer }` the person keeps in Settings ›
 * Renderers and adds to from the file viewer's switcher (*Use … for `*.ext`
 * by default*). It is stored backend-side (`render.defaults`, `renderers.rs`)
 * so it holds across a restart; `defaults.svelte.ts` is the app's copy.
 *
 * - **Which pattern decides**: of the patterns that take the file, the most
 *   specific ({@link specificity}); a tie goes to the one listed first.
 * - **A mapping names a renderer by registry id** (`source`, `markdown`,
 *   `command:<saved id>`). One the registry no longer holds (a deleted
 *   command renderer, a built-in not loaded) is passed over, so the file
 *   falls back to the next pattern, then to the registry's own order: the
 *   built-in default. The row stays, so a renderer saved again under the
 *   same id takes its files back.
 * - **A mapping also makes its renderer take the pattern's files**: mapping
 *   `*.txt` to Markdown draws `.txt` files as Markdown, though Markdown's own
 *   patterns don't name them. A binary file still only goes to a renderer that
 *   reads bytes.
 */
import type { RendererDefault } from "../ipc/app";
import { accepts, type Renderer } from "./registry";

export type { RendererDefault };

export interface Opened {
  /** Every renderer that takes the file, in the registry's order: what the switcher lists. */
  offered: Renderer[];
  /** What the file opens with: a mapping's renderer, else the first offered. */
  renderer: Renderer | null;
  /** The pattern that chose `renderer`; `null` where no mapping did. */
  pattern: string | null;
}

/**
 * How specific a pattern is: its literal characters, so `CHANGELOG.md` and
 * `docs/*.md` beat `*.md`, which beats `*`. Fewer wildcards break a tie.
 */
export function specificity(pattern: string): number {
  const wild = (pattern.match(/[*?]/g) ?? []).length;
  return (pattern.length - wild) * 16 - wild;
}

/** What `file` opens with under `defaults`, among `all` (the registry, in order). */
export function openWith(
  file: { path: string; binary: boolean },
  all: readonly Renderer[],
  defaults: readonly RendererDefault[],
): Opened {
  const fits = (r: Renderer) => !file.binary || r.input === "bytes";
  const matching = defaults
    .map((d, at) => ({ d, at }))
    .filter(({ d }) => accepts(d.pattern, file.path))
    .sort((a, b) => specificity(b.d.pattern) - specificity(a.d.pattern) || a.at - b.at)
    .map(({ d }) => d);
  const offered = all.filter(
    (r) => fits(r) && (r.patterns.some((p) => accepts(p, file.path)) || matching.some((d) => d.renderer === r.id)),
  );
  for (const d of matching) {
    const renderer = offered.find((r) => r.id === d.renderer);
    if (renderer) return { offered, renderer, pattern: d.pattern };
  }
  return { offered, renderer: offered[0] ?? null, pattern: null };
}

/** The pattern *Make default* writes for a file: `*.<extension>`, lowercase, or the file's whole name without one. */
export function defaultPattern(path: string): string {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? `*${name.slice(dot).toLowerCase()}` : name;
}

/**
 * The pattern *Make default* writes for a file `opened` under the defaults:
 * the one that chose its current default (so a `docs/*.md` row is changed,
 * not shadowed by a `*.md` it outranks), unless that pattern is broader than
 * the file's own {@link defaultPattern}. A `*` row deciding a README must not
 * become "Markdown for every file": the switcher's offer is per `*.ext`.
 */
export function makeDefaultPattern(opened: Opened, path: string): string {
  const own = defaultPattern(path);
  if (opened.pattern === null || specificity(opened.pattern) < specificity(own)) return own;
  return opened.pattern;
}

/** `list` with `pattern` mapped to `renderer`: the existing row (same pattern, any case) changed in place, else one more at the end. */
export function withDefault(list: readonly RendererDefault[], pattern: string, renderer: string): RendererDefault[] {
  const key = pattern.toLowerCase();
  const at = list.findIndex((d) => d.pattern.toLowerCase() === key);
  if (at === -1) return [...list, { pattern, renderer }];
  return list.map((d, i) => (i === at ? { pattern: d.pattern, renderer } : d));
}
