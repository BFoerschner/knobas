<!--
  *Back to <name>* `Esc`, at the end of every breadcrumb (spec #2227, #2228): a real link that does what the key does,
  because the bar is the only way back on screen. `back` is the shell's `escTarget`, the same answer the key acts on,
  and a press goes through the shell's own step (`back-step.ts`), so `⌘]` comes forward again as it does after `Esc`.
  A modified press is the browser's. Mounted alone, in a test, it is a plain link to the address.
  With no address the way back is a panel on this page (a narrowed list, a ticket's steps): a press closes it, as `Esc` does (F-153).
-->
<script lang="ts">
  import { layers } from "./layers.svelte";
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

  function closePanel(event: MouseEvent) {
    event.preventDefault();
    layers.open.findLast((layer) => layer.kind === "panel")?.close();
  }
</script>

<!-- A key is told from the words by its face, and the whole link is underlined (F-053), so it reads as one link to the thing it names. -->
{#snippet words()}{#if code}<span class="tf-esc-t">Back to <span class="tf-mono tf-key">{name}</span></span>{:else}<span class="tf-esc-t">Back to {name}</span>{/if}{/snippet}
{#if back.hash === undefined}
  <a class="tf-esc" href="#/" onclick={closePanel}>{@render words()} <kbd>Esc</kbd></a>
{:else}
  <a class="tf-esc" href={back.hash} onclick={press}>{@render words()} <kbd>Esc</kbd></a>
{/if}
