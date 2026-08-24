<!--
  Test fixture for `Modal.test.svelte.ts` — nothing else imports it, so it is
  tree-shaken out of the app bundle.

  It exists because a `Modal`'s interesting behaviour is about the focusable
  controls *inside* it, and a snippet cannot be handed to `mount()` from a
  plain TypeScript test. Two body buttons plus the header's close button give
  the Tab trap three stops to cycle between.
-->
<script lang="ts">
  import Modal from "./Modal.svelte";

  let { onclose }: { onclose: () => void } = $props();
</script>

<Modal title="Add source" subtitle="Step 1 of 5" {onclose}>
  {#snippet body()}
    <button class="btn">Test connection</button>
  {/snippet}
  {#snippet footer()}
    <button class="btn pri">Add</button>
  {/snippet}
</Modal>
