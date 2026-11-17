/**
 * The sizes of the images a text has drawn, for the next time it draws them (the nothing-moves pass, 2026-11-17,
 * `docs/decisions/2026-11-17-nothing-moves-once-drawn.md`). A repository's README read again after a sync drew its
 * badge image anew: its bytes came a moment after the text, and the image growing from nothing pushed everything
 * under it 15 px down. An image seen before is drawn at its size from the start, so its box is held while its bytes
 * are read. The first time an image is ever drawn still grows its paragraph (ruled on 2026-11-11: a document's size).
 */

/** How many images are remembered; the oldest go first. */
const KEPT = 500;
const sizes = new Map<string, { w: number; h: number }>();

/**
 * Holds `img`'s room at the size it last loaded at under `key` (a repository path, an attachment), and remembers the
 * size it loads at now. An image whose text names its own size keeps that.
 */
export function holdImageRoom(img: HTMLImageElement, key: string): void {
  if (!img.hasAttribute("width") && !img.hasAttribute("height")) {
    const known = sizes.get(key);
    if (known !== undefined) {
      img.width = known.w;
      img.height = known.h;
    }
  }
  img.addEventListener(
    "load",
    () => {
      if (img.naturalWidth <= 0 || img.naturalHeight <= 0) return;
      sizes.delete(key);
      sizes.set(key, { w: img.naturalWidth, h: img.naturalHeight });
      if (sizes.size > KEPT) sizes.delete(sizes.keys().next().value!);
    },
    { once: true },
  );
}

/** Forgets every size: for tests. */
export function forgetImageSizes(): void {
  sizes.clear();
}
