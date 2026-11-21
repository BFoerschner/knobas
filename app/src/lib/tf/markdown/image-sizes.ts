/**
 * The room a text's images take before their bytes are in (the nothing-moves pass, 2026-11-17, and the ux21
 * README-image pass, 2026-11-19: `docs/decisions/2026-11-19-a-readme-image-keeps-its-room.md`). A README's logo drew from
 * nothing and pushed the text under it 15 px down when its bytes loaded.
 *
 * - An image whose text names both its sides keeps them. One that names one side gets the other from its shape.
 * - An image seen before, in this session or an earlier one, draws at that size from its first frame: the sizes are
 *   kept in the webview's `localStorage` (`knobas.image-sizes`, the latest {@link KEPT}), by where the image comes from
 *   (a repository and its path, a ticket and its attachment).
 * - An image never seen gets its size from the head of its bytes the moment they are in (`render/image-size.ts`), before
 *   its `src`; meanwhile the text holds what stands from it down unseen, at most {@link IMAGE_HOLD_MS}
 *   (`TfMarkdown.svelte`).
 * - Its load checks the size: an image that changed since it was kept, or whose head said otherwise, is set right then.
 * - An image whose room is known before its bytes stands as a blank of that size (an SVG `data:` URL) until they are in:
 *   the webview draws an `<img>` with no `src` as its alt text, whatever its `width` and `height` say.
 */
import { dataUrlSize, type ImageSize } from "../../render/image-size";
import { windowStorage, type KeptStorage } from "../kept-value";

/** The longest what stands from an image of unknown size down waits unseen for that size (Fable, 2026-11-19). */
export const IMAGE_HOLD_MS = 300;

/** The tallest an attachment drawn in a text stands (`markdown.css`, `.tf-md img.tf-md-att`'s `max-height`). */
export const ATTACHMENT_MAX_H = 280;

/** How many images are remembered; the oldest go first. */
const KEPT = 500;
const STORE_KEY = "knobas.image-sizes";

let sizes: Map<string, ImageSize> | null = null;
let saving: ReturnType<typeof setTimeout> | null = null;

function store(): KeptStorage | null {
  try {
    return windowStorage();
  } catch {
    return null;
  }
}

function kept(): Map<string, ImageSize> {
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

function keep(key: string, size: ImageSize): void {
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

/** The size kept for `key`, for tests and the probe. */
export function keptImageSize(key: string): ImageSize | undefined {
  return kept().get(key);
}

/** A side the text names: a whole number of pixels; `undefined` where it names none, `NaN` where it names another kind. */
function ownSide(img: HTMLImageElement, name: "width" | "height"): number | undefined {
  const value = img.getAttribute(name);
  if (value === null) return undefined;
  const match = /^\s*(\d+(?:\.\d+)?)\s*(px)?\s*$/i.exec(value);
  return match === null ? NaN : Number(match[1]);
}

interface Room {
  key: string;
  /** The sides the text named. */
  w: number | undefined;
  h: number | undefined;
  /** The tallest the surface draws it (an attachment's figure), so a kept size is never stretched by a `max-height`. */
  maxHeight: number | null;
}

const rooms = new WeakMap<HTMLImageElement, Room>();

/** Sets `img`'s sides for an image of `size`, as its text (one side at most: two are left as they are) and its surface allow. */
function fit(img: HTMLImageElement, room: Room, size: ImageSize): void {
  let w: number;
  let h: number;
  if (room.w !== undefined) [w, h] = [room.w, (room.w * size.h) / size.w];
  else if (room.h !== undefined) [w, h] = [(room.h * size.w) / size.h, room.h];
  else [w, h] = [size.w, size.h];
  if (room.w === undefined && room.h === undefined && room.maxHeight !== null && h > room.maxHeight) [w, h] = [(w * room.maxHeight) / h, room.maxHeight];
  img.width = Math.max(1, Math.round(w));
  img.height = Math.max(1, Math.round(h));
}

/**
 * An empty SVG of `w` by `h` as a `data:` URL. Made as an element and written out, so its namespace comes from the
 * parser. The sides are the attributes, never `img.width`: a drawn `<img>` answers its box, its alt text's here.
 */
export function blankImage(w: string, h: string): string {
  const holder = document.createElement("div");
  holder.innerHTML = "<svg></svg>";
  const svg = holder.firstElementChild!;
  svg.setAttribute("width", w);
  svg.setAttribute("height", h);
  return `data:image/svg+xml,${encodeURIComponent(new XMLSerializer().serializeToString(svg))}`;
}

/** `img` drawn as a blank of the room it holds, until its bytes are in ({@link showImage}). */
function standIn(img: HTMLImageElement): void {
  if (img.hasAttribute("src") || !img.hasAttribute("width") || !img.hasAttribute("height")) return;
  img.dataset["mdStandin"] = "";
  img.src = blankImage(img.getAttribute("width")!, img.getAttribute("height")!);
}

/** Loads `img`'s own bytes from `url`, in place of its stand-in. */
export function showImage(img: HTMLImageElement, url: string): void {
  delete img.dataset["mdStandin"];
  img.src = url;
}

/**
 * Holds `img`'s room as far as it is known, under `key` (where the image comes from), and checks it when the image
 * loads. Answers whether the room is known now: the text names both sides, or the size was kept.
 */
export function holdImageRoom(img: HTMLImageElement, key: string, maxHeight: number | null = null): boolean {
  const room: Room = { key, w: ownSide(img, "width"), h: ownSide(img, "height"), maxHeight };
  // A side named in another unit (`50%`) is the text's to lay out: left as it is.
  if (Number.isNaN(room.w) || Number.isNaN(room.h)) return true;
  if (room.w !== undefined && room.h !== undefined) {
    standIn(img);
    return true;
  }
  rooms.set(img, room);
  const known = kept().get(key);
  if (known !== undefined) {
    fit(img, room, known);
    standIn(img);
  }
  const check = () => {
    // The stand-in's load, or one with nothing drawn, says nothing of the image.
    if (img.dataset["mdStandin"] !== undefined || img.naturalWidth <= 0 || img.naturalHeight <= 0) return;
    img.removeEventListener("load", check);
    const size = { w: img.naturalWidth, h: img.naturalHeight };
    keep(key, size);
    fit(img, room, size);
  };
  img.addEventListener("load", check);
  return known !== undefined;
}

/**
 * The size the head of `img`'s bytes says, before its `src` is set: its room, and kept for the next time. Answers
 * whether a size was set (`null`, a head that says none, leaves it to the load).
 */
export function sizeImageRoom(img: HTMLImageElement, size: ImageSize | null): boolean {
  const room = rooms.get(img);
  if (room === undefined || size === null) return room === undefined;
  keep(room.key, size);
  fit(img, room, size);
  return true;
}

/**
 * Holds the room of an image the text carries whole (a `data:image/…` URL) at the size the head of its bytes says.
 * Answers whether the room is known: the text names both sides, or the head says the size.
 */
export function holdDataImageRoom(img: HTMLImageElement): boolean {
  const room: Room = { key: "", w: ownSide(img, "width"), h: ownSide(img, "height"), maxHeight: null };
  if (Number.isNaN(room.w) || Number.isNaN(room.h) || (room.w !== undefined && room.h !== undefined)) return true;
  const size = dataUrlSize(img.getAttribute("src") ?? "");
  if (size === null) return false;
  fit(img, room, size);
  return true;
}

/** Forgets every size, and the store's: for tests. */
export function forgetImageSizes(): void {
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
export function reloadImageSizes(): void {
  sizes = null;
}
