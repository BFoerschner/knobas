<!--
  `#/pr/…`: the pull request page, or the page of one being created
  (`#/pr/new/<write id>`, #2584) until its source answers.
-->
<script lang="ts">
  import type { EscTarget } from "../escape";
  import { readCreatingRoute } from "./creating.svelte";
  import TfPr from "./TfPr.svelte";
  import TfPrCreating from "./TfPrCreating.svelte";

  let { ready, rest, back = null }: { ready: boolean; rest: string[]; back?: EscTarget | null } = $props();

  const creating = $derived(readCreatingRoute(rest));
</script>

{#if creating !== null}
  <TfPrCreating write={creating} {back} />
{:else}
  <TfPr {ready} {rest} {back} />
{/if}
