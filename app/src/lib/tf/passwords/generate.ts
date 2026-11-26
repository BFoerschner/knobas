/**
 * The password generator (ADR-0056), in the window on `crypto.getRandomValues`. Every pick is unbiased: a value is
 * drawn by rejection sampling, never `% n`, which favours the low values. *Characters*: a length of 8 to 64 from up to
 * four sets, look-alikes left out on request, one of each chosen set guaranteed. *Words*: 3 to 8 words of the EFF's
 * long list (12.9 bits each), a separator, capitals, a number.
 */

/** A source of random 32-bit words: `crypto.getRandomValues` in the app, a fixed stream in tests. */
export type Random = (into: Uint32Array) => Uint32Array;

const cryptoRandom: Random = (into) => crypto.getRandomValues(into);

/** An unbiased integer in `[0, n)`: values past the last whole multiple of `n` are drawn again. */
export function below(n: number, random: Random = cryptoRandom): number {
  if (!Number.isInteger(n) || n <= 0 || n > 2 ** 32) throw new RangeError(`below(${n})`);
  const limit = Math.floor(2 ** 32 / n) * n;
  const one = new Uint32Array(1);
  for (;;) {
    const v = random(one)[0]!;
    if (v < limit) return v % n;
  }
}

export const SETS = {
  lower: "abcdefghijklmnopqrstuvwxyz",
  upper: "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
  digits: "0123456789",
  symbols: "!#$%&*+-=?@^_~.:;",
} as const;

/** Characters that read alike: 0 O o, 1 l I |. */
const LOOK_ALIKE = new Set([..."0Oo1lI|"]);

export interface CharOptions {
  length: number;
  lower: boolean;
  upper: boolean;
  digits: boolean;
  symbols: boolean;
  /** Leave out 0 O o 1 l I. */
  plain: boolean;
}

export const DEFAULT_CHARS: CharOptions = { length: 20, lower: true, upper: true, digits: true, symbols: true, plain: true };

function pools(o: CharOptions): string[] {
  const chosen = (["lower", "upper", "digits", "symbols"] as const).filter((k) => o[k]).map((k) => SETS[k]);
  const sets = chosen.length > 0 ? chosen : [SETS.lower];
  return sets.map((set) => [...set].filter((c) => !o.plain || !LOOK_ALIKE.has(c)).join(""));
}

/** A password of characters, with at least one from each chosen set. */
export function characters(o: CharOptions, random: Random = cryptoRandom): string {
  const length = Math.max(8, Math.min(64, Math.round(o.length)));
  const sets = pools(o);
  const all = sets.join("");
  const out = Array.from({ length }, () => all[below(all.length, random)]!);
  // One of each chosen set, at distinct places, so no set is left out by chance.
  const places = Array.from({ length }, (_, i) => i);
  for (const set of sets) {
    const at = places.splice(below(places.length, random), 1)[0]!;
    out[at] = set[below(set.length, random)]!;
  }
  return out.join("");
}

export interface WordOptions {
  words: number;
  separator: string;
  capitals: boolean;
  number: boolean;
}

export const DEFAULT_WORDS: WordOptions = { words: 5, separator: "-", capitals: true, number: true };

/** A passphrase of `list`'s words. */
export function words(o: WordOptions, list: readonly string[], random: Random = cryptoRandom): string {
  const n = Math.max(3, Math.min(8, Math.round(o.words)));
  const picked = Array.from({ length: n }, () => {
    const w = list[below(list.length, random)]!;
    return o.capitals ? w.charAt(0).toUpperCase() + w.slice(1) : w;
  });
  if (o.number) {
    const at = below(n, random);
    picked[at] = `${picked[at]}${below(10, random)}`;
  }
  return picked.join(o.separator);
}

/** About how many bits of strength: `characters` from its pool, `words` at 12.9 a word. */
export function strength(mode: "chars" | "words", o: CharOptions | WordOptions, listSize = 7776): number {
  if (mode === "words") {
    const w = o as WordOptions;
    return Math.round(w.words * Math.log2(listSize) + (w.number ? Math.log2(10) + Math.log2(w.words) : 0));
  }
  const c = o as CharOptions;
  const size = pools(c).join("").length;
  return Math.round(Math.max(8, Math.min(64, c.length)) * Math.log2(size));
}

let list: string[] | null = null;

/** The EFF's long list, loaded the first time *Words* is chosen. */
export async function wordList(): Promise<string[]> {
  if (list !== null) return list;
  const text = (await import("./eff-large-wordlist.txt?raw")).default;
  list = text.split("\n").map((w) => w.trim()).filter(Boolean);
  return list;
}
