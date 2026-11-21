/**
 * What an engine is to the highlight worker (the highlighter re-ruling, 2026-11-21, section 3): a way to tokenize a text
 * in a language, answering the port's `[text, role]` lines in 400-line slices. Each engine is a module of its own, the
 * only file that imports its package, imported the first time a job needs it (`highlight-core.ts`).
 */
import type { LineTokens, Token } from "../port";
import type { TokenRole } from "../roles";
import type { LangId } from "../languages";

/** Lines per slice the port is answered in, and per step of a pass from the top. */
export const CHUNK = 400;
/** A line this long or longer is one plain token. */
export const LONGEST_LINE = 4000;

export interface RunContext {
  /** Answers lines `from`… of the text. */
  post(from: number, lines: LineTokens[]): void;
  /** Yields to the worker's queue, so a cancel or a new focus is read. */
  pause(): Promise<void>;
  cancelled(): boolean;
  /** The lines in view the viewer last named and that are not yet answered, taken. */
  takeFocus(): [from: number, to: number] | null;
}

export interface Engine {
  /** Tokenizes `text`, whose language is `lang` and which this engine calls `name`. Throws where it cannot. */
  run(text: string, name: string, lang: LangId, ctx: RunContext): Promise<void>;
}

/** A line's runs as the port's tokens: each run's role, neighbours of one role joined. */
export function joined(runs: Iterable<readonly [text: string, role: TokenRole]>): Token[] {
  const out: Token[] = [];
  for (const [text, role] of runs) {
    if (text === "") continue;
    const last = out[out.length - 1];
    if (last && last[1] === role) out[out.length - 1] = [last[0] + text, role];
    else out.push([text, role]);
  }
  return out;
}

/**
 * Answers lines tokenized whole in slices: the one holding the focus first, the rest from the top, with no pause
 * between, since nothing is left to compute. Stops at a cancel.
 */
export function postSlices(lines: LineTokens[], ctx: RunContext, base = 0): void {
  const focus = ctx.takeFocus();
  const sent = new Set<number>();
  if (focus !== null) {
    const at = Math.max(0, focus[0] - base);
    const start = at - (at % CHUNK);
    if (start < lines.length) {
      ctx.post(base + start, lines.slice(start, start + CHUNK));
      sent.add(start);
    }
  }
  for (let start = 0; start < lines.length && !ctx.cancelled(); start += CHUNK) {
    if (!sent.has(start)) ctx.post(base + start, lines.slice(start, start + CHUNK));
  }
}
