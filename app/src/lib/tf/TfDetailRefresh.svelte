<!--
  An item's detail's *Refresh* or *Sync now* (#2107), drawn in the detail's
  header toolbar, at the end of the job page's tab bar (#2134), or in a
  monitor's roster row (#2138)
  (`tf/refresh.svelte.ts`). *Refresh* reads the one item and
  says *Refreshing…* meanwhile; *Sync now* runs the item's source, shared with
  every detail of it, and the detail is read again once that run ends. Neither
  is drawn for a turned-off source.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import type { DetailRefresh, RefreshSubject } from "./refresh.svelte";
  import { push } from "../shell/toasts.svelte";
  import { pageRefresh } from "./page-refresh.svelte";
  import { pageVisible } from "./kept-pages.svelte";

  let {
    detail,
    refresh,
    reread,
    page = false,
    small = false,
    ghost = false,
  }: {
    detail: RefreshSubject;
    refresh: DetailRefresh;
    /** Read the detail again, over what it drew. */
    reread: () => Promise<void> | void;
    /**
     * Whether this is the page's own button, which ⌘R presses while it is
     * drawn (#2156): a detail page's header or tab bar says so. A monitor's
     * roster row does not; that page's is the header's *Sync now*.
     */
    page?: boolean;
    /** Drawn at the small size: in a page body, as the job's tab strip draws it (#2294). */
    small?: boolean;
    /** Drawn as a ghost, beside the other ghost actions of a monitor's row (#2138). */
    ghost?: boolean;
  } = $props();
  /** The button's classes: the Controls button at its size and look, and `tf-refresh`, which tests find it by. */
  const classes = $derived(["tf-btn", small && "tf-sm", ghost && "tf-ghost", "tf-refresh"].filter(Boolean).join(" "));

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

  /** *Refreshing…* or *Syncing…*: the button is disabled, and ⌘R presses nothing. */
  const busy = $derived(offer === "refresh" ? refresh.reading : sync?.running === true);

  /** What a click runs, and ⌘R. */
  function press() {
    if (busy) return;
    if (offer === "refresh") void refresh.refresh(detail, reread);
    else if (sync !== null) void syncNow(sync);
  }

  const drawn = $derived(offer === "refresh" || sync !== null);
  // While drawn, ⌘R presses it.
  // Only on the shown page: ⌘R never presses a page keep-alive holds hidden (#2247).
  const shown = pageVisible();
  $effect(() => {
    if (page && drawn && shown()) return untrack(() => pageRefresh.add({ kind: "detail", press }));
  });
</script>

{#if offer === "refresh"}
  <button
    type="button"
    class={classes}
    disabled={busy}
    title="Read this item from {detail.source.display_name} again"
    onclick={press}>{refresh.reading ? "Refreshing…" : "Refresh"}</button
  >
{:else if sync !== null}
  <button
    type="button"
    class={classes}
    disabled={busy}
    title="Sync {detail.source.display_name}, which this item is from"
    onclick={press}>{sync.running ? "Syncing…" : "Sync now"}</button
  >
{/if}
