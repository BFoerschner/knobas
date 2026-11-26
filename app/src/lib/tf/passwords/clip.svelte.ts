/**
 * What the clipboard holds for knobas now (ADR-0056): the Rust side's `clipboard:state`, kept for the status bar's
 * line and the entry's rows. Started once by the shell; a page asks nothing.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../../ipc";
import { clipboardState, type ClipState } from "../../ipc/passwords";

const EMPTY: ClipState = { what: null, entity_id: null, until: null };

const held = $state<{ now: ClipState }>({ now: EMPTY });

/** What the clipboard holds for knobas: what, whose, until when. */
export function clip(): ClipState {
  return held.now;
}

/** Set it from a command's answer, before its event arrives, so the row says so in the same frame. */
export function clipped(state: ClipState): void {
  held.now = state;
}

let started = false;

/** Listen for `clipboard:state`, once; and read what it holds now. */
export function startClip(listen = tauriListen, read = clipboardState): () => void {
  if (started) return () => {};
  started = true;
  let off: (() => void) | null = null;
  void listen(EVENTS.clipboardState, ({ payload }) => {
    held.now = (payload as ClipState | null) ?? EMPTY;
  }).then((unlisten) => {
    off = unlisten;
  });
  void read()
    .then((state) => {
      held.now = state;
    })
    .catch(() => {});
  return () => {
    started = false;
    off?.();
  };
}

/** Tests only. */
export function resetClip(): void {
  held.now = EMPTY;
  started = false;
}
