<!-- Words with code in them: a fact, a note, a line seen while reading. -->
<script lang="ts">
  import type { Run } from "../../../render/xsd-answer";

  let { runs }: { runs: readonly Run[] } = $props();
</script>

{#each runs as run, i (i)}{#if run.code}<code class="tf-xsd-code">{run.text}</code>{:else}{run.text}{/if}{/each}
