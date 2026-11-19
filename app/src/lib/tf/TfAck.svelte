<!--
  An opening's acknowledgement (`shell/ack.svelte.ts`, ux21): the one chip, where the press was, and the window's
  listeners that say where that is. Ink on the page as a toast is, one line, nothing to press. The live region is
  always there, so a reader hears the words the moment a chip comes.
-->
<script lang="ts">
  import { acks, notePress, resetAcks, type AckPlace } from "../shell/ack.svelte";

  $effect(() => {
    const note = (event: PointerEvent | KeyboardEvent) => notePress(event);
    window.addEventListener("pointerdown", note, true);
    window.addEventListener("keydown", note, true);
    return () => {
      window.removeEventListener("pointerdown", note, true);
      window.removeEventListener("keydown", note, true);
      // With nothing to draw it, a chip and its timers go too.
      resetAcks();
    };
  });

  /** Pinned through the element's style, not a `style` attribute, which the window's CSP refuses (`style-src 'self'`). */
  function placed(place: AckPlace) {
    return (node: HTMLElement) => {
      for (const side of ["left", "right", "top", "bottom"] as const) {
        const value = place[side];
        if (value !== undefined) node.style[side] = `${Math.round(value)}px`;
      }
    };
  }
</script>

<div class="tf-acks" role="status" aria-live="polite">
  {#if acks.item}
    {#key acks.item.id}
      <div class="tf-ack" class:tf-ack-centre={acks.item.place.centre} class:tf-ack-leaving={acks.item.leaving} {@attach placed(acks.item.place)}>
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
          ><path d="M9 3h4v4M13 3L7.5 8.5M11.5 9.5V13h-8.5V4.5H6.5" /></svg
        ><span>{acks.item.text}</span>
      </div>
    {/key}
  {/if}
</div>
