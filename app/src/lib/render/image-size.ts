/**
 * An image's size from the head of its bytes, with nothing decoded (the ux21 README-image pass, 2026-11-19,
 * `docs/decisions/2026-11-19-a-readme-image-keeps-its-room.md`): a README's logo drew from nothing and pushed the text
 * under it down when its bytes loaded. Read from the bytes as soon as they are in hand, the `<img>` gets its `width` and
 * `height` before its `src`, so its room is right in the frame it draws.
 *
 * PNG, GIF, JPEG (turned as its EXIF orientation says, as the webview draws it), WebP (lossy, lossless, extended), BMP
 * and SVG (its root's `width`/`height` in pixels, one of them with its `viewBox`'s shape, else the shape fitted to the
 * 300x150 an image with only a shape draws at). Anything else, or a head that does not parse, is `null`: its size is known once it loads.
 */

export interface ImageSize {
  w: number;
  h: number;
}

/** The largest side believed: a head that claims more is corrupt or hostile, and the image's own load decides. */
const MAX_SIDE = 65_535;

const sized = (w: number, h: number): ImageSize | null =>
  Number.isFinite(w) && Number.isFinite(h) && w >= 1 && h >= 1 && w <= MAX_SIDE && h <= MAX_SIDE ? { w: Math.round(w), h: Math.round(h) } : null;

/** The size of the image `bytes` hold; `null` where their head says none. */
export function imageSize(bytes: Uint8Array): ImageSize | null {
  try {
    return png(bytes) ?? gif(bytes) ?? jpeg(bytes) ?? webp(bytes) ?? bmp(bytes) ?? svg(bytes);
  } catch {
    return null;
  }
}

/** The size of a base64 `data:image/…` URL's image, from the head of its bytes only; `null` for anything else. */
export function dataUrlSize(url: string): ImageSize | null {
  const match = /^data:image\/([a-z0-9.+-]+)(?:;[^,;]*)*?(;base64)?,/i.exec(url);
  if (match === null) return null;
  const body = url.slice(match[0].length);
  if (match[2] === undefined) {
    // A text body (an SVG written out): its head is enough.
    try {
      return svg(new TextEncoder().encode(decodeURIComponent(body.slice(0, 4096))));
    } catch {
      return null;
    }
  }
  // A JPEG's size can stand after a long EXIF block; 64 KiB of head covers every camera's.
  const head = body.slice(0, 87_384).replace(/[^A-Za-z0-9+/]/g, "");
  try {
    const raw = atob(head.slice(0, head.length - (head.length % 4)));
    return imageSize(Uint8Array.from(raw, (c) => c.charCodeAt(0)));
  } catch {
    return null;
  }
}

const be16 = (b: Uint8Array, i: number) => (b[i]! << 8) | b[i + 1]!;
const le16 = (b: Uint8Array, i: number) => b[i]! | (b[i + 1]! << 8);
const be32 = (b: Uint8Array, i: number) => ((b[i]! << 24) >>> 0) + ((b[i + 1]! << 16) | (b[i + 2]! << 8) | b[i + 3]!);
const le24 = (b: Uint8Array, i: number) => b[i]! | (b[i + 1]! << 8) | (b[i + 2]! << 16);
const le32 = (b: Uint8Array, i: number) => (b[i]! | (b[i + 1]! << 8) | (b[i + 2]! << 16) | (b[i + 3]! << 24)) | 0;
const ascii = (b: Uint8Array, i: number, n: number) => String.fromCharCode(...b.subarray(i, i + n));

function png(b: Uint8Array): ImageSize | null {
  if (b.length < 24 || be32(b, 0) !== 0x89504e47 || be32(b, 4) !== 0x0d0a1a0a || ascii(b, 12, 4) !== "IHDR") return null;
  return sized(be32(b, 16), be32(b, 20));
}

function gif(b: Uint8Array): ImageSize | null {
  if (b.length < 10 || !/^GIF8[79]a$/.test(ascii(b, 0, 6))) return null;
  return sized(le16(b, 6), le16(b, 8));
}

/** A JPEG's frame size, turned where its EXIF orientation is a quarter turn (5 to 8), as the webview draws it. */
function jpeg(b: Uint8Array): ImageSize | null {
  if (b.length < 4 || b[0] !== 0xff || b[1] !== 0xd8) return null;
  let turned = false;
  let i = 2;
  while (i + 4 <= b.length) {
    if (b[i] !== 0xff) return null;
    const marker = b[i + 1]!;
    if (marker === 0xff) {
      i += 1;
      continue;
    }
    // Standalone markers carry no length.
    if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd8)) {
      i += 2;
      continue;
    }
    const length = be16(b, i + 2);
    if (length < 2) return null;
    if (marker === 0xe1 && ascii(b, i + 4, 6) === "Exif\0\0") turned = quarterTurned(b, i + 10, Math.min(b.length, i + 2 + length));
    // A start of frame: every SOFn but DHT (C4), JPG (C8) and DAC (CC).
    if (marker >= 0xc0 && marker <= 0xcf && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc) {
      if (i + 9 > b.length) return null;
      const h = be16(b, i + 5);
      const w = be16(b, i + 7);
      return turned ? sized(h, w) : sized(w, h);
    }
    if (marker === 0xda || marker === 0xd9) return null;
    i += 2 + length;
  }
  return null;
}

/** Whether the TIFF block at `at` (to `end`) says an orientation of 5 to 8: the image stands a quarter turn round. */
function quarterTurned(b: Uint8Array, at: number, end: number): boolean {
  if (at + 8 > end) return false;
  const order = ascii(b, at, 2);
  if (order !== "II" && order !== "MM") return false;
  const u16 = (i: number) => (order === "II" ? le16(b, i) : be16(b, i));
  const u32 = (i: number) => (order === "II" ? le32(b, i) >>> 0 : be32(b, i));
  const ifd = at + u32(at + 4);
  if (ifd + 2 > end) return false;
  const count = u16(ifd);
  for (let n = 0; n < count; n += 1) {
    const entry = ifd + 2 + n * 12;
    if (entry + 12 > end) return false;
    if (u16(entry) === 0x0112) {
      const orientation = u16(entry + 8);
      return orientation >= 5 && orientation <= 8;
    }
  }
  return false;
}

function webp(b: Uint8Array): ImageSize | null {
  if (b.length < 30 || ascii(b, 0, 4) !== "RIFF" || ascii(b, 8, 4) !== "WEBP") return null;
  const chunk = ascii(b, 12, 4);
  if (chunk === "VP8X") return sized(1 + le24(b, 24), 1 + le24(b, 27));
  if (chunk === "VP8L") {
    if (b[20] !== 0x2f) return null;
    const bits = b[21]! | (b[22]! << 8) | (b[23]! << 16) | (b[24]! << 24);
    return sized(1 + (bits & 0x3fff), 1 + ((bits >>> 14) & 0x3fff));
  }
  if (chunk === "VP8 ") {
    if (b[23] !== 0x9d || b[24] !== 0x01 || b[25] !== 0x2a) return null;
    return sized(le16(b, 26) & 0x3fff, le16(b, 28) & 0x3fff);
  }
  return null;
}

function bmp(b: Uint8Array): ImageSize | null {
  if (b.length < 26 || ascii(b, 0, 2) !== "BM") return null;
  const header = le32(b, 14);
  if (header === 12) return sized(le16(b, 18), le16(b, 20));
  return sized(Math.abs(le32(b, 18)), Math.abs(le32(b, 22)));
}

/** An SVG side in pixels: a plain number or `px`; a percentage or another unit says nothing. */
const side = (value: string | undefined): number | null => {
  const match = value === undefined ? null : /^\s*(\d*\.?\d+(?:e[+-]?\d+)?)\s*(px)?\s*$/i.exec(value);
  return match === null ? null : Number(match[1]);
};

function svg(b: Uint8Array): ImageSize | null {
  const text = new TextDecoder().decode(b.subarray(0, 8192));
  const root = /<svg\b([^>]*)>/i.exec(text);
  if (root === null) return null;
  const attrs: Record<string, string> = {};
  for (const [, name, , double, single] of root[1]!.matchAll(/([\w:-]+)\s*=\s*("([^"]*)"|'([^']*)')/g)) attrs[name!.toLowerCase()] = double ?? single ?? "";
  const box = attrs["viewbox"]?.trim().split(/[\s,]+/).map(Number);
  const ratio = box && box.length === 4 && box[2]! > 0 && box[3]! > 0 ? box[2]! / box[3]! : null;
  const w = side(attrs["width"]);
  const h = side(attrs["height"]);
  if (w !== null && h !== null) return sized(w, h);
  // One side and no shape, or nothing at all: left to the image's load.
  if (ratio === null) return null;
  if (w !== null) return sized(w, w / ratio);
  if (h !== null) return sized(h * ratio, h);
  // Only a shape: an `<img>` draws it fitted inside 300x150 (CSS Images 3, the default object size).
  return ratio >= 2 ? sized(300, 300 / ratio) : sized(150 * ratio, 150);
}
