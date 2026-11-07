<!--
  The two banners an item's detail draws above its body (#2107, CONTEXT.md's
  detail read): **withdrawn**, when the source no longer has the item and
  knobas kept it for the links and notes that point at it; **turned off**,
  when the reader turned its source off. Two banners, because the remedies
  differ: nothing undoes a withdrawal (a *Refresh* that finds the item again
  does), and re-enabling the source undoes the other. Both show when both hold.
  Under them, a failed *Refresh*'s line (#2107), the detail kept as it was, with
  its own *Try again*: the ticket page has no header button to point at, and a
  retry is what the line is for.
-->
<script lang="ts">
  import { tick } from "svelte";

  import type { EntityDetail } from "../ipc/entity";
  import { ageLabel } from "../time/format";
  import type { DetailRefresh } from "./refresh.svelte";

  let {
    detail,
    refresh,
    reread,
  }: {
    detail: EntityDetail;
    /** The detail's *Refresh*: its failure line, and what *Try again* runs. */
    refresh: DetailRefresh;
    /** Read the detail again, over what it drew. */
    reread: () => Promise<void> | void;
  } = $props();

  /**
   * The press takes its own button away while the read is out, so focus lands afterwards on what stands there: this
   * line's button if it failed again, else the header's *Refresh*. Only this page's, because a kept page is drawn too.
   */
  async function again(event: MouseEvent): Promise<void> {
    const page = (event.currentTarget as HTMLElement).closest(".tf-kept") ?? document;
    await refresh.refresh(detail, reread);
    await tick();
    if (document.activeElement !== document.body) return;
    (page.querySelector<HTMLElement>(".tf-refresh-failed button") ?? page.querySelector<HTMLElement>(".tf-refresh"))?.focus();
  }
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
{#if refresh.failed !== null}
  <div class="tf-banner tf-err tf-refresh-failed" role="alert">
    <span>{refresh.failed.replace("the source", detail.source.display_name)}.</span>
    <button type="button" class="tf-btn tf-sm" onclick={again}>Try again</button>
  </div>
{/if}
