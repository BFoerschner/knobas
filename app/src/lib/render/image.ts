/**
 * An image's bytes as something an `<img>` may load (#2256, spec #2234 step
 * 5): a `Blob` of the type the file name says, for `URL.createObjectURL`
 * (the CSP's `img-src` allows `blob:`).
 *
 * An SVG is sanitized first (spec story 44): DOMPurify's SVG profile drops
 * `<script>`, every handler, `<foreignObject>` and anything else that is not
 * drawing, and what is left is written back out as XML, so the namespace an
 * image-loaded SVG needs is on its root. An `<img>` runs no script in an SVG
 * anyway; the sanitizer is the second wall, and the one that holds if the
 * bytes ever reach a surface that inlines them.
 */
import DOMPurify from "dompurify";

const TYPES: Record<string, string> = {
  svg: "image/svg+xml",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  ico: "image/x-icon",
  png: "image/png",
  gif: "image/gif",
  webp: "image/webp",
  avif: "image/avif",
  bmp: "image/bmp",
};

/**
 * The media type of the image at `path`, by its extension; `application/octet-stream` for one not listed. Never built
 * from the name: `x.svg+xml;charset=utf-8` would make an SVG type that skips the sanitizer.
 */
export function imageType(path: string): string {
  const ext = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
  return TYPES[ext] ?? "application/octet-stream";
}

/**
 * `markup`'s first `<svg>`, with nothing left in it that is not drawing (no
 * script, no handler, no `javascript:` link), or `null`. The one SVG
 * sanitizer: an image here, the sandboxed frame (`frame.ts`) too.
 */
export function sanitizeSvg(markup: string): SVGSVGElement | null {
  return DOMPurify.sanitize(markup, { USE_PROFILES: { svg: true, svgFilters: true }, RETURN_DOM_FRAGMENT: true }).querySelector("svg");
}

/** The SVG text `svg`, sanitized and written back out as XML; `""` where no `<svg>` survives. */
function sanitizedSvgText(svg: string): string {
  const root = sanitizeSvg(svg);
  return root === null ? "" : new XMLSerializer().serializeToString(root);
}

/** `bytes`, the image at `path`, as a blob an `<img>` may load; an SVG sanitized. */
export function imageBlob(bytes: Uint8Array, path: string): Blob {
  return typedImageBlob(bytes, imageType(path));
}

/**
 * `bytes` as a blob of media type `type`, an SVG sanitized. A type that is
 * not one of the listed image types becomes `application/octet-stream`, so no
 * caller can name an SVG type the sanitizer would miss.
 */
export function typedImageBlob(bytes: Uint8Array, type: string): Blob {
  const known = Object.values(TYPES).includes(type) ? type : "application/octet-stream";
  if (known === "image/svg+xml") return new Blob([sanitizedSvgText(new TextDecoder().decode(bytes))], { type: known });
  return new Blob([bytes as Uint8Array<ArrayBuffer>], { type: known });
}

/**
 * A base64 `data:` URL's bytes and media type (a command renderer's image,
 * #2262); `null` for anything else. The type is the URL's as written:
 * {@link typedImageBlob} decides what it may be.
 */
export function dataUrlImage(url: string): { bytes: Uint8Array; type: string } | null {
  const match = /^data:([^;,]*);base64,(.*)$/s.exec(url);
  if (match === null) return null;
  try {
    return { type: match[1]!.toLowerCase(), bytes: Uint8Array.from(atob(match[2]!), (c) => c.charCodeAt(0)) };
  } catch {
    return null;
  }
}
