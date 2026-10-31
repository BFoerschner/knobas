<!--
  A role view's *Sync now* (#2106), drawn by the role frame (`TfPage.svelte`)
  into the header of every view whose page names a role, at the header's
  default size and last in its actions (#2238). It runs each enabled
  source that fills the role, as the mirror answers it (`sync_role`), says
  *Syncing…* until those runs end, and is not drawn for a role no enabled
  source fills.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import type { RoleSync } from "./role-sync.svelte";
  import { sourceRoles } from "./roles.svelte";
  import { push } from "../shell/toasts.svelte";
  import { pageRefresh } from "./page-refresh.svelte";
  import { pageVisible } from "./kept-pages.svelte";

  let { sync }: { sync: RoleSync } = $props();

  const title = $derived(
    sync.going > 0 ? `${sync.going} ${sync.going === 1 ? "source" : "sources"} syncing` : "Sync every source this view is drawn from",
  );

  async function start() {
    await sync.start();
    if (sync.failed !== null) push({ text: `Could not sync: ${sync.failed}`, tone: "err" });
  }

  /** What a click runs, and ⌘R (#2156): nothing while it says *Syncing…*. */
  function press() {
    if (!sync.running) void start();
  }

  const drawn = $derived(sourceRoles.syncs(sync.role));
  // While drawn on the shown page (not one keep-alive holds hidden, #2247),
  // ⌘R presses it, unless the page draws a detail's own.
  const shown = pageVisible();
  $effect(() => {
    if (drawn && shown()) return untrack(() => pageRefresh.add({ kind: "role", press }));
  });
</script>

{#if drawn}
  <button type="button" class="tf-btn tf-syncnow" disabled={sync.running} {title} onclick={press}
    >{sync.running ? "Syncing…" : "Sync now"}</button
  >
{/if}

