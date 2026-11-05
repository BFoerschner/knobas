<!--
  *Back to <name>* `Esc`, at the end of every breadcrumb (spec #2227, #2228): a real link that does what the key does,
  because the bar is the only way back on screen. `back` is the shell's `escTarget`, the same answer the key acts on,
  and a press goes through the shell's own step (`back-step.ts`), so `⌘]` comes forward again as it does after `Esc`.
  A modified press is the browser's. Mounted alone, in a test, it is a plain link to the address.
-->
<script lang="ts">
  import { backStep } from "./page-name.svelte";
  import type { EscTargetOf } from "./escape";

  let {
    back,
    name = back.name,
  }: {
    back: Pick<EscTargetOf, "name"> & Partial<EscTargetOf>;
    /** What the page calls the place, when it knows better than the shell. */
    name?: string;
  } = $props();

  const step = backStep();

  function press(event: MouseEvent) {
    if (back.hash === undefined || step === null) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    step(back.hash, back.opened ?? false);
  }
</script>

{#if back.hash === undefined}
  <span class="tf-esc"><span class="tf-esc-t">Back to {name}</span> <kbd>Esc</kbd></span>
{:else}
  <a class="tf-esc" href={back.hash} onclick={press}><span class="tf-esc-t">Back to {name}</span> <kbd>Esc</kbd></a>
{/if}
