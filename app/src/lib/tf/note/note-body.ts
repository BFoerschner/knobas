/**
 * A note's body as the read-only page draws it (#1504): text runs, and the
 * `[[refs]]` the backend resolved.
 *
 * The rooms UI's `notes/note-body.ts` `tokenise`, brought back for the page
 * (#1087 removed it with the editor). It does not decide what a reference is:
 * `knobas_core::note::parse_refs` does, and the note read hands its answer
 * over as `refs`. This finds the brackets and keeps a span as a reference only
 * when `refs` names it, so the page and the links never disagree over a stray
 * bracket. The tokens are drawn by an `{#each}`, never as HTML: the body is
 * text the reader typed.
 */
import type { NoteRef } from "../../ipc/entity";

/** A run of the body. */
export type NoteToken =
  | { kind: "text"; text: string }
  | {
      kind: "ref";
      /** The reference as it was written, brackets included. */
      text: string;
      /** What the backend resolved, or did not (`ref.target` is `null`). */
      ref: NoteRef;
    };

const OPEN = "[[";
const CLOSE = "]]";

/**
 * Split a body into text runs and references. A `[[…]]` the backend's `refs`
 * does not name stays text, brackets and all.
 */
export function tokenise(body: string, refs: readonly NoteRef[]): NoteToken[] {
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

    const ref = named.get(body.slice(open + OPEN.length, close).trim());
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
