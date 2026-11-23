/**
 * **A sidebar's width is the reader's** (the sidebar-resize pass, PR #2479;
 * Björn: "everywhere where I have a sidebar it to be possible to move it
 * side-to-side dragging it with my mouse"). Every column beside a page's main
 * content has one edge (`TfSideEdge.svelte`) that drags it, steps it from the
 * keyboard and returns it to its default on a double click, and the width is
 * kept per sidebar in the webview's `localStorage` (`knobas.side.<id>`), read
 * when the shell mounts, as the chosen job and the diff layout are.
 *
 * A sidebar nobody has resized holds no width: its page's CSS draws the
 * default, breakpoints and all, so a page at its defaults draws as it did. A
 * resized one is drawn through one custom property on its grid,
 * `--side-<id>`, which the grid's `grid-template-columns` reads with the
 * default as its fallback. The value clamps in CSS too, between the floor and
 * the sidebar's share of the grid, so a window made narrower after a drag
 * never squeezes the main column away.
 *
 * Each property is registered as not inherited (`side-edge.css`): a drag sets
 * it every frame, and an inherited one would restyle every row of the page
 * under it each time.
 */
import { keptValue, windowStorage, type KeptStorage, type KeptValue } from "./kept-value";

/** Which edge of the main content the sidebar stands at: `start` is a left rail, `end` a right column. */
export type SideAt = "start" | "end";

export interface SideSpec {
  /** What the edge names, as the separator's label: *Resize the pipelines*. */
  readonly label: string;
  readonly at: SideAt;
  /** The CSS default at the design width, for the separator's value until a measure reads the drawn width. */
  readonly base: number;
  /** The floor, in pixels. */
  readonly min: number;
  /** The most of its grid the sidebar may take, a fraction of the grid's width. */
  readonly share: number;
  /** A cap in pixels as well, where a width beyond it means nothing (the ticket card, #2402). */
  readonly max?: number;
  /** What the rest of the grid keeps whatever the width, in pixels: the outline leaves Docs' page its reading column. */
  readonly keep?: number;
  /** Where the width is kept, when it is not `knobas.side.<id>` (the ticket card's, kept since #2402). */
  readonly key?: string;
}

/** Every sidebar in the app. A sidebar shown in several places (a diff's file list) is one entry: it is one width. */
export const SIDES = {
  "tickets-views": { label: "Resize the views", at: "start", base: 230, min: 160, share: 0.3 },
  "ticket-card": { label: "Resize the card", at: "start", base: 320, min: 240, max: 720, share: 0.6, key: "knobas.ticket.side" },
  builds: { label: "Resize the pipelines", at: "start", base: 300, min: 180, share: 0.45 },
  "repo-tree": { label: "Resize the file tree", at: "start", base: 280, min: 160, share: 0.5 },
  "repo-builds": { label: "Resize the branches", at: "start", base: 300, min: 180, share: 0.45 },
  compare: { label: "Resize the side panel", at: "end", base: 300, min: 220, share: 0.45 },
  "diff-files": { label: "Resize the changed files", at: "start", base: 240, min: 160, share: 0.45 },
  "docs-tree": { label: "Resize the page tree", at: "start", base: 300, min: 180, share: 0.45 },
  "docs-outline": { label: "Resize the outline", at: "end", base: 268, min: 180, share: 0.5, keep: 660 + 56 },
  settings: { label: "Resize the sections", at: "start", base: 230, min: 160, share: 0.35 },
  pr: { label: "Resize the side panel", at: "end", base: 300, min: 220, share: 0.45 },
  set: { label: "Resize the side panel", at: "end", base: 300, min: 220, share: 0.45 },
  assets: { label: "Resize the asset pane", at: "end", base: 340, min: 240, share: 0.5 },
  monitor: { label: "Resize the monitor", at: "end", base: 360, min: 300, share: 0.45 },
  "oas-rail": { label: "Resize the endpoints", at: "start", base: 300, min: 240, max: 520, share: 0.4 },
  "doc-rail": { label: "Resize the document's rail", at: "start", base: 300, min: 240, max: 520, share: 0.4 },
} as const satisfies Record<string, SideSpec>;

export type SideId = keyof typeof SIDES;

/**
 * Widths kept for sidebars that are gone, cleared at the next load: Tickets' preview of the selected ticket
 * (`knobas.side.tickets-preview`), taken out in ux18 (Björn: "Remove the right sidebar from the tickets view").
 */
const GONE_KEYS = ["knobas.side.tickets-preview"];

export const SIDE_IDS = Object.keys(SIDES) as SideId[];

/** The arrows' step, and with Shift held, the larger one. */
export const SIDE_STEP = 16;
export const SIDE_STEP_BIG = 64;

export function specOf(id: SideId): SideSpec {
  return SIDES[id];
}

/** The custom property a sidebar's grid reads its width from. */
export function sideProperty(id: SideId): string {
  return `--side-${id}`;
}

export function sideKey(id: SideId): string {
  return specOf(id).key ?? `knobas.side.${id}`;
}

/**
 * The most `id` may take of a grid `grid` pixels wide: its share, under its
 * cap, never under its floor. A grid not laid out (0) has no share to give,
 * so only the cap holds.
 */
export function maxWidth(id: SideId, grid: number): number {
  const spec = specOf(id);
  const share = grid > 0 ? Math.min(spec.share * grid, grid - (spec.keep ?? 0)) : Number.POSITIVE_INFINITY;
  return Math.max(spec.min, Math.min(spec.max ?? Number.POSITIVE_INFINITY, share));
}

/** `px` in whole pixels, within `id`'s bounds in a grid `grid` pixels wide; with no bound to hold an endless width, the default. */
export function clampWidth(id: SideId, px: number, grid = 0): number {
  const width = Math.min(maxWidth(id, grid), Math.max(specOf(id).min, px));
  return Number.isFinite(width) ? Math.round(width) : specOf(id).base;
}

/** What the grid's custom property holds for a width: clamped in CSS as well, to the floor and the share. */
export function cssWidth(id: SideId, px: number): string {
  const spec = specOf(id);
  const share = `${Math.round(spec.share * 1000) / 10}%`;
  return `clamp(${spec.min}px, ${px}px, ${spec.keep === undefined ? share : `min(${share}, 100% - ${spec.keep}px)`})`;
}

export interface SideWidths {
  /** The kept width of `id`, or `null` while it is at its default. */
  px(id: SideId): number | null;
  /** Set `id` to `px`, clamped, and keep it. */
  set(id: SideId, px: number, grid?: number): void;
  /** Back to the default: nothing kept, the page's CSS draws it. */
  reset(id: SideId): void;
  /** Read every kept width again: the shell's mount, as a restart does. */
  load(): void;
  /** Tests only: every sidebar at its default, nothing kept. */
  resetAll(): void;
}

export function createSideWidths(storage: () => KeptStorage | null): SideWidths {
  const kept = new Map<SideId, KeptValue>(SIDE_IDS.map((id) => [id, keptValue(storage, sideKey(id))]));
  const gone = GONE_KEYS.map((key) => keptValue(storage, key));
  const read = (id: SideId): number | null => {
    const raw = kept.get(id)!.read();
    if (raw === null) return null;
    const px = Number.parseFloat(raw);
    return Number.isFinite(px) ? clampWidth(id, px) : null;
  };
  let widths = $state<Partial<Record<SideId, number>>>({});
  return {
    px(id) {
      return widths[id] ?? null;
    },
    set(id, px, grid = 0) {
      const next = clampWidth(id, px, grid);
      if (widths[id] !== next) widths = { ...widths, [id]: next };
      kept.get(id)!.write(String(next));
    },
    reset(id) {
      if (id in widths) {
        const { [id]: _gone, ...rest } = widths;
        widths = rest;
      }
      kept.get(id)!.clear();
    },
    load() {
      for (const value of gone) value.clear();
      const next: Partial<Record<SideId, number>> = {};
      for (const id of SIDE_IDS) {
        const px = read(id);
        if (px !== null) next[id] = px;
      }
      widths = next;
    },
    resetAll() {
      widths = {};
      for (const value of kept.values()) value.clear();
    },
  };
}

/** The window's sidebar widths, over this viewer's `localStorage` where there is one. */
export const sideWidths: SideWidths = createSideWidths(windowStorage);

/**
 * On a sidebar's grid: its kept width as `--side-<id>`, or none, so the CSS
 * default draws. Re-runs as the kept width changes, which is once a drag
 * settles; a drag in progress writes the property itself (`TfSideEdge`).
 */
export function sideWidth(id: SideId) {
  return (node: HTMLElement) => {
    const px = sideWidths.px(id);
    if (px === null) node.style.removeProperty(sideProperty(id));
    else node.style.setProperty(sideProperty(id), cssWidth(id, px));
  };
}
