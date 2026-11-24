/**
 * What opens in a line of a document (the XML view's design, 2026-11-23, sections 13 and 14). Pure; only the lines in
 * view are read.
 *
 * - **A web address** in an element's text or an attribute's value: it opens where knobas opens every address.
 * - **A path** is a link only where the repository holds a file there, read from the document's folder first, then
 *   from the repository's root. A path that opens nothing is worse than plain text.
 * - **A namespace is a name, not a page**: the value of `xmlns` and `xmlns:…`, and the namespace halves of
 *   `xsi:schemaLocation`.
 * - **A schema's location**: the other halves of `xsi:schemaLocation`, and `xsi:noNamespaceSchemaLocation`.
 * - **An image** is an address or a path whose ending is an image's.
 */

export type LinkKind = "web" | "path" | "namespace" | "schema";

/** One thing that opens, by where it stands in the line's text. */
export interface Link {
  at: number;
  end: number;
  kind: LinkKind;
  /** The address, the repository path, the namespace's name, or the schema's location as written. */
  to: string;
  /** It is an image's address or path. */
  image: boolean;
}

/** An image, by its ending (section 14). */
export const IMAGE_ENDING = /\.(png|jpe?g|gif|webp|avif|bmp|ico|svg)(?:[?#].*)?$/i;

const URL_RE = /https?:\/\/[^\s<>"'`]+[^\s<>"'`.,;:!?)\]}]/g;
const ATTR = /([\w:.-]+)(\s*=\s*)(["'])([^"']*)\3/g;

/** Whether a value reads like a path: no spaces, a slash or an ending, no scheme, and a letter in it. */
export function pathLike(v: string): boolean {
  if (v.length <= 2 || v.length > 260) return false;
  if (/[\s<>"|*?]/.test(v) || /^[a-z][a-z0-9+.-]*:/i.test(v) || !/[A-Za-z]/.test(v)) return false;
  // A path ends in a name with an ending: `image/png` is a media type, `a/b` a formula, `1.2.3` a version.
  if (!/[^/]\.[A-Za-z][A-Za-z0-9]{0,7}$/.test(v)) return false;
  return v.includes("/") || /^[\w.-]+$/.test(v);
}

/** `rel` read from the folder of `from`, inside the repository; `null` where it climbs out. */
export function joinPath(from: string, rel: string): string | null {
  if (rel.startsWith("/")) return rel.replace(/^\/+/, "");
  const parts = from.split("/").slice(0, -1);
  for (const p of rel.split("/")) {
    if (p === "" || p === ".") continue;
    if (p === "..") {
      if (parts.length === 0) return null;
      parts.pop();
    } else parts.push(p);
  }
  return parts.join("/");
}

/** The repository path a path-like value names, where a file is there; `null` for none. */
export function fileAt(value: string, docPath: string, has: (path: string) => boolean): string | null {
  const near = joinPath(docPath, value);
  if (near !== null && has(near)) return near;
  const top = value.replace(/^\.?\/+/, "");
  return value.startsWith("..") || !has(top) ? null : top;
}

/** The web addresses in a piece of text, with where they stand. */
export function addressesIn(s: string): { at: number; end: number; url: string }[] {
  const out: { at: number; end: number; url: string }[] = [];
  URL_RE.lastIndex = 0;
  for (let m = URL_RE.exec(s); m !== null; m = URL_RE.exec(s)) out.push({ at: m.index, end: m.index + m[0].length, url: m[0] });
  return out;
}

/**
 * What opens in line `text` of the document at `docPath`: addresses and paths in its text and values, namespaces and
 * schema locations in its attributes. `has` says whether the repository holds a file at a path; until it can answer
 * (the paths not read yet), no path is a link.
 */
export function linksIn(text: string, docPath: string, has: ((path: string) => boolean) | null): Link[] {
  if (text.length < 4) return [];
  const out: Link[] = [];
  const taken: [number, number][] = [];
  const push = (link: Omit<Link, "image">) => {
    out.push({ ...link, image: link.kind !== "namespace" && IMAGE_ENDING.test(link.to) });
    taken.push([link.at, link.end]);
  };
  // Attributes first: a namespace and a schema's location are what their name says, not addresses.
  ATTR.lastIndex = 0;
  for (let m = ATTR.exec(text); m !== null; m = ATTR.exec(text)) {
    const name = m[1]!;
    const value = m[4]!;
    const at = m.index + m[1]!.length + m[2]!.length + 1;
    if (name === "xmlns" || name.startsWith("xmlns:")) {
      if (value !== "") push({ at, end: at + value.length, kind: "namespace", to: value });
    } else if (/(^|:)schemaLocation$/.test(name)) {
      let i = 0;
      for (const part of value.matchAll(/\S+/g)) {
        push({ at: at + part.index, end: at + part.index + part[0].length, kind: i % 2 === 0 ? "namespace" : "schema", to: part[0] });
        i += 1;
      }
    } else if (/(^|:)noNamespaceSchemaLocation$/.test(name) && value.trim() !== "") {
      push({ at, end: at + value.length, kind: "schema", to: value });
    }
  }
  const free = (a: number, b: number) => taken.every(([x, y]) => b <= x || a >= y);
  for (const u of addressesIn(text)) if (free(u.at, u.end)) push({ at: u.at, end: u.end, kind: "web", to: u.url });
  if (has !== null) {
    // Words that read like a path, in values and between tags, tried against the repository.
    const WORD = /[^\s()<>"',;=]+/g;
    for (let m = WORD.exec(text); m !== null; m = WORD.exec(text)) {
      const w = m[0].replace(/[.:]+$/, "");
      if (!free(m.index, m.index + w.length) || !pathLike(w)) continue;
      const to = fileAt(w, docPath, has);
      if (to !== null) push({ at: m.index, end: m.index + w.length, kind: "path", to });
    }
  }
  return out.sort((a, b) => a.at - b.at);
}

/** A run of a line's drawn text, cut where a link starts or ends: `link` is the link it stands in. */
export interface Piece<R> {
  run: R;
  text: string;
  link: number | null;
}

/** `runs` (a line's text, in order) cut at the links' edges, each piece naming the link it stands in. */
export function piecesOf<R extends { text: string }>(runs: readonly R[], links: readonly Link[]): Piece<R>[] {
  if (links.length === 0) return runs.map((run) => ({ run, text: run.text, link: null }));
  const out: Piece<R>[] = [];
  let pos = 0;
  for (const run of runs) {
    const from = pos;
    const to = pos + run.text.length;
    let at = from;
    while (at < to) {
      const inside = links.findIndex((l) => l.at <= at && at < l.end);
      let next = to;
      if (inside !== -1) next = Math.min(to, links[inside]!.end);
      else for (const l of links) if (l.at > at && l.at < next) next = l.at;
      out.push({ run, text: run.text.slice(at - from, next - from), link: inside === -1 ? null : inside });
      at = next;
    }
    pos = to;
  }
  return out;
}
