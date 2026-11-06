<!--
  Test fixture for `inline-editor.test.svelte.ts`: the usual shape of an inline editor, a button a
  redraw swaps for the editor and swaps back as a new element.
-->
<script lang="ts">
  import { inlineEditor } from "./inline-editor";
  import type { Layers } from "./layers.svelte";

  let { layers, text = "" }: { layers: Layers; text?: string } = $props();

  let editing = $state(false);
  let host = $state<HTMLElement | null>(null);
</script>

<div bind:this={host}>
  {#if editing}
    <div
      {@attach inlineEditor(() => (editing = false), {
        layers,
        back: () => host?.querySelector<HTMLElement>("[data-open]") ?? null,
      })}
    >
      <textarea aria-label="Comment" value={text}></textarea>
      <button data-close onclick={() => (editing = false)}>Cancel</button>
    </div>
  {:else}
    <button data-open onclick={() => (editing = true)}>Edit</button>
  {/if}
</div>
