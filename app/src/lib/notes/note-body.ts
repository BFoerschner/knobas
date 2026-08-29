/**
 * Reading a note's markdown for the two things the editor draws: the chips,
 * and the `[[` being typed right now.
 *
 * Pure, and a plain `.ts` because nothing here is reactive.
 *
 * # The backend decides what a reference *is*
 *
 * There is a `[[…]]` scanner in Rust too (`knobas_core::note::parse_refs`), and
 * that one is the truth: it is what the save reconciles the note's links
 * against. A second scanner here that disagreed with it — over a stray bracket,
 * a line break, a length — would draw a chip for something that is not a link,
 * or leave a link with no chip, and neither is visible until somebody notices
 * the panel and the body saying different things.
 *
 * So {@link tokenise} does not decide. It finds the brackets, and then keeps a
 * span as a reference **only if the backend's own `refs` list names it**. What
 * the two scanners share is therefore just "where the brackets are", and the
 * question that matters — is this a reference — has one answer, computed once,
 * on the side that writes the links.
 *
 * # Rendering, not HTML
 *
 * The tokens are drawn by a Svelte `{#each}` (`NoteBody.svelte`), never by
 * turning the body into an HTML string: `{@html}` is forbidden app-wide
 * (`shell/house-rules.test.ts`, roadmap §4 gotcha 7) and a note is a document
 * the user typed, which is exactly the content that rule protects.
 */

import type { NoteRef } from "../ipc/entity";

/** A run of the body, as the editor draws it. */
export type NoteToken =
  | { kind: "text"; text: string }
  | {
      kind: "ref";
      /** The reference as it was written, brackets included. */
      text: string;
      /** What the backend resolved — or did not (`ref.target` is `null`). */
      ref: NoteRef;
    };

/** The `[[` the caret is currently inside, if it is inside one. */
export interface ActiveRef {
  /** Index of the `[[` that opened it. */
  start: number;
  /** Index just past the caret — what an accepted completion replaces up to. */
  end: number;
  /** What has been typed since the `[[`, trimmed. */
  query: string;
}

const OPEN = "[[";
const CLOSE = "]]";

/**
 * Split a body into text runs and reference chips.
 *
 * A `[[…]]` the backend's `refs` does not name is left as **text**, brackets
 * and all. That is the honest drawing of it: what the reader sees is what the
 * body says, and a span that is not a reference does not get to look like one.
 */
export function tokenise(body: string, refs: NoteRef[]): NoteToken[] {
  const named = new Map(refs.map((ref) => [ref.target_id, ref]));
  const tokens: NoteToken[] = [];
  let text = "";
  let at = 0;

  const flush = () => {
    if (text !== "") {
      tokens.push({ kind: "text", text });
      text = "";
    }
  };

  while (at < body.length) {
    const open = body.indexOf(OPEN, at);
    if (open === -1) break;
    const close = body.indexOf(CLOSE, open + OPEN.length);
    if (close === -1) break;

    const inner = body.slice(open + OPEN.length, close).trim();
    const ref = named.get(inner);
    if (ref) {
      text += body.slice(at, open);
      flush();
      tokens.push({ kind: "ref", text: body.slice(open, close + CLOSE.length), ref });
    } else {
      text += body.slice(at, close + CLOSE.length);
    }
    at = close + CLOSE.length;
  }

  text += body.slice(at);
  flush();
  return tokens;
}

/**
 * The `[[` the caret sits inside, or `null`.
 *
 * "Inside" means: the nearest `[[` before the caret has no `]]` between it and
 * the caret, and nothing between them is a line break. The line break is what
 * closes an abandoned `[[` — a person who typed one, thought better of it and
 * pressed Enter is not still completing a reference, and an autocomplete that
 * thought otherwise would follow them down the document.
 */
export function activeRef(body: string, caret: number): ActiveRef | null {
  const before = body.slice(0, caret);
  const start = before.lastIndexOf(OPEN);
  if (start === -1) return null;

  const typed = before.slice(start + OPEN.length);
  if (typed.includes(CLOSE) || typed.includes("\n")) return null;
  return { start, end: caret, query: typed.trim() };
}

/**
 * Replace the `[[` being typed with a finished reference to `targetId`.
 *
 * Returns the new body and where the caret belongs — just past the `]]`, so
 * typing continues after the chip rather than inside it.
 *
 * Any `]]` the user had already typed just after the caret is consumed, so
 * completing `[[pay]]` with the caret before the brackets does not leave
 * `[[mock:PAY-231]]]]`.
 */
export function insertRef(
  body: string,
  active: ActiveRef,
  targetId: string,
): { body: string; caret: number } {
  const rest = body.slice(active.end);
  const tail = rest.startsWith(CLOSE) ? rest.slice(CLOSE.length) : rest;
  const written = `${OPEN}${targetId}${CLOSE}`;
  return {
    body: body.slice(0, active.start) + written + tail,
    caret: active.start + written.length,
  };
}
