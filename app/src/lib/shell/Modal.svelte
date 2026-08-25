<!--
  A focus-trapped dialog over a scrim.

  Deliberately not a native `<dialog>`: `showModal()` puts the element in the
  top layer, where `::backdrop` — not `.scrim` — draws the dim, and the
  mockup's `.scrim > .dlg` geometry (the 80 px drop from the top, `.scrim.center`
  for the centred variant) would have to be rebuilt against a pseudo-element
  this stylesheet does not style. What a native dialog *gives* is focus
  trapping, Escape and focus restore, so those three are written out below and
  tested; nothing else about `showModal` was wanted.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    subtitle,
    center = false,
    wide = false,
    onclose,
    body,
    footer,
  }: {
    title: string;
    subtitle?: string;
    /** `.scrim.center` — vertically centred rather than dropped from the top. */
    center?: boolean;
    /** `.dlg.wide` — the 820 px variant the add-source flow uses. */
    wide?: boolean;
    onclose: () => void;
    body?: Snippet;
    footer?: Snippet;
  } = $props();

  /** Unique per instance, so two stacked dialogs do not share a label id. */
  const titleId = `dlg-title-${Math.random().toString(36).slice(2, 9)}`;

  let dialog = $state<HTMLDivElement | null>(null);

  /** Everything inside the dialog a Tab can reach, in document order. */
  function focusable(): HTMLElement[] {
    if (!dialog) return [];
    return [
      ...dialog.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ),
    ];
  }

  $effect(() => {
    // Captured before the move, and restored on teardown: after a dialog
    // closes the reader must be back on the control that opened it, not at the
    // top of the document.
    const restoreTo = document.activeElement;
    focusable()[0]?.focus();
    return () => {
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
  class="scrim {center ? 'center' : ''}"
  onclick={(event) => {
    if (event.target === event.currentTarget) onclose();
  }}
>
  <div
    class="dlg {wide ? 'wide' : ''}"
    bind:this={dialog}
    role="dialog"
    aria-modal="true"
    aria-labelledby={titleId}
    tabindex="-1"
    {onkeydown}
  >
    <div class="dlg-h">
      <h2 id={titleId}>{title}</h2>
      {#if subtitle}<span class="lab">{subtitle}</span>{/if}
      <button class="x" aria-label="Close" onclick={onclose}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
      </button>
    </div>
    {#if body}<div class="dlg-b">{@render body()}</div>{/if}
    {#if footer}<div class="dlg-f">{@render footer()}</div>{/if}
  </div>
</div>
