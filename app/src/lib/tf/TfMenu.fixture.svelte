<!-- Beside `TfMenu.test.svelte.ts`: an opener, its menu, and a dialog a pick can open, which is how a pick's focus matters. -->
<script lang="ts">
  import type { Layers } from "./layers.svelte";
  import TfDialog from "./settings/TfDialog.svelte";
  import TfMenu from "./TfMenu.svelte";

  let { layers, host = false, ready = true }: { layers: Layers; host?: boolean; ready?: boolean } = $props();

  let open = $state(false);
  let asking = $state(false);
  let face = $state<HTMLButtonElement | null>(null);
</script>

<div class="wrap">
  <button type="button" class="face" aria-expanded={open} bind:this={face} onclick={() => (open = !open)}>Open</button>
  <button type="button" class="after">After</button>
  <TfMenu bind:open opener={face} label="Things" {layers} {ready} focusOn="[aria-checked=true]">
    <button type="button" role="menuitem" class="one" onclick={() => (open = false)}>One</button>
    <button type="button" role="menuitem" disabled>Two</button>
    <button type="button" role="menuitemradio" aria-checked="false" class="three" onclick={() => ((open = false), (asking = true))}>Three</button>
  </TfMenu>
</div>
{#if host}<div class="tf-floats"></div>{/if}
{#if asking}
  <TfDialog title="Asked" {layers} onclose={() => (asking = false)}>
    {#snippet body()}<input aria-label="Field" />{/snippet}
  </TfDialog>
{/if}
