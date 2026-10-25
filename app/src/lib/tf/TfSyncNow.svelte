<!--
  A role view's *Sync now* (#2106), drawn by the role frame (`TfPage.svelte`)
  into the header of every view whose page names a role. It runs each enabled
  source that fills the role, as the mirror answers it (`sync_role`), says
  *Syncing…* until those runs end, and is not drawn for a role no enabled
  source fills.
-->
<script lang="ts">
  import type { RoleSync } from "./role-sync.svelte";
  import { sourceRoles } from "./roles.svelte";
  import { push } from "../shell/toasts.svelte";

  let { sync }: { sync: RoleSync } = $props();

  const title = $derived(
    sync.going > 0 ? `${sync.going} ${sync.going === 1 ? "source" : "sources"} syncing` : "Sync every source this view is drawn from",
  );

  async function start() {
    await sync.start();
    if (sync.failed !== null) push({ text: `Could not sync: ${sync.failed}`, tone: "err" });
  }
</script>

{#if sourceRoles.syncs(sync.role)}
  <button type="button" class="tf-btn tf-sm tf-syncnow" disabled={sync.running} {title} onclick={() => void start()}
    >{sync.running ? "Syncing…" : "Sync now"}</button
  >
{/if}

