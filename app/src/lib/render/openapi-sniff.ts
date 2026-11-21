/**
 * Whether a text file is an OpenAPI or Swagger spec (ux18, `docs/decisions/2026-11-17-an-api-spec-is-drawn-as-its-endpoints.md`):
 * by what it holds, never by its name. A spec is a YAML or JSON document whose top level has an `openapi` key (3.x)
 * or a `swagger` key (`2.0`), so `openapi.yaml`, `api/v1/spec.json` and `petstore.oas.yml` are all found, and a
 * `docker-compose.yml` or a `package.json` is not.
 *
 * It runs on every text file the Code page opens (the registry asks it), so it reads at most {@link HEAD} characters
 * and never parses: a YAML top-level key is a line that starts at column 0; a JSON one is a key of the outermost
 * object, found by a scan that tracks strings and depth. Writers put the version key first; a spec whose version
 * key comes after 256 KB of other keys is drawn as Source, and *Render as* cannot offer it.
 */

/** How much of a file the sniff reads. */
export const HEAD = 256 * 1024;

const YAML_KEY = /^(["']?)(openapi|swagger)\1[ \t]*:[ \t]*(["']?)(\d+(?:\.\d+)*)/m;

/** The spec's own version string (`3.1.0`, `2.0`) and its key, or `null` for a file that is not a spec. */
export function sniffSpec(text: string): { key: "openapi" | "swagger"; version: string } | null {
  let start = 0;
  // A byte-order mark and leading blank space say nothing.
  while (start < text.length && (text.charCodeAt(start) <= 32 || text.charCodeAt(start) === 0xfeff)) start++;
  if (start >= text.length) return null;
  const head = text.length - start > HEAD ? text.slice(start, start + HEAD) : start === 0 ? text : text.slice(start);
  const found = head.charCodeAt(0) === 123 /* { */ ? jsonKey(head) : yamlKey(head);
  if (found === null) return null;
  if (found.key === "openapi" ? !/^3\./.test(found.version) : found.version !== "2.0") return null;
  return found;
}

/** Whether `text` is an OpenAPI 3.x or Swagger 2.0 spec: what the registry's sniff asks. */
export function isOpenApi(text: string): boolean {
  return sniffSpec(text) !== null;
}

function yamlKey(head: string): { key: "openapi" | "swagger"; version: string } | null {
  const m = YAML_KEY.exec(head);
  return m === null ? null : { key: m[2] as "openapi" | "swagger", version: m[4] ?? "" };
}

/**
 * The outermost object's `openapi` or `swagger` key and its string value, scanned without building anything: depth
 * counts brackets outside strings, and at depth 1 the string after `{` or `,` is a key.
 */
function jsonKey(head: string): { key: "openapi" | "swagger"; version: string } | null {
  let depth = 0;
  let expectKey = false;
  let i = 0;
  const n = head.length;
  while (i < n) {
    const c = head.charCodeAt(i);
    if (c === 34 /* " */) {
      const end = stringEnd(head, i);
      if (end < 0) return null;
      // Only the outermost object's `{` and commas start a key, so a key read here is a top-level one.
      if (expectKey) {
        const key = head.slice(i + 1, end);
        expectKey = false;
        if (key === "openapi" || key === "swagger") {
          let j = end + 1;
          while (j < n && isSpace(head.charCodeAt(j))) j++;
          if (head.charCodeAt(j) !== 58 /* : */) return null;
          j++;
          while (j < n && isSpace(head.charCodeAt(j))) j++;
          if (head.charCodeAt(j) !== 34) return null;
          const close = stringEnd(head, j);
          return close < 0 ? null : { key, version: head.slice(j + 1, close) };
        }
      }
      i = end + 1;
      continue;
    }
    if (c === 123 || c === 91 /* { [ */) {
      depth++;
      expectKey = c === 123 && depth === 1;
    } else if (c === 125 || c === 93 /* } ] */) {
      depth--;
      if (depth <= 0) return null;
    } else if (c === 44 /* , */) {
      if (depth === 1) expectKey = true;
    }
    i++;
  }
  return null;
}

function isSpace(c: number): boolean {
  return c === 32 || c === 9 || c === 10 || c === 13;
}

/** The index of the quote that closes the string opened at `open`, or -1 where the head ends first. */
function stringEnd(text: string, open: number): number {
  let i = open + 1;
  for (;;) {
    const q = text.indexOf('"', i);
    if (q < 0) return -1;
    let slashes = 0;
    for (let k = q - 1; k > open && text.charCodeAt(k) === 92; k--) slashes++;
    if (slashes % 2 === 0) return q;
    i = q + 1;
  }
}
