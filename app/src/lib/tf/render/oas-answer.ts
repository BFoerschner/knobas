/**
 * An answer's words in the API explorer (ux31, section 19): the tone its status takes, its size in words, and a JSON
 * line's tokens for the syntax colours. Pure, for its tests and its mutants.
 */

/** A status's tone: `ok` under 300, `warn` under 400, `err` from 400. An outcome that happened, so it has one. */
export function statusTone(status: number): "ok" | "warn" | "err" {
  return status < 300 ? "ok" : status < 400 ? "warn" : "err";
}

/** A size in words: `512 B`, `2.1 KB`, `3.4 MB`. */
export function sizeWords(n: number): string {
  return n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/** One run of a JSON line: `k` a key, `s` a string, `n` a number, `b` true, false or null, `""` the rest. */
export interface Token {
  t: string;
  k: "" | "k" | "s" | "n" | "b";
}

const TOKEN = /("(?:[^"\\]|\\.)*")(\s*:)?|\b(true|false|null)\b|(-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b)/g;

/** A JSON line's tokens, as text: never markup, so an answer cannot draw an element of its own. */
export function jsonTokens(line: string): Token[] {
  const out: Token[] = [];
  let at = 0;
  for (const m of line.matchAll(TOKEN)) {
    const start = m.index;
    if (start > at) out.push({ t: line.slice(at, start), k: "" });
    if (m[1] !== undefined) {
      out.push({ t: m[1], k: m[2] !== undefined ? "k" : "s" });
      if (m[2] !== undefined) out.push({ t: m[2], k: "" });
    } else out.push({ t: m[0], k: m[3] !== undefined ? "b" : "n" });
    at = start + m[0].length;
  }
  if (at < line.length) out.push({ t: line.slice(at), k: "" });
  return out;
}
