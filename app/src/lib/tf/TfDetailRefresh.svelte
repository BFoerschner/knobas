<!--
  An item's detail's *Refresh* or *Sync now* (#2107), drawn in the detail's
  header toolbar (`tf/refresh.svelte.ts`). *Refresh* reads the one item and
  says *Refreshing…* meanwhile; *Sync now* runs the item's source, shared with
  every detail of it, and the detail is read again once that run ends. Neither
  is drawn for a turned-off source.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import type { EntityDetail } from "../ipc/entity";
  import type { DetailRefresh } from "./refresh.svelte";
  import { push } from "../shell/toasts.svelte";

  let {
    detail,
    refresh,
    reread,
  }: {
    detail: EntityDetail;
    refresh: DetailRefresh;
    /** Read the detail again, over what it drew. */
    reread: () => Promise<void> | void;
  } = $props();

  const offer = $derived(refresh.offer(detail));
  const sync = $derived(offer === "sync" ? refresh.syncOf(detail) : null);

  /** The run endings this button has seen: one more is a run that ended while it was drawn. */
  let seen: number | null = null;
  // The source's run ended: the detail is read again, once per ending.
  $effect(() => {
    const ended = sync?.revision ?? 0;
    if (seen !== null && ended > seen) untrack(() => void reread());
    seen = ended;
  });

  async function syncNow(run: NonNullable<typeof sync>) {
    await run.start();
    if (run.failed !== null) push({ text: `Could not sync: ${run.failed}`, tone: "err" });
  }
</script>

{#if offer === "refresh"}
  <button
    type="button"
    class="tf-btn tf-refresh"
    disabled={refresh.reading}
    title="Read this item from {detail.source.display_name} again"
    onclick={() => void refresh.refresh(detail, reread)}>{refresh.reading ? "Refreshing…" : "Refresh"}</button
  >
{:else if sync !== null}
  <button
    type="button"
    class="tf-btn tf-refresh"
    disabled={sync.running}
    title="Sync {detail.source.display_name}, which this item is from"
    onclick={() => void syncNow(sync)}>{sync.running ? "Syncing…" : "Sync now"}</button
  >
{/if}
