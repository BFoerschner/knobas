<!--
  A facts block (ADR-0020): label and value pairs, drawn as the detail's own
  `.kv` grid so a view's facts read like the rest of the panel. A tone colours
  the value only; colour means something is wrong or moving.
-->
<script lang="ts">
  import type { Fact } from "../../ipc/entity";
  import { toneClass } from "./tone";

  let { facts }: { facts: Fact[] } = $props();
</script>

<div class="kv fb">
  {#each facts as fact, index (index)}
    <span class="l">{fact.label}</span>
    <span class="v fb-v {toneClass(fact.tone)}">{fact.value}</span>
  {/each}
</div>

<style>
  .fb {
    grid-template-columns: 110px 1fr;
  }

  .fb-v {
    overflow-wrap: anywhere;
  }

</style>
