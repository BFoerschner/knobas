/**
 * **Opening at the match** (the everything-search decisions, 2026-11-20, D12): what the launcher hands the page a row
 * opens, so the page's find bar opens on the words, at the passage the row quoted.
 *
 * The launcher sets it for the address it is about to go to; the page's find bar takes it when that page shows. It is
 * answered once, and only within {@link ARRIVAL_MS}: a page shown later, or another address, takes nothing. Reactive, so
 * a find bar whose page is already on screen sees one land.
 */
import type { Excerpt } from "../ipc";

export interface Arrival {
  /** The excerpt's marked texts, distinct, lower-cased. */
  words: string[];
  /** The excerpt's text, without its ellipses. */
  passage: string;
}

/** How long an arrival waits for its page. */
export const ARRIVAL_MS = 5_000;

let held = $state.raw<{ hash: string; arrival: Arrival; at: number } | null>(null);

/** Whether `here` is `hash`, or an address under it: a page may add its tab. */
const at = (hash: string, here: string): boolean => here === hash || here.startsWith(`${hash}/`) || here.startsWith(`${hash}?`);

export const arrival = {
  /** Hand `what` to the page at `hash`. */
  set(hash: string, what: Arrival): void {
    held = { hash, arrival: what, at: Date.now() };
  },
  /** The arrival for `hash`, once; `null` for another address, after {@link ARRIVAL_MS}, or once taken. */
  take(hash: string): Arrival | null {
    const now = held;
    if (now === null) return null;
    if (Date.now() - now.at > ARRIVAL_MS) {
      held = null;
      return null;
    }
    if (!at(now.hash, hash)) return null;
    held = null;
    return now.arrival;
  },
  /** Whether one is waiting for `hash`, read in an effect so it runs again when one lands. */
  waiting(hash: string): boolean {
    const now = held;
    return now !== null && at(now.hash, hash) && Date.now() - now.at <= ARRIVAL_MS;
  },
  /** Forget it (a test's reset). */
  clear(): void {
    held = null;
  },
};

/** What an excerpt hands its page: its marked words and its text. */
export function arrivalOf(excerpt: Pick<Excerpt, "segments">): Arrival {
  const words: string[] = [];
  for (const seg of excerpt.segments) {
    const word = seg.text.trim().toLowerCase();
    if (seg.hit && word !== "" && !words.includes(word)) words.push(word);
  }
  const passage = excerpt.segments
    .map((seg) => seg.text)
    .join("")
    .replace(/…/g, "")
    .trim();
  return { words, passage };
}

/** Letters and digits alone, lower-cased: how a passage and a drawn block are compared, whatever markup each kept. */
const bare = (text: string): string => text.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, "");

/** The length of the piece of the passage a block must hold. */
const PIECE = 24;

/**
 * The 24 characters of `passage` around its first marked word, bare: what a match's block must hold to be the one the
 * row quoted. The whole passage where it is shorter.
 */
export function pieceOf(what: Arrival): string {
  const lower = what.passage.toLowerCase();
  const first = what.words[0];
  const index = first === undefined ? -1 : lower.indexOf(first);
  const chars = Array.from(what.passage);
  if (chars.length <= PIECE || index < 0) return bare(what.passage);
  const mid = Array.from(what.passage.slice(0, index)).length + Math.floor(Array.from(first!).length / 2);
  const start = Math.max(0, Math.min(chars.length - PIECE, mid - PIECE / 2));
  return bare(chars.slice(start, start + PIECE).join(""));
}

/** Whether the drawn text `block` holds the passage's piece. */
export function holdsPiece(block: string, piece: string): boolean {
  return piece !== "" && bare(block).includes(piece);
}
