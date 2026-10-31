<!--
  Test fixture for `Modal.test.svelte.ts` — nothing else imports it, so it is
  tree-shaken out of the app bundle.

  It exists because a `Modal`'s interesting behaviour is about the focusable
  controls *inside* it, and a snippet cannot be handed to `mount()` from a
  plain TypeScript test. Two body buttons plus the header's close button give
  the Tab trap three stops to cycle between.

  `content` picks what the body holds, for the open-time focus (#1431): a button
  (the default), a text field, or nothing focusable at all.
-->
<script lang="ts">
  import { layers as sharedLayers, type Layers } from "../tf/layers.svelte";
  import Modal from "./Modal.svelte";

  let {
    onclose,
    content = "button",
    layers = sharedLayers,
  }: { onclose: () => void; content?: "button" | "input" | "none"; layers?: Layers } = $props();
</script>

<Modal title="Add source" subtitle="Step 1 of 5" {onclose} {layers}>
  {#snippet body()}
    {#if content === "button"}
      <button class="tf-btn tf-sm">Test connection</button>
    {:else if content === "input"}
      <label for="fixture-name">Name</label>
      <input id="fixture-name" type="text" />
    {:else}
      <p>Nothing to fill in.</p>
    {/if}
  {/snippet}
  {#snippet footer()}
    <button class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button class="tf-btn tf-primary">Add</button>
  {/snippet}
</Modal>
