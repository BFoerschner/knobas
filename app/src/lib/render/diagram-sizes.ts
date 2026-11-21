/**
 * The size each diagram drew at, kept between sessions (#2510, `docs/decisions/2026-11-20-a-diagram-keeps-its-room.md`),
 * so a diagram seen before stands in a room of that size from the frame its text draws in, and nothing under it moves
 * when its drawing lands.
 *
 * A size is kept by everything the drawing's size comes from: the engine and its version, the text scale the engine
 * lays its labels out at, and the source. A diagram is drawn at its own size whatever the column's width (a wider one
 * scrolls its holder), so the text scale is the only width the drawing scales by. The theme is not in the key: every
 * engine is handed the theme as colours alone, so a diagram seen in the light theme has its room in the dark one. The
 * source is kept as a digest: the store holds the latest {@link KEPT} sizes in the webview's `localStorage`
 * (`knobas.diagram-sizes`).
 *
 * A size that turns out wrong (two sources of one digest, a font the machine has since lost) is set right when the
 * drawing lands, and the drawing is fitted into the room it was given (`diagram.ts`).
 */
import { windowStorage, type KeptStorage } from "../tf/kept-value";
import type { DiagramRequest } from "./port";

/** A drawing's size in CSS pixels. */
export interface DiagramSize {
  w: number;
  h: number;
}

/**
 * The version of each engine the app is built with (`app/package.json`): a new engine may lay the same source out at
 * another size. `diagram-sizes.test.ts` fails when one is not the installed package's.
 */
export const ENGINE_VERSIONS: Record<DiagramRequest["engine"], string> = { mermaid: "11.17.2", d2: "0.1.33", graphviz: "3.31.0" };

/** How many diagrams are remembered; the oldest go first. */
const KEPT = 400;
const STORE_KEY = "knobas.diagram-sizes";

let sizes: Map<string, DiagramSize> | null = null;
let saving: ReturnType<typeof setTimeout> | null = null;

function store(): KeptStorage | null {
  try {
    return windowStorage();
  } catch {
    return null;
  }
}

function kept(): Map<string, DiagramSize> {
  if (sizes !== null) return sizes;
  sizes = new Map();
  try {
    const stored = JSON.parse(store()?.getItem(STORE_KEY) ?? "[]") as unknown;
    if (Array.isArray(stored))
      for (const entry of stored)
        if (Array.isArray(entry) && typeof entry[0] === "string" && entry[1] >= 1 && entry[2] >= 1) sizes.set(entry[0], { w: entry[1], h: entry[2] });
  } catch {
    // An unreadable store holds nothing.
  }
  return sizes;
}

/** A 53-bit digest of `text` (cyrb53), in base 36: two sources that share one share a room until the first lands. */
function digest(text: string): string {
  let a = 0xdeadbeef;
  let b = 0x41c6ce57;
  for (let i = 0; i < text.length; i += 1) {
    const c = text.charCodeAt(i);
    a = Math.imul(a ^ c, 2654435761);
    b = Math.imul(b ^ c, 1597334677);
  }
  a = Math.imul(a ^ (a >>> 16), 2246822507) ^ Math.imul(b ^ (b >>> 13), 3266489909);
  b = Math.imul(b ^ (b >>> 16), 2246822507) ^ Math.imul(a ^ (a >>> 13), 3266489909);
  return (4294967296 * (2097151 & b) + (a >>> 0)).toString(36);
}

/** What a drawing's size is kept under: `engine` and its version, the text `scale` the engine lays labels out at, and the source. */
export function diagramKey(engine: DiagramRequest["engine"], code: string, scale: string): string {
  return `${engine}@${ENGINE_VERSIONS[engine]}|${scale}|${code.length}.${digest(code)}`;
}

/** The size kept under `key`, in this session or an earlier one. */
export function keptDiagramSize(key: string): DiagramSize | undefined {
  return kept().get(key);
}

/** Keeps `size` under `key`, for this session at once and for later ones within a second. */
export function keepDiagramSize(key: string, size: DiagramSize): void {
  const all = kept();
  const was = all.get(key);
  if (was?.w === size.w && was.h === size.h) return;
  all.delete(key);
  all.set(key, size);
  if (all.size > KEPT) all.delete(all.keys().next().value!);
  if (saving !== null) return;
  saving = setTimeout(() => {
    saving = null;
    try {
      store()?.setItem(STORE_KEY, JSON.stringify([...kept()].map(([at, { w, h }]) => [at, w, h])));
    } catch {
      // A full or refused store keeps the sizes for this session.
    }
  }, 1000);
}

/** Forgets every size, and the store's: for tests. */
export function forgetDiagramSizes(): void {
  sizes = new Map();
  if (saving !== null) clearTimeout(saving);
  saving = null;
  try {
    store()?.removeItem(STORE_KEY);
  } catch {
    // Nothing kept.
  }
}

/** Reads the store again, as a new session would: for tests. */
export function reloadDiagramSizes(): void {
  sizes = null;
}
