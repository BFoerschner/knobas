<!--
  A sentence the app composes with a ticket key in it (*Linked to PAY-231*, *Parent PAY-230*): the words as they are, each
  key as a key (design system, Type).
-->
<script lang="ts">
  import TfKey from "../TfKey.svelte";
  import { keyRuns } from "./docs";

  let { text }: { text: string } = $props();
</script>

{#each keyRuns(text) as run, at (at)}{#if run.key}<TfKey value={run.text} />{:else}{run.text}{/if}{/each}
