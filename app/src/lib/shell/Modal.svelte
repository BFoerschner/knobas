<!--
  A focus-trapped dialog over a scrim, drawn as the ticket-first dialog
  (`.tf-modal` / `.tf-dialog` in `dialogs.css`, the same sheet, header and
  canonical footer `TfDialog` draws; #2292). Its header adds a Close icon button.

  Deliberately not a native `<dialog>`: `showModal()` puts the element in the
  top layer, where `::backdrop` — not `.tf-modal` — draws the dim, and the
  sheet's geometry (the drop from the top) would have to be rebuilt against a
  pseudo-element this stylesheet does not style. What a native dialog *gives* is
  focus trapping, Escape and focus restore, so those three are written out below
  and tested; nothing else about `showModal` was wanted.

  It sits on the ticket-first shell's layer stack while open, as `TfDialog`
  does (#1527): the window's `Esc` closes it when focus is outside it, rather
  than navigating the page behind, and history stays put under it (#1086).
-->
<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  import { layers as sharedLayers, type Layers } from "../tf/layers.svelte";

  let {
    title,
    subtitle,
    wide = false,
    onclose,
    layers = sharedLayers,
    body,
    footer,
  }: {
    title: string;
    subtitle?: string;
    /** The 820 px variant the import dialog uses. */
    wide?: boolean;
    onclose: () => void;
    /** The stack the shell's `Esc` and history read; tests hand in their own. */
    layers?: Layers;
    body?: Snippet;
    footer?: Snippet;
  } = $props();

  /** Unique per instance, so two stacked dialogs do not share a label id. */
  const titleId = `dlg-title-${Math.random().toString(36).slice(2, 9)}`;

  let dialog = $state<HTMLDivElement | null>(null);

  const FOCUSABLE =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  /** Everything inside the dialog a Tab can reach, in document order. */
  function focusable(): HTMLElement[] {
    if (!dialog) return [];
    return [...dialog.querySelectorAll<HTMLElement>(FOCUSABLE)];
  }

  $effect(() => {
    // Captured before the move, and restored on teardown: after a dialog
    // closes the reader must be back on the control that opened it, not at the
    // top of the document.
    const restoreTo = document.activeElement;
    // Untracked: `add` reads the stack it writes, and this effect must run once.
    const off = untrack(() => layers.add({ kind: "dialog", close: () => onclose() }));
    // The body's first control, not the dialog's: the header's Close comes
    // first in document order, and a reader who opened a dialog to type would
    // type into nothing (#1431). Close only when the body offers nothing.
    (dialog?.querySelector<HTMLElement>(`.tf-body :is(${FOCUSABLE})`) ?? focusable()[0])?.focus();
    return () => {
      untrack(off);
      if (restoreTo instanceof HTMLElement && document.contains(restoreTo)) {
        restoreTo.focus();
      }
    };
  });

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      // Rung 1 of the Esc ladder (`keys.ts`): the shell's global handler
      // unwinds the detail slide-over, and it must not do that as well as
      // closing this dialog on one keypress.
      event.stopPropagation();
      onclose();
      return;
    }
    if (event.key !== "Tab") return;

    const items = focusable();
    if (items.length === 0) return;
    const first = items[0]!;
    const last = items[items.length - 1]!;
    // Only the two ends are handled: everywhere else the browser's own order
    // is already correct, and re-implementing it would get it wrong.
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

<!--
  The scrim closes on a click that started *and* ended on itself, so a drag
  that began inside the dialog and released over the scrim does not shut it.
  `role="presentation"` and no key handler of its own: Escape is the keyboard
  equivalent and lives on the dialog, so this is not a control.
-->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="tf-modal"
  onclick={(event) => {
    if (event.target === event.currentTarget) onclose();
  }}
>
  <div
    class="tf-dialog"
    class:wide
    bind:this={dialog}
    role="dialog"
    aria-modal="true"
    aria-labelledby={titleId}
    tabindex="-1"
    {onkeydown}
  >
    <header>
      <div>
        <h2 id={titleId}>{title}</h2>
        {#if subtitle}<p class="tf-dsub">{subtitle}</p>{/if}
      </div>
      <button class="tf-iconbtn" aria-label="Close" onclick={onclose}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
      </button>
    </header>
    {#if body}<div class="tf-body">{@render body()}</div>{/if}
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .wide {
    width: min(820px, calc(100vw - 32px));
  }

  /* The title block on the left, Close on the right of the title line. The
     negative margin keeps the 32 px icon button from growing the header:
     placement only, no size. */
  header {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  header .tf-iconbtn {
    margin: -5px -8px -5px auto;
  }

  header svg {
    stroke: currentColor;
    fill: none;
    stroke-width: 1.6;
    stroke-linecap: round;
  }
</style>
