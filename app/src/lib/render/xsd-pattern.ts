/**
 * XML Schema's patterns, read by knobas (the XSD design, 2026-11-23, section 21). Not JavaScript's: a pattern matches
 * the whole value, `^` and `$` are ordinary characters, `\w` leaves out the underscore, `\i` and `\c` are XML's name
 * characters, `\p{IsGreek}` names a block, and `[a-z-[aeiou]]` takes a class from a class. So a pattern is parsed here,
 * matched under a step budget ({@link STEP_BUDGET}), and a string is made from what was parsed. A pattern from somebody
 * else's file is never handed to the engine's `RegExp`, which backtracks without a bound.
 */

/** How many steps a match may take before its answer is "not known". */
export const STEP_BUDGET = 200_000;

const BLOCKS: Record<string, readonly [number, number]> = {
  BasicLatin: [0x0000, 0x007f],
  "Latin-1Supplement": [0x0080, 0x00ff],
  "LatinExtended-A": [0x0100, 0x017f],
  "LatinExtended-B": [0x0180, 0x024f],
  IPAExtensions: [0x0250, 0x02af],
  Greek: [0x0370, 0x03ff],
  Cyrillic: [0x0400, 0x04ff],
  Hebrew: [0x0590, 0x05ff],
  Arabic: [0x0600, 0x06ff],
  GeneralPunctuation: [0x2000, 0x206f],
  CurrencySymbols: [0x20a0, 0x20cf],
  Hiragana: [0x3040, 0x309f],
  Katakana: [0x30a0, 0x30ff],
  CJKUnifiedIdeographs: [0x4e00, 0x9fff],
  HangulSyllables: [0xac00, 0xd7a3],
};

const categories = new Map<string, RegExp | null>();
/** Whether `ch` is in Unicode category `cat`: the engine's own tables, through a fixed `\p{…}` of a known name. */
function inCategory(cat: string, ch: string): boolean {
  let re = categories.get(cat);
  if (re === undefined) {
    try {
      re = new RegExp(`^\\p{${cat}}$`, "u");
    } catch {
      re = null;
    }
    categories.set(cat, re);
  }
  return re !== null && re.test(ch);
}
const NAME_START = /^[:A-Z_a-z\u00C0-\u00D6\u00D8-\u00F6\u00F8-\u02FF\u0370-\u037D\u037F-\u1FFF\u200C\u200D\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD]$/;
const NAME_CHAR = /^[-.0-9:A-Z_a-z\u00B7\u00C0-\u00D6\u00D8-\u00F6\u00F8-\u037D\u037F-\u1FFF\u200C\u200D\u203F\u2040\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD]$/;

export type SetItem = { lo: number; hi: number } | { cls: string } | { cat: string; neg: boolean } | { block: string; neg: boolean };
export interface CharSet {
  neg: boolean;
  items: SetItem[];
  sub: CharSet | null;
}
export type Atom = { lit: string } | { set: CharSet } | { group: Alt };
export interface Piece {
  atom: Atom;
  min: number;
  max: number;
}
export interface Alt {
  branches: Piece[][];
}

/** Whether `ch` (one code point) is in `set`. */
export function inSet(set: CharSet, ch: string): boolean {
  const cp = ch.codePointAt(0)!;
  let hit = false;
  for (const it of set.items) {
    if ("lo" in it) hit = cp >= it.lo && cp <= it.hi;
    else if ("cls" in it) {
      const c = it.cls;
      const lower = c.toLowerCase();
      let v: boolean;
      if (lower === "d") v = inCategory("Nd", ch);
      else if (lower === "s") v = ch === " " || ch === "\t" || ch === "\n" || ch === "\r";
      // `\w`: everything but punctuation, separators and controls. The underscore is punctuation.
      else if (lower === "w") v = !(inCategory("P", ch) || inCategory("Z", ch) || inCategory("C", ch));
      else if (lower === "i") v = NAME_START.test(ch);
      else if (lower === "c") v = NAME_CHAR.test(ch);
      else v = ch !== "\n" && ch !== "\r";
      hit = c === lower ? v : !v;
    } else if ("cat" in it) hit = inCategory(it.cat, ch) !== it.neg;
    else {
      const b = BLOCKS[it.block];
      hit = (b !== undefined && cp >= b[0] && cp <= b[1]) !== it.neg;
    }
    if (hit) break;
  }
  if (set.neg) hit = !hit;
  if (hit && set.sub !== null) hit = !inSet(set.sub, ch);
  return hit;
}

class Bad extends Error {}

/** Reads a pattern: `{ ok: true, re }`, or `{ ok: false, why }` where knobas does not read it. */
export function rxParse(src: string): { ok: true; re: Alt } | { ok: false; why: string } {
  let i = 0;
  const cps = Array.from(src);
  const n = cps.length;
  const bad = (why: string): never => {
    throw new Bad(why);
  };
  const esc = (): { set: CharSet } | { ch: string } => {
    const c = cps[i++];
    if (c === undefined) return bad("it ends in a backslash");
    if ("dDsSwWiIcC".includes(c)) return { set: { neg: false, items: [{ cls: c }], sub: null } };
    if (c === "p" || c === "P") {
      if (cps[i] !== "{") bad("\\p needs a name in braces");
      const close = cps.indexOf("}", i);
      if (close === -1) bad("\\p{ is not closed");
      const name = cps.slice(i + 1, close).join("");
      i = close + 1;
      if (name.startsWith("Is")) {
        if (BLOCKS[name.slice(2)] === undefined) bad(`knobas does not know the block ${name.slice(2)}`);
        return { set: { neg: false, items: [{ block: name.slice(2), neg: c === "P" }], sub: null } };
      }
      if (!/^[A-Z][a-z]?$/.test(name)) bad(`${name} is no character category`);
      return { set: { neg: false, items: [{ cat: name, neg: c === "P" }], sub: null } };
    }
    const single = ({ n: "\n", r: "\r", t: "\t" } as Record<string, string>)[c];
    if (single !== undefined) return { ch: single };
    if ("\\|.-^?*+{}()[]".includes(c)) return { ch: c };
    return bad(`\\${c} is not an escape XML Schema knows`);
  };
  const charClass = (): CharSet => {
    const set: CharSet = { neg: false, items: [], sub: null };
    if (cps[i] === "^") {
      set.neg = true;
      i += 1;
    }
    let first = true;
    for (;;) {
      const c = cps[i];
      if (c === undefined) return bad("a [ is not closed");
      if (c === "]" && !first) {
        i += 1;
        return set;
      }
      first = false;
      if (c === "-" && cps[i + 1] === "[") {
        i += 2;
        set.sub = charClass();
        if (cps[i] !== "]") bad("a class taken from a class must end the class");
        i += 1;
        return set;
      }
      let lo: string;
      if (c === "\\") {
        i += 1;
        const e = esc();
        if ("set" in e) {
          set.items.push(...e.set.items);
          continue;
        }
        lo = e.ch;
      } else {
        if (c === "[") bad("a [ inside a class must be written \\[");
        lo = c;
        i += 1;
      }
      if (cps[i] === "-" && cps[i + 1] !== "]" && cps[i + 1] !== "[" && cps[i + 1] !== undefined) {
        i += 1;
        let hi = cps[i++]!;
        if (hi === "\\") {
          const e = esc();
          if ("set" in e) bad("a range cannot end in a class");
          hi = (e as { ch: string }).ch;
        }
        if (hi.codePointAt(0)! < lo.codePointAt(0)!) bad(`the range ${lo}-${hi} runs backwards`);
        set.items.push({ lo: lo.codePointAt(0)!, hi: hi.codePointAt(0)! });
      } else set.items.push({ lo: lo.codePointAt(0)!, hi: lo.codePointAt(0)! });
    }
  };
  const seq = (): Piece[] => {
    const pieces: Piece[] = [];
    while (i < n && cps[i] !== "|" && cps[i] !== ")") {
      let atom: Atom;
      const c = cps[i++]!;
      if (c === "(") {
        atom = { group: alt() };
        if (cps[i] !== ")") bad("a ( is not closed");
        i += 1;
      } else if (c === "[") atom = { set: charClass() };
      else if (c === ".") atom = { set: { neg: false, items: [{ cls: "." }], sub: null } };
      else if (c === "\\") {
        const e = esc();
        atom = "set" in e ? { set: e.set } : { lit: e.ch };
      } else if ("?*+{}]".includes(c)) return bad(`a ${c} stands where a character is expected`);
      else atom = { lit: c };
      let min = 1;
      let max = 1;
      const q = cps[i];
      if (q === "?") {
        min = 0;
        i += 1;
      } else if (q === "*") {
        min = 0;
        max = Infinity;
        i += 1;
      } else if (q === "+") {
        max = Infinity;
        i += 1;
      } else if (q === "{") {
        const close = cps.indexOf("}", i);
        const m = close === -1 ? null : /^(\d+)(?:(,)(\d*))?$/.exec(cps.slice(i + 1, close).join(""));
        if (m === null) return bad("a { count } is not written as XML Schema writes it");
        min = Number(m[1]);
        max = m[2] === undefined ? min : m[3] === "" ? Infinity : Number(m[3]);
        if (max < min) bad("a count's upper bound is under its lower");
        i = close + 1;
      }
      pieces.push({ atom, min, max });
    }
    return pieces;
  };
  const alt = (): Alt => {
    const branches = [seq()];
    while (cps[i] === "|") {
      i += 1;
      branches.push(seq());
    }
    return { branches };
  };
  try {
    const re = alt();
    if (i < n) bad("a ) closes nothing");
    return { ok: true, re };
  } catch (e) {
    if (e instanceof Bad) return { ok: false, why: e.message };
    throw e;
  }
}

const OUT = Symbol("budget");
/** Whether the whole of `text` matches: `null` where the budget ran out and the answer is not known. */
export function rxTest(re: Alt, text: string, budget = STEP_BUDGET): boolean | null {
  const cps = Array.from(text);
  let steps = budget;
  const mAlt = (a: Alt, pos: number, k: (p: number) => boolean): boolean => {
    for (const b of a.branches) if (mSeq(b, 0, pos, k)) return true;
    return false;
  };
  const mSeq = (pieces: Piece[], at: number, pos: number, k: (p: number) => boolean): boolean => {
    if (--steps < 0) throw OUT;
    if (at === pieces.length) return k(pos);
    const p = pieces[at]!;
    return mRep(p, 0, pos, (q) => mSeq(pieces, at + 1, q, k));
  };
  const mRep = (p: Piece, count: number, pos: number, k: (p: number) => boolean): boolean => {
    if (--steps < 0) throw OUT;
    if (count < p.max && mAtom(p.atom, pos, (q) => (q === pos && count >= p.min ? false : mRep(p, count + 1, q, k)))) return true;
    return count >= p.min && k(pos);
  };
  const mAtom = (atom: Atom, pos: number, k: (p: number) => boolean): boolean => {
    if ("group" in atom) return mAlt(atom.group, pos, k);
    if (pos >= cps.length) return false;
    if ("lit" in atom) return cps[pos] === atom.lit && k(pos + 1);
    return inSet(atom.set, cps[pos]!) && k(pos + 1);
  };
  try {
    return mAlt(re, 0, (pos) => pos === cps.length);
  } catch (e) {
    if (e === OUT) return null;
    throw e;
  }
}

/** The characters a made string draws from, the plain ones first. */
const ALPHA = Array.from("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
const OTHER = Array.from(" -_.,:/+@#()'&=%!?*;<>[]{}|~^$\"\\äöüßéÄÖÜ\t");
const alphabets = new WeakMap<CharSet, string[]>();
function alphabetOf(set: CharSet): string[] {
  let got = alphabets.get(set);
  if (got === undefined) {
    got = ALPHA.filter((c) => inSet(set, c));
    // Nothing plain fits: the first other character that does (a space for `\s`), so a made string stays calm.
    if (got.length === 0) got = OTHER.filter((c) => inSet(set, c)).slice(0, 1);
    alphabets.set(set, got);
  }
  return got;
}

/** Whether a branch is plain words: only literal characters. Such a branch is what a person would write. */
function literalBranch(pieces: Piece[]): boolean {
  return pieces.length > 0 && pieces.every((p) => "lit" in p.atom && p.min === 1 && p.max === 1);
}
/** Whether a pattern offers plain words to choose from at its top: `gros_list|net_list|udp_\w{1,16}`. */
export const rxHasWords = (re: Alt): boolean => re.branches.some(literalBranch);

const NONE = Symbol("none");
/**
 * A string the pattern matches, made with `rnd` (0 ≤ x < 1); `null` where none could be made. Among plain words, one
 * `prefer` likes (what the holder's name asks for), else a seeded one; counts are short.
 */
export function rxMake(re: Alt, rnd: () => number, prefer?: (word: string) => boolean): string | null {
  const gAlt = (a: Alt): string => {
    const literal = a.branches.filter(literalBranch);
    let from = literal.length > 0 ? literal : a.branches;
    if (prefer !== undefined && literal.length > 1) {
      const liked = literal.filter((b) => prefer(b.map((p) => (p.atom as { lit: string }).lit).join("")));
      if (liked.length > 0) from = liked;
    }
    return gSeq(from[Math.floor(rnd() * from.length)]!);
  };
  const gSeq = (pieces: Piece[]): string => {
    let out = "";
    for (const p of pieces) {
      const group = "group" in p.atom;
      const span = Math.min(p.max, p.min + (group ? 1 : p.max === Infinity ? 5 : 7)) - p.min;
      let times = p.min + Math.floor(rnd() * (span + 1));
      // `\w{1,50}` reads as a word.
      if (!group && p.max > 8 && p.min <= 4) times = Math.max(times, Math.min(p.max, 4));
      for (let t = 0; t < times; t += 1) {
        if ("group" in p.atom) out += gAlt(p.atom.group);
        else if ("lit" in p.atom) out += p.atom.lit;
        else {
          const from = alphabetOf(p.atom.set);
          if (from.length === 0) throw NONE;
          out += from[Math.floor(rnd() * from.length)]!;
        }
      }
    }
    return out;
  };
  try {
    for (let tries = 0; tries < 8; tries += 1) {
      const made = gAlt(re);
      if (rxTest(re, made) === true) return made;
    }
    return null;
  } catch (e) {
    if (e === NONE) return null;
    throw e;
  }
}

const parsed = new Map<string, ReturnType<typeof rxParse>>();
/** A pattern read once. */
export function rx(src: string): ReturnType<typeof rxParse> {
  let got = parsed.get(src);
  if (got === undefined) {
    if (parsed.size > 4000) parsed.clear();
    parsed.set(src, (got = rxParse(src)));
  }
  return got;
}
