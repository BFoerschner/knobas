<!--
  The two banners an item's detail draws above its body (#2107, CONTEXT.md's
  detail read): **withdrawn**, when the source no longer has the item and
  knobas kept it for the links and notes that point at it; **turned off**,
  when the reader turned its source off. Two banners, because the remedies
  differ: nothing undoes a withdrawal (a *Refresh* that finds the item again
  does), and re-enabling the source undoes the other. Both show when both hold.
-->
<script lang="ts">
  import type { EntityDetail } from "../ipc/entity";
  import { ageLabel } from "../time/format";

  let { detail }: { detail: EntityDetail } = $props();
</script>

{#if detail.deleted_at !== null}
  <div class="tf-banner tf-warn tf-withdrawn" role="status">
    Withdrawn upstream, last seen {ageLabel(detail.deleted_at)} ago. Kept because links and notes may point at it.
  </div>
{/if}
{#if !detail.source.enabled}
  <div class="tf-banner tf-warn tf-turnedoff" role="status">
    Its source is turned off: {detail.source.display_name} is disabled, so this item is hidden everywhere else. Re-enable the source to bring it back.
  </div>
{/if}
