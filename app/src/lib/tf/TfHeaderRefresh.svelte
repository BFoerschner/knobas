<!--
  The end of a detail page's header (#2287, review of #2238): the detail's own
  *Refresh* or *Sync now* (`TfDetailRefresh`, the page's ⌘R), then the view's
  *Sync now* (the role frame's `sync` snippet), unless the detail's button
  already is one: one *Sync now* per header. With no detail read yet, the
  view's *Sync now* alone. The pipeline, the run and the Docs reader end their
  `.tf-acts` with this.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import TfDetailRefresh from "./TfDetailRefresh.svelte";
  import type { DetailRefresh, RefreshSubject } from "./refresh.svelte";

  let {
    detail,
    refresh,
    reread,
    sync,
  }: {
    detail: RefreshSubject | null;
    refresh: DetailRefresh;
    /** Read the detail again, over what it drew. */
    reread: () => Promise<void> | void;
    /** The view's *Sync now*. */
    sync?: Snippet | undefined;
  } = $props();
</script>

{#if detail !== null}<TfDetailRefresh {detail} {refresh} {reread} page />{/if}
{#if detail === null || refresh.offer(detail) !== "sync"}{@render sync?.()}{/if}
