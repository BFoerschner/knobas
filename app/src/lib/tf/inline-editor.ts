/**
 * An editor drawn in the page, not in a dialog: a diff's line comment, a row that
 * turns into a field, the write queue's *Edit…*. `{@attach inlineEditor(close)}`
 * on its root owns the three things each caller used to do by hand, or not at all:
 *
 * - **focus in**: its first field, caret at the end of what is there, else its first
 *   control;
 * - **the layer**: a `panel` on the stack (`layers.svelte.ts`), so the shell's `Esc`
 *   reaches `close` (a text field blurs first, as everywhere: `escape.ts`). Inside a
 *   dialog it is a `dialog` layer, because the ladder reads dialogs before panels and
 *   the same press must close the editor, not what holds it;
 * - **focus back**: to `back()`, or to what had focus when it opened, when focus has
 *   nowhere else to be. A press that moved focus on purpose is left alone;
 * - **⌘Enter sends** (the key table): `send`, else the form or the primary inside it.
 *
 * `Esc` keeps its two steps here (a field blurs, then the editor closes) until the caller
 * keeps its words: `drafts.svelte.ts` holds them, and then `escCloses` makes it one press.
 *
 * `back` is for the usual shape, `{#if editing}<editor>{:else}<button>{/if}`: the
 * button that opened it is gone by the time this runs, and the one that comes back
 * is a new element, so the caller says how to find it. It is asked after the page
 * has redrawn.
 */
import { tick, untrack } from "svelte";
import type { Attachment } from "svelte/attachments";
import { on } from "svelte/events";

import { layers as sharedLayers, type Layers } from "./layers.svelte";

const FIELD = 'input:not([disabled], [type="radio"], [type="checkbox"], [type="hidden"]), textarea:not([disabled]), select:not([disabled])';
const CONTROL = 'a[href], button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])';

export interface InlineEditorOptions {
  /** Finds the control to return to, when what opened the editor is not there any more. */
  back?: () => HTMLElement | null;
  /** A selector for what takes focus, when the first field is not it. */
  focus?: string;
  /** What ⌘Enter does; without it, the editor's form submits, or else its primary is pressed. */
  send?: () => void;
  /** `close` keeps what was typed (`drafts.svelte.ts`), so one `Esc` closes the editor from inside a field. */
  escCloses?: boolean;
  layers?: Layers;
}

/**
 * ⌘Enter sends, in every composer (the key table). `{@attach sendKey(send)}` on one that is not an inline editor, or
 * `onkeydown={(event) => { if (isSendKey(event)) send(); }}` where the composer has its own key handler.
 */
export function isSendKey(event: KeyboardEvent): boolean {
  return event.key === "Enter" && (event.metaKey || event.ctrlKey) && !event.isComposing && !event.defaultPrevented;
}

/** What ⌘Enter presses in `node` when the caller doesn't say: its form, else its primary button. */
function sendFrom(node: HTMLElement): void {
  const form = node instanceof HTMLFormElement ? node : node.querySelector("form");
  const primary = node.querySelector<HTMLElement>('.tf-primary:not([disabled], [aria-disabled="true"])');
  if (primary !== null) primary.click();
  else form?.requestSubmit();
}

function listenForSend(node: HTMLElement, send: (() => void) | undefined): () => void {
  return on(node, "keydown", (event: KeyboardEvent) => {
    if (!isSendKey(event)) return;
    event.preventDefault();
    if (send !== undefined) send();
    else sendFrom(node);
  });
}

/** ⌘Enter on `node` does `send`, or presses its form or primary. */
export function sendKey(send?: () => void): Attachment<HTMLElement> {
  return (node) => listenForSend(node, send);
}

/** Focus is nowhere a reader put it: on the body, or on something the redraw took away. */
function lost(): boolean {
  const active = document.activeElement;
  return active === null || active === document.body || !active.isConnected;
}

export function inlineEditor(close: () => void, options: InlineEditorOptions = {}): Attachment<HTMLElement> {
  return (node) =>
    untrack(() => {
      const before = document.activeElement;
      const first =
        (options.focus ? node.querySelector<HTMLElement>(options.focus) : null) ?? node.querySelector<HTMLElement>(FIELD) ?? node.querySelector<HTMLElement>(CONTROL) ?? null;
      first?.focus();
      if ((first instanceof HTMLTextAreaElement || (first instanceof HTMLInputElement && first.type === "text")) && first.value !== "") {
        first.setSelectionRange(first.value.length, first.value.length);
      }
      const stack = options.layers ?? sharedLayers;
      const off = stack.add({ kind: node.closest(".tf-dialog") ? "dialog" : "panel", close });
      const offSend = listenForSend(node, options.send);
      const offEsc = options.escCloses
        ? on(node, "keydown", (event: KeyboardEvent) => {
            if (event.key !== "Escape" || event.defaultPrevented || event.isComposing) return;
            event.preventDefault();
            event.stopPropagation();
            close();
          })
        : () => {};
      return () => {
        offSend();
        offEsc();
        untrack(off);
        // Asked now: a dialog around it picks up fallen focus before the redraw is done, and that isn't a reader's move.
        if (!lost() && !node.contains(document.activeElement)) return;
        const under = stack.open.at(-1);
        void tick().then(() => {
          // Something opened on the same press, and it has focus now.
          if (stack.open.at(-1) !== under) return;
          const opener = before instanceof HTMLElement && before !== document.body && before.isConnected ? before : null;
          // `back` first: a pressed button doesn't take focus in every webview, so `before` may be some other control.
          (options.back?.() ?? opener)?.focus({ preventScroll: true });
        });
      };
    });
}
