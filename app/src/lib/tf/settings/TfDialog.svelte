<!--
  The app's one dialog shell (#830, review of #2238): prototype-6's `.dialog`,
  a header, a body and a footer of buttons over a scrim. A dialog of its own
  width passes `className`; the asset tree's dialogs ask for a header Close.

  It registers itself with the shell's layer stack while it is open, so the
  window's `Esc` closes it as rung 1 of the ladder (`escape.ts`) even when focus
  is outside it, and history stays put under it (#1086, #1527). An `Esc` pressed
  inside it closes it here and stops there, so the shell never unwinds a second
  ladder on the same press. Focus goes to its first field, else the body's
  first control, else its first control, and returns to what opened it when it
  closes; `Tab` stays inside it.

  Deliberately not a native `<dialog>`: `showModal()` puts the element in the
  top layer, where `::backdrop`, not `.tf-modal`, draws the dim. What a native
  dialog gives is focus trapping, Escape and focus restore, so those three are
  written out here and tested in `TfDialog.test.svelte.ts`.
-->
<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  import { layers as sharedLayers, type Layer, type Layers } from "../layers.svelte";

  let {
    title,
    subtitle = null,
    onclose,
    layers = sharedLayers,
    body,
    footer,
    className = "",
    closeButton = false,
  }: {
    title: string;
    /** A line under the title, in the dialog's sub tone (the write queue's counts, #1425). */
    subtitle?: string | null;
    onclose: () => void;
    layers?: Layers;
    body: Snippet;
    footer?: Snippet;
    /** A class beside `tf-dialog`, for a dialog of its own size (the view editor's, the import dialog's `tf-dwide`). */
    className?: string;
    /** A Close icon button on the right of the title line. */
    closeButton?: boolean;
  } = $props();

  const uid = $props.id();
  const titleId = `tf-dlg-${uid}`;
  let dialog = $state<HTMLDivElement | null>(null);

  const FOCUSABLE =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  /** Everything inside the dialog a Tab can reach, in document order. */
  function focusable(): HTMLElement[] {
    if (!dialog) return [];
    return [...dialog.querySelectorAll<HTMLElement>(FOCUSABLE)];
  }

  /** This dialog's entry on the layer stack. */
  const layer: Layer = { kind: "dialog", close: () => onclose() };

  $effect(() => {
    const restoreTo = document.activeElement;
    // Untracked: `add` reads the stack it writes, and this effect must run once.
    const off = untrack(() => layers.add(layer));
    // The first field, else the body's first control: the header's Close comes first in document order,
    // and a reader who opened a dialog to type would type into nothing (#1431). Close only when nothing else.
    const first =
      dialog?.querySelector<HTMLElement>("input:not([disabled]):not([type=radio])") ??
      dialog?.querySelector<HTMLElement>(`.tf-body :is(${FOCUSABLE})`) ??
      focusable()[0];
    first?.focus();
    return () => {
      untrack(off);
      if (restoreTo instanceof HTMLElement && document.contains(restoreTo)) restoreTo.focus();
    };
  });

  function onkeydown(event: KeyboardEvent) {
    // A field that takes the key for itself says so (the launcher's box opens its action chain on `Tab`, #1422).
    if (event.defaultPrevented) return;
    // Only while this dialog is the topmost layer: a layer opened over it (#1474) is the shell's to close.
    if (event.key === "Escape" && layers.open.at(-1) === layer) {
      event.preventDefault();
      // Rung 1 of the Esc ladder, answered here: the shell must not also unwind what is behind the dialog.
      event.stopPropagation();
      onclose();
      return;
    }
    if (event.key !== "Tab") return;
    const items = focusable();
    if (items.length === 0) return;
    const first = items[0]!;
    const last = items[items.length - 1]!;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="tf-modal"
  onclick={(event) => {
    if (event.target === event.currentTarget) onclose();
  }}
>
  <div class="tf-dialog {className}" role="dialog" aria-modal="true" aria-labelledby={titleId} tabindex="-1" bind:this={dialog} {onkeydown}>
    <header class="tf-dhead">
      <div class="tf-dtitle">
        <h2 id={titleId}>{title}</h2>
        {#if subtitle}<p class="tf-dsub">{subtitle}</p>{/if}
      </div>
      {#if closeButton}
        <button type="button" class="tf-iconbtn tf-dclose" aria-label="Close" onclick={onclose}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
        </button>
      {/if}
    </header>
    <div class="tf-body">{@render body()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>
