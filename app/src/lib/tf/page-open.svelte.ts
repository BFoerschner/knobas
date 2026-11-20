/**
 * What the bare letter `O` presses on the page in front of the reader (ADR-0050, decisions §12): the page's own
 * *Open in ⟨source⟩*, which opens its site inside knobas when that source's pages open there, else the browser. `⇧O`
 * opens the browser instead. A bare letter, as `M` is on a run and `L` on the job: never in a field, never with a
 * dialog open, never on a page keep-alive holds hidden.
 *
 * The button registers while drawn ({@link openKey}, an attachment), the shell presses the newest one on the shown page,
 * and the button's title names the key. Built like `page-refresh.svelte.ts`.
 */
import type { Attachment } from "svelte/attachments";

import { inTextField } from "../shell/keys";
import { openWeb } from "../shell/open-web";

/** A drawn *Open in ⟨source⟩*. */
export interface OpenButton {
  el: Element;
  /** Open its address; `elsewhere` asks for the browser. */
  press: (elsewhere: boolean) => void;
}

export interface PageOpen {
  add(button: OpenButton): () => void;
  /** Press the newest button on the shown page; `false` when there is none. */
  press(elsewhere: boolean): boolean;
  reset(): void;
}

export function createPageOpen(): PageOpen {
  let drawn: OpenButton[] = [];
  return {
    add(button) {
      drawn = [...drawn, button];
      return () => {
        drawn = drawn.filter((candidate) => candidate !== button);
      };
    },
    press(elsewhere) {
      const shown = [...drawn].reverse().find((one) => one.el.isConnected && one.el.closest("[hidden], [inert]") === null);
      if (shown === undefined) return false;
      shown.press(elsewhere);
      return true;
    },
    reset() {
      drawn = [];
    },
  };
}

/** The window's one registry. */
export const pageOpen = createPageOpen();

/**
 * On an *Open in ⟨source⟩*: `O` presses it while it is drawn on the shown page. `at` reads the address and its
 * source when pressed.
 */
export function openKey(at: () => { url: string | null; source: string | null; item?: string | null }, registry: PageOpen = pageOpen): Attachment<Element> {
  return (el) =>
    registry.add({
      el,
      press: (elsewhere) => {
        const { url, source, item } = at();
        if (url) openWeb(url, { source, elsewhere, item: item ?? null });
      },
    });
}

/**
 * The shell's `O` and `⇧O`. The answer says whether the key was taken.
 */
export function onOpenKey(event: KeyboardEvent, overlayOpen: boolean, registry: PageOpen = pageOpen): boolean {
  if (event.key !== "o" && event.key !== "O") return false;
  if (event.metaKey || event.ctrlKey || event.altKey || event.repeat || event.defaultPrevented || overlayOpen) return false;
  if (inTextField(event.target)) return false;
  if (!registry.press(event.shiftKey)) return false;
  event.preventDefault();
  return true;
}
