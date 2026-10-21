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
  import Modal from "./Modal.svelte";

  let {
    onclose,
    content = "button",
  }: { onclose: () => void; content?: "button" | "input" | "none" } = $props();
</script>

<Modal title="Add source" subtitle="Step 1 of 5" {onclose}>
  {#snippet body()}
    {#if content === "button"}
      <button class="btn">Test connection</button>
    {:else if content === "input"}
      <label for="fixture-name">Name</label>
      <input id="fixture-name" type="text" />
    {:else}
      <p>Nothing to fill in.</p>
    {/if}
  {/snippet}
  {#snippet footer()}
    <button class="btn pri">Add</button>
  {/snippet}
</Modal>
