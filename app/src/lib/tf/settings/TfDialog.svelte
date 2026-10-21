<!--
  A dialog in the ticket-first look (#830): prototype-6's `.dialog`, a header,
  a body and a footer of buttons over a scrim.

  It registers itself with the shell's layer stack while it is open, so `Esc`
  closes it as rung 1 of the ladder (`escape.ts`) and nothing else on that
  press. Focus goes to its first field, or its first button, and returns to
  what opened it when it closes; `Tab` stays inside it.
-->
<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  import { layers as sharedLayers, type Layers } from "../layers.svelte";

  let {
    title,
    subtitle = null,
    onclose,
    layers = sharedLayers,
    body,
    footer,
    className = "",
  }: {
    title: string;
    /** A line under the title, in the dialog's sub tone (the write queue's counts, #1425). */
    subtitle?: string | null;
    onclose: () => void;
    layers?: Layers;
    body: Snippet;
    footer: Snippet;
    /** A class beside `tf-dialog`, for a dialog of its own size (the view editor's). */
    className?: string;
  } = $props();

  const uid = $props.id();
  const titleId = `tf-dlg-${uid}`;
  let dialog = $state<HTMLDivElement | null>(null);

  function focusable(): HTMLElement[] {
    if (!dialog) return [];
    return [
      ...dialog.querySelectorAll<HTMLElement>(
        "button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href]",
      ),
    ];
  }

  $effect(() => {
    const restoreTo = document.activeElement;
    // Untracked: `add` reads the stack it writes, and this effect must run once.
    const off = untrack(() => layers.add({ kind: "dialog", close: () => onclose() }));
    const first = dialog?.querySelector<HTMLElement>("input:not([disabled]):not([type=radio])") ?? focusable()[0];
    first?.focus();
    return () => {
      untrack(off);
      if (restoreTo instanceof HTMLElement && document.contains(restoreTo)) restoreTo.focus();
    };
  });

  function onkeydown(event: KeyboardEvent) {
    // A field that takes `Tab` for itself says so (the launcher's box opens its action chain, #1422).
    if (event.key !== "Tab" || event.defaultPrevented) return;
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
    <header>
      <h2 id={titleId}>{title}</h2>
      {#if subtitle !== null}<p class="tf-dsub">{subtitle}</p>{/if}
    </header>
    <div class="tf-body">{@render body()}</div>
    <footer>{@render footer()}</footer>
  </div>
</div>
