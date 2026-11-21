<!-- Beside `TfMenu.test.svelte.ts`: an opener, its menu, and a dialog a pick can open, which is how a pick's focus matters.
     `alive` takes the menu's owner away (a page going as the address moves); `second` draws another menu. -->
<script lang="ts">
  import type { Layers } from "./layers.svelte";
  import TfDialog from "./settings/TfDialog.svelte";
  import TfMenu from "./TfMenu.svelte";

  let {
    layers,
    host = false,
    ready = true,
    inDialog = false,
    alive = true,
    second = false,
  }: { layers: Layers; host?: boolean; ready?: boolean; inDialog?: boolean; alive?: boolean; second?: boolean } = $props();

  let open = $state(false);
  let asking = $state(false);
  let face = $state<HTMLButtonElement | null>(null);
  let innerOpen = $state(false);
  let innerFace = $state<HTMLButtonElement | null>(null);
  let otherOpen = $state(false);
  let otherFace = $state<HTMLButtonElement | null>(null);

  /** Open or close the first menu, or the other, as its opener would, with no press for its outside-click to see. */
  export function set(which: "first" | "other", to: boolean) {
    if (which === "first") open = to;
    else otherOpen = to;
  }
</script>

<div class="wrap">
  <button type="button" class="face" aria-expanded={open} bind:this={face} onclick={() => (open = !open)}>Open</button>
  <button type="button" class="after">After</button>
  {#if alive}
    <TfMenu bind:open opener={face} label="Things" {layers} {ready} focusOn="[aria-checked=true]">
      <button type="button" role="menuitem" class="one" onclick={() => (open = false)}>One</button>
      <button type="button" role="menuitem" disabled>Two</button>
      <button type="button" role="menuitemradio" aria-checked="false" class="three" onclick={() => ((open = false), (asking = true))}>Three</button>
    </TfMenu>
  {/if}
  {#if second}
    <button type="button" class="other" bind:this={otherFace} onclick={() => (otherOpen = !otherOpen)}>Other</button>
    <TfMenu bind:open={otherOpen} opener={otherFace} label="Other things" {layers}>
      <button type="button" role="menuitem">Else</button>
    </TfMenu>
  {/if}
</div>
{#if host}<div class="tf-floats"></div>{/if}
{#if inDialog}
  <TfDialog title="Holds a menu" {layers} onclose={() => {}}>
    {#snippet body()}
      <input aria-label="Before" />
      <button type="button" class="inner" aria-expanded={innerOpen} bind:this={innerFace} onclick={() => (innerOpen = !innerOpen)}>Inner</button>
      <TfMenu bind:open={innerOpen} opener={innerFace} label="Inner things" {layers}>
        <button type="button" role="menuitem" onclick={() => (innerOpen = false)}>Only</button>
      </TfMenu>
    {/snippet}
  </TfDialog>
{/if}
{#if asking}
  <TfDialog title="Asked" {layers} onclose={() => (asking = false)}>
    {#snippet body()}<input aria-label="Field" />{/snippet}
  </TfDialog>
{/if}
