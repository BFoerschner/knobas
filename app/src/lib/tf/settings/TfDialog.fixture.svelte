<!--
  Test fixture for `TfDialog.test.svelte.ts` — nothing else imports it, so it is
  tree-shaken out of the app bundle.

  It exists because a dialog's interesting behaviour is about the focusable
  controls *inside* it, and a snippet cannot be handed to `mount()` from a
  plain TypeScript test. Two body buttons plus the header's Close give the Tab
  trap three stops to cycle between.

  `content` picks what the body holds, for the open-time focus (#1431): a button
  (the default), a text field, a select, radios, a list that drops the pressed
  row, or nothing focusable at all. `markupTitle` hands the title as a snippet with
  markup in it, as the worklog draft's is (#2346). `viewer` draws no footer, and
  `why` is the footer's missing line.
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
    viewer = false,
    why = null,
    onsubmit = () => {},
  }: {
    onclose: () => void;
    content?: "button" | "input" | "select" | "radios" | "rows" | "none";
    layers?: Layers;
    closeButton?: boolean;
    className?: string;
    markupTitle?: boolean;
    viewer?: boolean;
    why?: string | null;
    onsubmit?: () => void;
  } = $props();

  let gone = $state(false);
</script>

{#snippet keyed()}Log time to <span class="tf-mono">PROJ-7</span>{/snippet}

{#snippet actions()}
  <button class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
  <button class="tf-btn tf-primary">Add</button>
{/snippet}

<TfDialog title={markupTitle ? keyed : "Add source"} subtitle="Step 1 of 5" {onclose} {layers} {closeButton} {className} {why} footer={viewer ? undefined : actions}>
  {#snippet body()}
    {#if content === "button"}
      <button class="tf-btn tf-sm">Test connection</button>
    {:else if content === "input"}
      <label for="fixture-name">Name</label>
      <input id="fixture-name" type="text" />
    {:else if content === "select"}
      <button class="tf-btn tf-sm">Test connection</button>
      <select aria-label="Kind"><option>One</option></select>
      <input type="text" aria-label="Name" />
    {:else if content === "radios"}
      <form onsubmit={(event) => { event.preventDefault(); onsubmit(); }}>
        <button class="tf-btn tf-sm" type="button">Test connection</button>
        <label><input type="radio" name="fixture-style" /> Merge</label>
        <button type="submit" hidden>Send</button>
      </form>
    {:else if content === "rows"}
      {#if !gone}<button class="tf-btn tf-sm" data-row onclick={() => (gone = true)}>Send mine anyway</button>{/if}
      <p>{gone ? "Sent." : "One row."}</p>
    {:else}
      <p>Nothing to fill in.</p>
    {/if}
  {/snippet}
</TfDialog>
