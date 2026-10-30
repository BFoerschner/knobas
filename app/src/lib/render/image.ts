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

/** The media type of the image at `path`, by its extension. */
export function imageType(path: string): string {
  const ext = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
  return TYPES[ext] ?? `image/${ext}`;
}

/** The SVG text `svg`, with nothing left in it that is not drawing; `""` where no `<svg>` survives. */
export function sanitizeSvg(svg: string): string {
  const fragment = DOMPurify.sanitize(svg, {
    USE_PROFILES: { svg: true, svgFilters: true },
    RETURN_DOM_FRAGMENT: true,
  });
  const root = fragment.querySelector("svg");
  return root === null ? "" : new XMLSerializer().serializeToString(root);
}

/** `bytes`, the image at `path`, as a blob an `<img>` may load; an SVG sanitized. */
export function imageBlob(bytes: Uint8Array, path: string): Blob {
  const type = imageType(path);
  if (type === "image/svg+xml") return new Blob([sanitizeSvg(new TextDecoder().decode(bytes))], { type });
  return new Blob([bytes as Uint8Array<ArrayBuffer>], { type });
}
