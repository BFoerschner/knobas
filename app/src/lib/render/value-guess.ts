/**
 * What knobas takes a value for (the XML view's design, 2026-11-23, section 17): a value gets a chip only where one of
 * these holds; everything else is one right-click away.
 *
 * - **Said by the file**: an attribute `encoding="base64"` beside it, a `data:` address (its media type is in it). The
 *   schema's own word (`xs:base64Binary`, `xs:hexBinary`) joins this with the check.
 * - **Read from its own shape**: a JWT, an XML string, a JSON string, the whole value.
 * - **Looks like base64, with nothing said: only where what it decodes to is something knobas knows.** At least 24
 *   characters of the alphabet and white space, a multiple of four, mixing upper case, lower case and a digit, a plus or
 *   a slash; and its first 48 bytes, decoded, start as a PNG, a JPEG, a GIF, a WebP, a PDF, a zip, gzip, zstd, XML or
 *   JSON, or, for 64 characters and more, are text a person could read. An id that happens to be letters and digits
 *   decodes to noise, and gets no chip.
 *
 * What stands beside a value chooses the renderer, and **the bytes overrule it**: a PNG labelled `image/jpeg` is drawn
 * as what it is. Only the lines in view are read, 48 bytes a value. Nothing is decoded but those 48 bytes, here; the
 * chain runs in the backend when a person looks.
 */
import type { PipeStep } from "../ipc/entity";

/** What the file says of a value. */
export interface Said {
  /** A media type beside it: `xmime:contentType`, `contentType`, `mimeType`, a sibling `MIME_TYPE`. */
  mime?: string;
  /** Declared base64: `encoding="base64"`, or the schema's `base64Binary`. */
  base64?: boolean;
  /** Declared hex: the schema's `hexBinary`. */
  hex?: boolean;
}

/** The renderer a chain's output is drawn by. */
export type End = "image" | "pdf" | "xml" | "json" | "text" | "hex" | "archive";

/** What bytes are, by how they start. */
export interface Sniffed {
  kind: "image" | "pdf" | "zip" | "gzip" | "zstd" | "xml" | "json" | "text";
  mime: string;
  ext: string;
  word: string;
}

/** What knobas takes a value for, and the chain it would send it through first. */
export interface Guess {
  what: "base64" | "hex" | "data" | "jwt" | "xml" | "json";
  /** The steps after the value is taken out of the file. */
  steps: PipeStep[];
  end: End;
  /** Said by the file or by its shape, not only read from its decoded start. */
  sure: boolean;
  /** What the chip says: `base64 PDF 1.4 KB`, `JWT`, `data png`. */
  chip: { lead: string; bold: string; tail: string };
  /** In words, for the chip's title and the bar's line. */
  why: string;
  /** What the decoded bytes are, where known. */
  sniffed: Sniffed | null;
  /** How many bytes it decodes to, for base64. */
  bytes: number;
}

const B64_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/** The first `count` bytes of base64 text, decoded: enough to say what it holds. */
export function b64Head(s: string, count: number): number[] {
  const want = Math.ceil(count / 3) * 4;
  const clean = s.slice(0, want + 64).replace(/\s+/g, "").slice(0, want);
  const out: number[] = [];
  const at = (ch: string | undefined) => (ch === undefined || ch === "=" ? -1 : B64_ALPHABET.indexOf(ch === "-" ? "+" : ch === "_" ? "/" : ch));
  for (let i = 0; i + 1 < clean.length; i += 4) {
    const a = at(clean[i]);
    const b = at(clean[i + 1]);
    const c = at(clean[i + 2]);
    const d = at(clean[i + 3]);
    if (a < 0 || b < 0) break;
    out.push((a << 2) | (b >> 4));
    if (c >= 0) out.push(((b & 15) << 4) | (c >> 2));
    if (c >= 0 && d >= 0) out.push(((c & 3) << 6) | d);
  }
  return out.slice(0, count);
}

/** What bytes are, by how they start; `null` where nothing is recognised. */
export function sniffBytes(b: ArrayLike<number>): Sniffed | null {
  const at = (i: number) => (i < b.length ? b[i]! : -1);
  const ascii = (s: string, o = 0) => {
    for (let i = 0; i < s.length; i += 1) if (at(o + i) !== s.charCodeAt(i)) return false;
    return true;
  };
  if (at(0) === 0x89 && ascii("PNG", 1)) return { kind: "image", mime: "image/png", ext: "png", word: "PNG" };
  if (at(0) === 0xff && at(1) === 0xd8 && at(2) === 0xff) return { kind: "image", mime: "image/jpeg", ext: "jpg", word: "JPEG" };
  if (ascii("GIF87a") || ascii("GIF89a")) return { kind: "image", mime: "image/gif", ext: "gif", word: "GIF" };
  if (ascii("RIFF") && ascii("WEBP", 8)) return { kind: "image", mime: "image/webp", ext: "webp", word: "WebP" };
  if (ascii("%PDF-")) return { kind: "pdf", mime: "application/pdf", ext: "pdf", word: "PDF" };
  if (ascii("PK\u0003\u0004")) return { kind: "zip", mime: "application/zip", ext: "zip", word: "zip" };
  if (at(0) === 0x1f && at(1) === 0x8b) return { kind: "gzip", mime: "application/gzip", ext: "gz", word: "gzip" };
  if (at(0) === 0x28 && at(1) === 0xb5 && at(2) === 0x2f && at(3) === 0xfd) return { kind: "zstd", mime: "application/zstd", ext: "zst", word: "zstd" };
  let i = 0;
  if (at(0) === 0xef && at(1) === 0xbb && at(2) === 0xbf) i = 3;
  while (at(i) === 32 || at(i) === 10 || at(i) === 13 || at(i) === 9) i += 1;
  if (ascii("<?xml", i) || (at(i) === 60 && ((at(i + 1) >= 65 && at(i + 1) <= 122) || at(i + 1) === 33)))
    return ascii("<svg", i) ? { kind: "image", mime: "image/svg+xml", ext: "svg", word: "SVG" } : { kind: "xml", mime: "application/xml", ext: "xml", word: "XML" };
  if ((at(i) === 123 && (at(i + 1) === 34 || at(i + 1) === 10 || at(i + 1) === 32 || at(i + 1) === 13)) || (at(i) === 91 && (at(i + 1) === 123 || at(i + 1) === 34)))
    return { kind: "json", mime: "application/json", ext: "json", word: "JSON" };
  return null;
}

/** Text a person could read: tabs, line ends, printable ASCII, and the lead bytes of UTF-8. */
const readable = (b: number[]) => b.length > 0 && b.every((c) => c === 9 || c === 10 || c === 13 || (c >= 32 && c < 127) || c >= 0xc2);

/** The renderer for a media type and what the bytes are: the bytes first. */
export function endFor(mime: string | null | undefined, kind: Sniffed | null): End {
  if (kind !== null) {
    if (kind.kind === "image") return "image";
    if (kind.kind === "pdf") return "pdf";
    if (kind.kind === "xml") return "xml";
    if (kind.kind === "json") return "json";
    if (kind.kind === "zip") return "archive";
    if (kind.kind === "text") return "text";
  }
  const m = (mime ?? "").toLowerCase();
  if (m.startsWith("image/")) return "image";
  if (m === "application/pdf") return "pdf";
  if (/[/+]xml$/.test(m)) return "xml";
  if (/[/+]json$/.test(m)) return "json";
  if (m.startsWith("text/")) return "text";
  return "hex";
}

/** A size in the unit a person reads it in. */
export function sizeWord(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const JWT = /^eyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{2,}\.[A-Za-z0-9_-]*$/;
const B64 = /^[A-Za-z0-9+/\s]+={0,2}\s*$/;
const DATA = /^data:([\w.+-]+\/[\w.+-]+)?(?:;charset=[\w-]+)?(;base64)?,/i;

/** How long a whole value is, where only its start is at hand: in characters, and with its white space taken out. */
export interface Whole {
  length: number;
  bare: number;
}

/**
 * What `value` is taken for and the first chain, or `null` where knobas takes it for nothing. `value` may be only the
 * start of a long one (4 KB is plenty); `whole` then says how long all of it is.
 */
export function guess(value: string, said: Said = {}, whole?: Whole): Guess | null {
  const v = value.trim();
  const length = whole?.length ?? v.length;
  if (v.length === 0) return null;
  const data = DATA.exec(v);
  if (data !== null) {
    const mime = (data[1] ?? "text/plain").toLowerCase();
    // A `data:` address is the value: its head is cut off by the text step that reads it.
    const steps: PipeStep[] = data[2] !== undefined ? [{ step: "base64_decode" }] : [{ step: "url_decode" }];
    return { what: "data", steps, end: endFor(mime, null), sure: true, chip: { lead: "data", bold: mime.split("/")[1] ?? mime, tail: "" }, why: `a data: address of ${mime}`, sniffed: null, bytes: 0 };
  }
  if (JWT.test(v)) return { what: "jwt", steps: [{ step: "jwt" }], end: "json", sure: true, chip: { lead: "", bold: "JWT", tail: "" }, why: "a JWT, by its three parts", sniffed: null, bytes: 0 };
  if (said.hex === true && /^(?:[0-9a-fA-F]{2}\s*)+$/.test(v)) {
    return { what: "hex", steps: [{ step: "hex_decode" }], end: endFor(said.mime, null), sure: true, chip: { lead: "", bold: "hex", tail: "" }, why: "hex, as its schema says", sniffed: null, bytes: Math.floor(v.replace(/\s+/g, "").length / 2) };
  }
  const bare = whole?.bare ?? v.replace(/\s+/g, "").length;
  const looks = length >= 24 && B64.test(v) && bare % 4 === 0;
  if (said.base64 === true || looks) {
    const head = b64Head(v, 48);
    let kind = sniffBytes(head);
    if (said.base64 !== true) {
      // Undeclared: letters and digits alone are not enough. It must mix its cases and decode to something known.
      if (!/[A-Z]/.test(v) || !/[a-z]/.test(v) || !/[0-9+/]/.test(v)) kind = null;
      else if (kind === null && length >= 64 && readable(head)) kind = { kind: "text", mime: "text/plain", ext: "txt", word: "text" };
      if (kind === null) return null;
    }
    const steps: PipeStep[] = [{ step: "base64_decode" }];
    if (kind?.kind === "gzip") steps.push({ step: "gunzip" });
    const pad = v.endsWith("==") ? 2 : v.endsWith("=") ? 1 : 0;
    const bytes = Math.floor((bare * 3) / 4) - (whole === undefined ? pad : 0);
    const word = kind?.word ?? (said.mime ?? "bytes");
    const end: End = kind?.kind === "gzip" ? "text" : endFor(said.mime ?? null, kind);
    const why = said.base64 === true ? "base64, as the file says" : `base64 that holds ${kind!.kind === "text" ? "text" : `a ${kind!.word}`}, by how its start decodes`;
    return { what: "base64", steps, end, sure: said.base64 === true, chip: { lead: "base64", bold: word, tail: sizeWord(bytes) }, why, sniffed: kind, bytes };
  }
  if (/^(?:<\?xml|<[A-Za-z_][\w:.-]*[\s>/])/.test(v) && /<\/[\w:.-]+>\s*$|\/>\s*$/.test(v))
    return { what: "xml", steps: [], end: "xml", sure: true, chip: { lead: "", bold: "XML", tail: "" }, why: "an XML document, written inside the value", sniffed: null, bytes: 0 };
  if ((v.startsWith("{") && v.endsWith("}") && /^\{\s*"/.test(v)) || (v.startsWith("[") && v.endsWith("]") && /^\[\s*[{["]/.test(v)))
    return { what: "json", steps: [], end: "json", sure: true, chip: { lead: "", bold: "JSON", tail: "" }, why: "JSON, written inside the value", sniffed: null, bytes: 0 };
  return null;
}
