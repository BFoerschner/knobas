<!--
  Where the run dialog stands (ADR-0060): one for the window, beside the toasts. It reads the saved commands and the
  runs once the shell is up, and hears runs start and end.
-->
<script lang="ts">
  import { oneoffs as sharedStore, type OneOffs } from "../../render/oneoff.svelte";
  import TfRunDialog from "./TfRunDialog.svelte";

  let { store = sharedStore }: { store?: OneOffs } = $props();

  $effect(() => {
    const off = store.hear();
    if (!store.loaded) void store.load();
    return off;
  });
</script>

{#if store.open !== null}
  {#key store.open}
    <TfRunDialog run={store.open} {store} />
  {/key}
{/if}
