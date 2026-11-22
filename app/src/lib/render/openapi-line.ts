/**
 * Which place of a spec a line of its text stands in (ux31; Björn, 2026-11-22, on the Everything search: "Having it
 * open with the correct path already would be nice"): a search hit inside a spec opens the explorer at the operation
 * or schema that holds the hit's line, not at the overview. There is no index of operations: the hit's line is
 * mapped to its place when the file opens, by one scan of the text up to that line.
 *
 * - **`pathAtLine`** answers the keys from the document's top to what line `n` (1-based) defines: a JSON scan that
 *   tracks strings, depth and each object's current key; a YAML scan by indentation that skips block scalars and
 *   comments, for the YAML spec tools write.
 * - **`placeOfPath`** turns that into a place: `paths./pets/{id}.get…` is `get /pets/{id}`, a path item's own lines
 *   its first operation, `components.schemas.Pet…` (2.0's `definitions.Pet…`) `schemas/Pet`, a 3.1 webhook its
 *   operation; anything else is no place (the overview).
 */
import { METHODS } from "./openapi";

/** The keys and indexes from the top to what line `line` (1-based) of `text` defines; `[]` at the top. */
export function pathAtLine(text: string, line: number): (string | number)[] {
  const body = text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;
  return /^\s*[{[]/.test(body) ? jsonPath(body, line) : yamlPath(body, line);
}

type Frame = { kind: "obj"; key: string | null } | { kind: "arr"; index: number };

function jsonPath(text: string, line: number): (string | number)[] {
  const stack: Frame[] = [];
  let at = 1;
  let i = 0;
  let expectKey = false;
  /** The path as it stood when the target line's first key was read, or at its start. */
  let found: (string | number)[] | null = null;
  const snapshot = (): (string | number)[] => stack.flatMap((f): (string | number)[] => (f.kind === "obj" ? (f.key === null ? [] : [f.key]) : [f.index]));
  while (i < text.length) {
    const c = text.charCodeAt(i);
    if (c === 10) {
      if (at === line && found === null) found = snapshot();
      at += 1;
      if (at > line) break;
      i += 1;
      continue;
    }
    if (c === 34 /* " */) {
      let j = i + 1;
      while (j < text.length) {
        const d = text.charCodeAt(j);
        if (d === 92) j += 2;
        else if (d === 34) break;
        else {
          // A newline inside a string is a JSON error; count it, as the line numbers do.
          if (d === 10) at += 1;
          j += 1;
        }
      }
      if (expectKey) {
        const top = stack.at(-1);
        let key: string;
        try {
          key = JSON.parse(text.slice(i, j + 1)) as string;
        } catch {
          key = text.slice(i + 1, j);
        }
        if (top?.kind === "obj") top.key = key;
        expectKey = false;
        if (at === line && found === null) found = snapshot();
      }
      i = j + 1;
      continue;
    }
    if (c === 123 /* { */) {
      stack.push({ kind: "obj", key: null });
      expectKey = true;
    } else if (c === 91 /* [ */) {
      stack.push({ kind: "arr", index: 0 });
    } else if (c === 125 || c === 93 /* } ] */) {
      stack.pop();
    } else if (c === 44 /* , */) {
      const top = stack.at(-1);
      if (top?.kind === "arr") top.index += 1;
      else if (top?.kind === "obj") expectKey = true;
    }
    i += 1;
  }
  return found ?? snapshot();
}

const YAML_KEY = /^(\s*)(-\s+)?("(?:[^"\\]|\\.)*"|'(?:[^']|'')*'|[^\s#'"{[][^:#]*?)\s*:(?:\s|$)/;

function yamlPath(text: string, line: number): (string | number)[] {
  const stack: { indent: number; key: string | number }[] = [];
  /** Lines deeper than this are a block scalar's text (`description: |`), never keys. */
  let scalarUnder = -1;
  const lines = text.split("\n", line);
  let result: (string | number)[] = [];
  for (let n = 0; n < lines.length; n++) {
    const raw = lines[n]!.replace(/\r$/, "");
    const content = raw.trimStart();
    const indent = raw.length - content.length;
    const isTarget = n === line - 1;
    if (scalarUnder >= 0) {
      if (content === "" || indent > scalarUnder) {
        if (isTarget) result = stack.map((f) => f.key);
        continue;
      }
      scalarUnder = -1;
    }
    if (content === "" || content.startsWith("#") || content === "---") {
      if (isTarget) result = stack.map((f) => f.key);
      continue;
    }
    const m = YAML_KEY.exec(raw);
    const item = /^-(\s|$)/.test(content);
    if (m !== null) {
      const keyIndent = m[1]!.length + (m[2]?.length ?? 0);
      while (stack.length > 0 && stack.at(-1)!.indent >= keyIndent) stack.pop();
      let key = m[3]!.trim();
      if (key.startsWith('"')) {
        try {
          key = JSON.parse(key) as string;
        } catch {
          key = key.slice(1, -1);
        }
      } else if (key.startsWith("'")) key = key.slice(1, -1).replace(/''/g, "'");
      stack.push({ indent: keyIndent, key });
      const rest = raw.slice(m[0].length).trim();
      if (/^[|>][-+0-9]*\s*(#.*)?$/.test(rest)) scalarUnder = keyIndent;
    } else if (item) {
      while (stack.length > 0 && stack.at(-1)!.indent >= indent + 2) stack.pop();
    } else if (stack.length > 0 && indent <= stack.at(-1)!.indent) {
      while (stack.length > 0 && stack.at(-1)!.indent >= indent) stack.pop();
    }
    if (isTarget) result = stack.map((f) => f.key);
  }
  return result;
}

/** The place a path from the top names: an operation, a schema, or `null` for the overview. */
export function placeOfPath(path: readonly (string | number)[], firstOp: (p: string) => string | null): string | null {
  const [top, a, b] = path;
  if (top === "paths" && typeof a === "string") {
    if (typeof b === "string" && (METHODS as readonly string[]).includes(b)) return `${b} ${a}`;
    return firstOp(a);
  }
  if (top === "webhooks" && typeof a === "string" && typeof b === "string" && (METHODS as readonly string[]).includes(b)) return `${b} ${a}`;
  if (top === "components" && a === "schemas" && typeof b === "string") return `schemas/${b}`;
  if (top === "definitions" && typeof a === "string") return `schemas/${a}`;
  return null;
}
