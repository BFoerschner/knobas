/**
 * A toast's *Undo* on a link set. The toast outlives the editor that raised it: the reader may be on another set by
 * then, on another page, or back on this set in a new editor. So *Undo* goes by the set's id: through the editor showing
 * that set, whose saves run in order and which draws the answer, else straight to the set as the backend holds it.
 */
import { ipcErrorMessage } from "../../ipc";
import type { LinkSetDraft } from "../../ipc/entity";
import { push } from "../../shell/toasts.svelte";
import { holdStillPress } from "../still-press";
import { draftOf } from "./apply";
import type { SetPorts } from "./ports";

type Change = (draft: LinkSetDraft) => void;

const shown = new Map<string, (change: Change) => Promise<void>>();

/** An editor draws this set, and saves through `save`. Returns what takes it back. */
export function editorShows(id: string, save: (change: Change) => Promise<void>): () => void {
  shown.set(id, save);
  return () => {
    if (shown.get(id) === save) shown.delete(id);
  };
}

export function undoOn(id: string, change: Change, ports: Pick<SetPorts, "linkSet" | "saveLinkSet">): void {
  holdStillPress();
  const editor = shown.get(id);
  if (editor !== undefined) {
    void editor(change);
    return;
  }
  void ports
    .linkSet(id)
    .then((held) => {
      const draft = draftOf(held);
      change(draft);
      return ports.saveLinkSet(draft);
    })
    .catch((cause: unknown) => push({ text: `Could not undo it: ${ipcErrorMessage(cause)}`, tone: "err" }));
}
