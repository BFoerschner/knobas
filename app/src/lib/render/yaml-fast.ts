/**
 * A fast reader for the YAML that API spec tools write (ux18): block mappings and sequences, plain and quoted scalars
 * on one line, flow collections on one line, and literal and folded block scalars. js-yaml read a 2 MB spec of 5,000
 * operations in 50 ms (`JSON.parse` reads the same document in 8), the whole of a big spec's open; this reads it in a
 * fraction of that.
 *
 * It gives exactly what js-yaml's `load(text, { schema: JSON_SCHEMA, json: true })` gives, or nothing: on anything it
 * does not read for certain (anchors, aliases and tags, a scalar or a flow over several lines, an indentation
 * indicator, a tab in an indentation, a key YAML would read as a number, two documents, a line it cannot place) it
 * answers `undefined` and the caller asks js-yaml, which also says where a broken file breaks. `yaml-fast.test.ts`
 * holds it to js-yaml's answers.
 */

/** Thrown inside to say "not for certain": the caller then asks js-yaml. */
const UNSURE = Symbol("unsure");

/** `text` read as YAML, or `undefined` where this reader is not certain to read it as js-yaml would. */
export function fastYaml(text: string): unknown {
  try {
    return new Reader(text).document();
  } catch (cause) {
    if (cause === UNSURE) return undefined;
    throw cause;
  }
}

/** js-yaml's JSON-schema int and float, as one: an int it reads with `parseInt` reads the same with `Number`. */
const NUMBER = /^-?(?:0|[1-9][0-9]*)(?:\.[0-9]*)?(?:[eE][-+]?[0-9]+)?$/;

/** A plain scalar as js-yaml's JSON schema resolves it: `null`, `true`, `false`, a JSON-shaped number, else a string. */
function plain(s: string): unknown {
  const c = s.charCodeAt(0);
  if (c === 110 /* n */ && s === "null") return null;
  if (c === 116 /* t */ && s === "true") return true;
  if (c === 102 /* f */ && s === "false") return false;
  if ((c === 45 || (c >= 48 && c <= 57)) && NUMBER.test(s)) {
    const n = Number(s);
    if (Number.isFinite(n)) return n;
  }
  return s;
}

/** Characters a plain scalar may not start with, in a block. */
function badStart(c: number, next: number): boolean {
  switch (c) {
    case 38: // &
    case 42: // *
    case 33: // !
    case 37: // %
    case 64: // @
    case 96: // `
    case 44: // ,
    case 93: // ]
    case 125: // }
    case 35: // #
    case 124: // |
    case 62: // >
    case 39: // '
    case 34: // "
    case 91: // [
    case 123: // {
      return true;
    case 45: // -
    case 63: // ?
    case 58: // :
      return next === 32 || Number.isNaN(next);
    default:
      return false;
  }
}

const ESCAPES: Record<string, string> = { "\\": "\\", '"': '"', "/": "/", n: "\n", t: "\t", r: "\r", "0": "\0", b: "\b", f: "\f", " ": " " };

/** A line's indent that holds nothing (blank, or only a comment), and one whose indent holds a tab. */
const EMPTY_LINE = -1;
const TAB_LINE = -2;

class Reader {
  readonly #lines: string[];
  /** Each line's indent, worked out once: `EMPTY_LINE`, `TAB_LINE`, or the count of its leading spaces. */
  readonly #indents: Int32Array;
  #i = 0;

  constructor(text: string) {
    const lines = text.split("\n");
    const indents = new Int32Array(lines.length);
    for (let k = 0; k < lines.length; k++) {
      let l = lines[k]!;
      if (l.charCodeAt(l.length - 1) === 13) lines[k] = l = l.slice(0, -1);
      let n = 0;
      while (l.charCodeAt(n) === 32) n++;
      const c = l.charCodeAt(n);
      if (n === l.length || c === 35 /* # */) indents[k] = EMPTY_LINE;
      else if (c === 9 /* tab */) {
        // A tab after the indent is only blank space when the rest is blank or a comment.
        const rest = l.slice(n).trimStart();
        indents[k] = rest === "" || rest.startsWith("#") ? EMPTY_LINE : TAB_LINE;
      } else indents[k] = n;
    }
    this.#lines = lines;
    this.#indents = indents;
  }

  /** Moves to the next line that holds something; its indent, or -1 at the end. */
  #next(): number {
    const indents = this.#indents;
    const end = indents.length;
    let i = this.#i;
    while (i < end) {
      const n = indents[i]!;
      if (n >= 0) {
        this.#i = i;
        return n;
      }
      if (n === TAB_LINE) throw UNSURE;
      i++;
    }
    this.#i = i;
    return -1;
  }

  document(): unknown {
    let n = this.#next();
    if (n === 0) {
      const l = this.#lines[this.#i]!;
      if (l === "---") {
        this.#i++;
        n = this.#next();
      } else if (l.startsWith("---") || l.startsWith("...") || l.startsWith("%")) throw UNSURE;
    }
    if (n < 0) throw UNSURE;
    const value = this.#block(n);
    if (this.#next() >= 0) throw UNSURE;
    if (typeof value !== "object" || value === null) throw UNSURE;
    return value;
  }

  /** The block (a mapping or a sequence) whose first line is the current one, at indent `n`. */
  #block(n: number): unknown {
    const c = this.#lines[this.#i]!.slice(n);
    if (c === "-" || c.startsWith("- ")) return this.#sequence(n);
    return this.#mapping(n, null);
  }

  /** A block mapping at indent `n`; `first` is its first entry's text where it starts after a sequence's `- `. */
  #mapping(n: number, first: string | null): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    let entry = first;
    for (;;) {
      if (entry === null) {
        const k = this.#next();
        if (k < 0 || k < n) break;
        if (k > n) throw UNSURE;
        entry = this.#lines[this.#i]!.slice(n);
        // A sequence at the mapping's own indent ends it (it was the value of the key before, read there).
        if (entry === "-" || entry.startsWith("- ")) break;
      }
      const { key, rest } = this.#key(entry);
      out[key] = this.#value(rest, n, true);
      entry = null;
    }
    return out;
  }

  /** An entry's key and what follows its colon. */
  #key(c: string): { key: string; rest: string } {
    const q = c.charCodeAt(0);
    let key: string;
    let after: number;
    if (q === 34 || q === 39) {
      const quoted = q === 34 ? doubleQuoted(c, 0) : singleQuoted(c, 0);
      key = quoted.value;
      if (c.charCodeAt(quoted.end) !== 58) throw UNSURE;
      after = quoted.end + 1;
      const s = c.charCodeAt(after);
      if (!(Number.isNaN(s) || s === 32)) throw UNSURE;
    } else {
      let at = c.indexOf(": ");
      if (at < 0) {
        if (c.charCodeAt(c.length - 1) !== 58) throw UNSURE;
        at = c.length - 1;
      }
      key = c.slice(0, at).trimEnd();
      if (key === "" || badStart(key.charCodeAt(0), key.charCodeAt(1)) || key.includes(" #") || key.charCodeAt(key.length - 1) === 58) throw UNSURE;
      // A plain key YAML reads as a number is written back as that number's string: `200` is `200`, `1.0` is `1`.
      const read = plain(key);
      if (typeof read === "number" && String(read) !== key) throw UNSURE;
      after = at + 1;
    }
    if (key === "__proto__" || key === "<<") throw UNSURE;
    return { key, rest: c.slice(after) };
  }

  /**
   * The value after a key's colon (`rest`) on the current line, the entry at indent `n`: inline, or the block under it.
   * `seqAtN`: a sequence at the key's own indent is its value (`tags:` then `- a` under it, not indented).
   */
  #value(rest: string, n: number, seqAtN: boolean): unknown {
    const r = rest.trimStart();
    if (r === "" || r.charCodeAt(0) === 35) {
      this.#i++;
      const k = this.#next();
      if (k > n) return this.#block(k);
      if (k === n && seqAtN) {
        const c = this.#lines[this.#i]!.slice(n);
        if (c === "-" || c.startsWith("- ")) return this.#sequence(n);
      }
      // An empty node is the empty string under the JSON schema, as js-yaml reads it.
      return "";
    }
    // A line under it deeper than its key (a scalar over several lines) is refused where the block reads on.
    return this.#inline(r, n);
  }

  /** A block sequence at indent `n`. */
  #sequence(n: number): unknown[] {
    const out: unknown[] = [];
    for (;;) {
      const k = this.#next();
      if (k < 0 || k < n) break;
      if (k > n) throw UNSURE;
      const c = this.#lines[this.#i]!.slice(n);
      if (!(c === "-" || c.startsWith("- "))) break;
      const r0 = c === "-" ? "" : c.slice(2);
      let s = 0;
      while (r0.charCodeAt(s) === 32) s++;
      const r = r0.slice(s);
      if (r === "" || r.charCodeAt(0) === 35) {
        this.#i++;
        const m = this.#next();
        out.push(m > n ? this.#block(m) : "");
        continue;
      }
      if (r === "-" || r.startsWith("- ")) throw UNSURE;
      if (isEntry(r)) {
        out.push(this.#mapping(n + 2 + s, r));
        continue;
      }
      const value = this.#inline(r, n);
      if (this.#next() > n) throw UNSURE;
      out.push(value);
    }
    return out;
  }

  /** A value written on the current line (a scalar, a flow, or a block scalar's header), for an entry at indent `n`. */
  #inline(r: string, n: number): unknown {
    const c = r.charCodeAt(0);
    if (c === 124 || c === 62) return this.#blockScalar(r, n);
    let value: unknown;
    let end: number;
    if (c === 34 || c === 39) {
      const q = c === 34 ? doubleQuoted(r, 0) : singleQuoted(r, 0);
      value = q.value;
      end = q.end;
    } else if (c === 91 || c === 123) {
      const f = new Flow(r);
      value = f.value();
      end = f.at;
    } else {
      if (badStart(c, r.charCodeAt(1))) throw UNSURE;
      const hash = r.indexOf(" #");
      const s = (hash < 0 ? r : r.slice(0, hash)).trimEnd();
      // A tab before a `#` starts a comment too, and `: ` inside a plain scalar is a mapping YAML refuses.
      if (s.includes(": ") || s.includes("\t") || s.charCodeAt(s.length - 1) === 58) throw UNSURE;
      this.#i++;
      return plain(s);
    }
    tail(r, end);
    this.#i++;
    return value;
  }

  /** A literal (`|`) or folded (`>`) block scalar whose header is `r`, under an entry at indent `n`. */
  #blockScalar(r: string, n: number): string {
    const header = r.trimEnd();
    if (!/^[|>][-+]?$/.test(header)) throw UNSURE;
    const folded = header.charCodeAt(0) === 62;
    const chomp = header.length > 1 ? header[1] : "";
    this.#i++;
    const lines: string[] = [];
    let m = -1;
    /** The most spaces a blank line before the first content line held: past the content's indent they are content. */
    let blankBefore = 0;
    while (this.#i < this.#lines.length) {
      const l = this.#lines[this.#i]!;
      let k = 0;
      while (l.charCodeAt(k) === 32) k++;
      if (k === l.length) {
        // A blank line: in the scalar, unless it holds more spaces than the content's indent.
        if (m >= 0 && k > m) throw UNSURE;
        if (m < 0) blankBefore = Math.max(blankBefore, k);
        lines.push("");
        this.#i++;
        continue;
      }
      if (l.charCodeAt(k) === 9) throw UNSURE;
      if (m < 0) {
        if (k <= n) break;
        m = k;
        // Blank lines before the content: more spaces than its indent are content, and a folded scalar starting
        // blank folds by rules this does not take on.
        if (blankBefore > m || (folded && lines.length > 0)) throw UNSURE;
      }
      if (k < m) {
        if (k > n) throw UNSURE;
        break;
      }
      const content = l.slice(m);
      if (folded && content.charCodeAt(0) === 32) throw UNSURE;
      lines.push(content);
      this.#i++;
    }
    // Trailing blank lines are the chomping's.
    let last = lines.length;
    while (last > 0 && lines[last - 1] === "") last--;
    const body = lines.slice(0, last);
    const trailing = lines.length - last;
    if (body.length === 0) return chomp === "+" ? "\n".repeat(trailing) : "";
    let text: string;
    if (folded) {
      text = "";
      for (let k = 0; k < body.length; k++) {
        const line = body[k]!;
        if (k === 0) text = line;
        else if (line === "") text += "\n";
        else text += body[k - 1] === "" ? line : ` ${line}`;
      }
    } else text = body.join("\n");
    if (chomp === "-") return text;
    if (chomp === "+") return `${text}\n${"\n".repeat(trailing)}`;
    return `${text}\n`;
  }
}

/** Whether `r` (after a sequence's `- `) is a mapping's first entry: `key: value` or `key:`. */
function isEntry(r: string): boolean {
  const c = r.charCodeAt(0);
  if (c === 34 || c === 39) {
    const q = c === 34 ? doubleQuoted(r, 0) : singleQuoted(r, 0);
    if (r.charCodeAt(q.end) !== 58) return false;
    const s = r.charCodeAt(q.end + 1);
    return Number.isNaN(s) || s === 32;
  }
  if (c === 91 || c === 123 || c === 124 || c === 62) return false;
  const hash = r.indexOf(" #");
  const s = hash < 0 ? r : r.slice(0, hash);
  return s.includes(": ") || s.trimEnd().endsWith(":");
}

/** What follows a value on its line must be nothing, or a comment after a space. */
function tail(r: string, end: number): void {
  const rest = r.slice(end);
  if (rest.trim() === "") return;
  if (/^\s+#/.test(rest)) return;
  throw UNSURE;
}

/** A double-quoted scalar starting at `at` on one line: its value and the index after its closing quote. */
function doubleQuoted(r: string, at: number): { value: string; end: number } {
  let out = "";
  let k = at + 1;
  for (;;) {
    const q = r.indexOf('"', k);
    const b = r.indexOf("\\", k);
    if (q < 0) throw UNSURE;
    if (b >= 0 && b < q) {
      out += r.slice(k, b);
      const e = r[b + 1];
      if (e === undefined) throw UNSURE;
      if (e === "u" || e === "x" || e === "U") {
        const len = e === "u" ? 4 : e === "x" ? 2 : 8;
        const hex = r.slice(b + 2, b + 2 + len);
        if (!new RegExp(`^[0-9a-fA-F]{${len}}$`).test(hex)) throw UNSURE;
        out += String.fromCodePoint(parseInt(hex, 16));
        k = b + 2 + len;
        continue;
      }
      const plainEscape = ESCAPES[e];
      if (plainEscape === undefined) throw UNSURE;
      out += plainEscape;
      k = b + 2;
      continue;
    }
    out += r.slice(k, q);
    return { value: out, end: q + 1 };
  }
}

/** A single-quoted scalar starting at `at` on one line (`''` is a quote). */
function singleQuoted(r: string, at: number): { value: string; end: number } {
  let out = "";
  let k = at + 1;
  for (;;) {
    const q = r.indexOf("'", k);
    if (q < 0) throw UNSURE;
    out += r.slice(k, q);
    if (r.charCodeAt(q + 1) === 39) {
      out += "'";
      k = q + 2;
      continue;
    }
    return { value: out, end: q + 1 };
  }
}

/** A flow collection on one line (`{ type: string }`, `[a, "b"]`). */
class Flow {
  at = 0;
  readonly r: string;
  constructor(r: string) {
    this.r = r;
  }

  #space() {
    while (this.r.charCodeAt(this.at) === 32) this.at++;
  }

  value(): unknown {
    this.#space();
    const c = this.r.charCodeAt(this.at);
    if (c === 123) return this.#map();
    if (c === 91) return this.#seq();
    if (c === 34 || c === 39) {
      const q = c === 34 ? doubleQuoted(this.r, this.at) : singleQuoted(this.r, this.at);
      this.at = q.end;
      return q.value;
    }
    return plain(this.#plain());
  }

  /** A plain scalar in a flow: up to `,`, `]`, `}`, or a `: `; never empty. */
  #plain(): string {
    const start = this.at;
    const c = this.r.charCodeAt(start);
    if (Number.isNaN(c) || badStart(c, this.r.charCodeAt(start + 1))) throw UNSURE;
    for (;;) {
      const ch = this.r.charCodeAt(this.at);
      if (Number.isNaN(ch) || ch === 44 || ch === 93 || ch === 125 || ch === 91 || ch === 123) break;
      if (ch === 58) {
        const nx = this.r.charCodeAt(this.at + 1);
        if (nx === 32 || nx === 44 || nx === 93 || nx === 125 || Number.isNaN(nx)) break;
      }
      if (ch === 35 && this.r.charCodeAt(this.at - 1) === 32) throw UNSURE;
      this.at++;
    }
    const s = this.r.slice(start, this.at).trimEnd();
    if (s === "") throw UNSURE;
    return s;
  }

  #map(): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    this.at++;
    this.#space();
    if (this.r.charCodeAt(this.at) === 125) {
      this.at++;
      return out;
    }
    for (;;) {
      this.#space();
      const c = this.r.charCodeAt(this.at);
      let key: string;
      if (c === 34 || c === 39) {
        const q = c === 34 ? doubleQuoted(this.r, this.at) : singleQuoted(this.r, this.at);
        this.at = q.end;
        key = q.value;
      } else {
        key = this.#plain();
        const read = plain(key);
        if (typeof read === "number" && String(read) !== key) throw UNSURE;
      }
      if (key === "__proto__" || key === "<<") throw UNSURE;
      this.#space();
      if (this.r.charCodeAt(this.at) !== 58) throw UNSURE;
      this.at++;
      const nx = this.r.charCodeAt(this.at);
      if (!(nx === 32 || c === 34 || c === 39)) throw UNSURE;
      this.#space();
      const v = this.r.charCodeAt(this.at);
      if (v === 44 || v === 125) throw UNSURE;
      out[key] = this.value();
      this.#space();
      const d = this.r.charCodeAt(this.at);
      this.at++;
      if (d === 125) return out;
      if (d !== 44) throw UNSURE;
      this.#space();
      if (this.r.charCodeAt(this.at) === 125) throw UNSURE;
    }
  }

  #seq(): unknown[] {
    const out: unknown[] = [];
    this.at++;
    this.#space();
    if (this.r.charCodeAt(this.at) === 93) {
      this.at++;
      return out;
    }
    for (;;) {
      out.push(this.value());
      this.#space();
      const d = this.r.charCodeAt(this.at);
      // `[a: b]` is a pair in YAML: unsure.
      if (d === 58) throw UNSURE;
      this.at++;
      if (d === 93) return out;
      if (d !== 44) throw UNSURE;
      this.#space();
      if (this.r.charCodeAt(this.at) === 93) throw UNSURE;
    }
  }
}
