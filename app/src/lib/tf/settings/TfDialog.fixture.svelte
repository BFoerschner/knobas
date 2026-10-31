<!--
  Test fixture for `TfDialog.test.svelte.ts` — nothing else imports it, so it is
  tree-shaken out of the app bundle.

  It exists because a dialog's interesting behaviour is about the focusable
  controls *inside* it, and a snippet cannot be handed to `mount()` from a
  plain TypeScript test. Two body buttons plus the header's Close give the Tab
  trap three stops to cycle between.

  `content` picks what the body holds, for the open-time focus (#1431): a button
  (the default), a text field, or nothing focusable at all. `markupTitle` hands
  the title as a snippet with markup in it, as the worklog draft's is (#2346).
-->
<script lang="ts">
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfDialog from "./TfDialog.svelte";

  let {
    onclose,
    content = "button",
    layers = sharedLayers,
    closeButton = true,
    className = "",
    markupTitle = false,
  }: {
    onclose: () => void;
    content?: "button" | "input" | "none";
    layers?: Layers;
    closeButton?: boolean;
    className?: string;
    markupTitle?: boolean;
  } = $props();
</script>

{#snippet keyed()}Log time to <span class="tf-mono">PROJ-7</span>{/snippet}

<TfDialog title={markupTitle ? keyed : "Add source"} subtitle="Step 1 of 5" {onclose} {layers} {closeButton} {className}>
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
</TfDialog>
