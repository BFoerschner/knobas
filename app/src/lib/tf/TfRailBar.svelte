<!--
  The one fold bar (design system, Layout): below 900 px a stacked side rail folds behind this bar, which names what is
  open. Tickets' views, Builds' pipelines, Settings' sections and Docs' tree all draw it. A link picked in the rail folds
  it again. Above 900 px the bar is not drawn (`.tf-railbar`, chrome.css).

  With `onclose`, an open fold is a panel on the layer stack: `Esc` folds it before it leaves the page (F-619), and focus
  comes back to this bar, because the link that held it is folded away.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { layers as sharedLayers, type Layers } from "./layers.svelte";

  let {
    label,
    value,
    open,
    controls,
    onpress,
    onclose,
    layers = sharedLayers,
  }: {
    /** What the rail lists: *View*, *Pipeline*, *Section*, *Page*. */
    label: string;
    /** What is open. */
    value: string;
    open: boolean;
    /** The rail's id. */
    controls: string;
    onpress: (event: MouseEvent) => void;
    /** Folds the rail, for `Esc`. */
    onclose?: (() => void) | undefined;
    layers?: Layers;
  } = $props();

  let press = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    if (!open || !onclose) return;
    // Untracked: `add` reads the stack it writes.
    const off = untrack(() =>
      layers.add({
        kind: "panel",
        close: () => {
          onclose?.();
          press?.focus({ preventScroll: true });
        },
      }),
    );
    return () => untrack(off);
  });
</script>

<div class="tf-railbar">
  <button type="button" class="tf-btn tf-sm" aria-expanded={open} aria-controls={controls} onclick={onpress} bind:this={press}>
    {label}: <b>{value}</b><i class="tf-railmark" aria-hidden="true"></i>
  </button>
</div>
