<!--
  *Back to <name>* `Esc`, at the end of every breadcrumb (spec #2227, #2228): a real link that does what the key does,
  because the bar is the only way back on screen. `back` is the shell's `escTarget`, the same answer the key acts on,
  and a press goes through the shell's own step (`back-step.ts`), so `⌘]` comes forward again as it does after `Esc`.
  A modified press is the browser's. Mounted alone, in a test, it is a plain link to the address.
-->
<script lang="ts">
  import { backStep } from "./page-name.svelte";
  import type { EscTargetOf } from "./escape";
  import { isKey } from "./TfKey.svelte";

  let {
    back,
    name = back.name,
  }: {
    back: Pick<EscTargetOf, "name"> & Partial<EscTargetOf>;
    /** What the page calls the place, when it knows better than the shell. */
    name?: string;
  } = $props();

  const step = backStep();
  /** A repository's name is a code value `isKey` cannot tell from words, so the place it leads to says so. */
  const code = $derived(isKey(name) || (back.hash?.startsWith("#/repo/") === true && name.includes("/")));

  function press(event: MouseEvent) {
    if (back.hash === undefined || step === null) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    step(back.hash, back.opened ?? false);
  }
</script>

<!-- A key is told from the words by its face, so the underline stops at the words. -->
{#snippet words()}{#if code}<span class="tf-esc-t">Back to</span> <span class="tf-mono tf-key">{name}</span>{:else}<span class="tf-esc-t">Back to {name}</span>{/if}{/snippet}
{#if back.hash === undefined}
  <span class="tf-esc">{@render words()} <kbd>Esc</kbd></span>
{:else}
  <a class="tf-esc" href={back.hash} onclick={press}>{@render words()} <kbd>Esc</kbd></a>
{/if}
