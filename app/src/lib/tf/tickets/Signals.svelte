<!--
  A ticket's signals (#815, R-3.8): each a dot in its tone and its words, in
  the order the read gives them. The card's meta line and the list's Signals
  column both draw them through this.
-->
<script lang="ts">
  import type { Signal } from "../../ipc/entity";

  let { signals }: { signals: Signal[] } = $props();
</script>

{#each signals as signal (signal.kind + signal.text)}
  <span class="tf-signal"><span class="tf-dot tf-{signal.tone}"></span>{signal.text}</span>
{/each}
