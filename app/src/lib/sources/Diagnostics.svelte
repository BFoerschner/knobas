<!--
  The diagnostics panel under the sources list — spec §3 (Rec 08-24): *"this is
  where you look when a sync misbehaves"*.

  Two independent reads, and they stay independent: `db_stats` failing must
  leave the run log readable, because a diagnostics view that blanks when half
  of it errors is least useful at the exact moment it is most needed.
-->
<script lang="ts">
  import { dbStats, listSyncRuns, reindexFts, type DbStats, type SyncRunRow } from "../ipc/sources";
  import { ipcErrorMessage } from "../ipc";
  import Modal from "../shell/Modal.svelte";
  import { push } from "../shell/toasts.svelte";
  import SyncRunList from "./SyncRunList.svelte";
  import { formatBytes } from "./diagnostics";

  let {
    now = new Date(),
    sourceId = null,
    reloadKey = 0,
  }: {
    now?: Date;
    /** `null` spans every source — the whole log. */
    sourceId?: string | null;
    /** Bump to re-read: the sources view does it when a run finishes. */
    reloadKey?: number;
  } = $props();

  /** Clamped to 500 by the backend; 50 is what fits before a reader scrolls. */
  const LIMIT = 50;

  let runs = $state<SyncRunRow[]>([]);
  let stats = $state<DbStats | null>(null);
  let runsError = $state<string | null>(null);
  let confirming = $state(false);
  let reindexing = $state(false);

  async function load() {
    // Two awaits, two `catch`es: neither read may take the other down.
    await Promise.all([
      listSyncRuns(sourceId, LIMIT)
        .then((rows) => {
          runs = rows;
          runsError = null;
        })
        .catch((cause) => {
          runsError = ipcErrorMessage(cause);
        }),
      dbStats()
        .then((next) => {
          stats = next;
        })
        .catch(() => {
          // The `.dbbar` renders em dashes. A status line that says `0 items`
          // when it simply has not asked is worse than one that says it does
          // not know.
          stats = null;
        }),
    ]);
  }

  $effect(() => {
    // Read so the effect re-runs when the caller bumps it.
    void reloadKey;
    void sourceId;
    void load();
  });

  async function reindex() {
    confirming = false;
    reindexing = true;
    try {
      await reindexFts();
      push({ text: "Full-text index rebuilt." });
    } catch (cause) {
      push({ text: `Re-index failed: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      reindexing = false;
    }
  }

  /** A stamp as the diagnostics line prints it: date and minute, in UTC. */
  function stamp(iso: string | null): string {
    if (!iso) return "—";
    const at = new Date(iso);
    if (Number.isNaN(at.getTime())) return "—";
    return at.toISOString().slice(0, 16).replace("T", " ");
  }
</script>

<div class="dbbar">
  <span>postgres knobas · {stats ? formatBytes(stats.db_bytes) : "—"}</span>
  <span>{stats ? stats.entity_count : "—"} entities · {stats ? stats.item_count : "—"} items</span>
  <span>synced {stamp(stats?.oldest_synced_at ?? null)} → {stamp(stats?.newest_synced_at ?? null)}</span>
  <span class="spacer"></span>
  <button class="btn sm" onclick={() => void load()}>Refresh</button>
  <button class="btn sm" disabled={reindexing} onclick={() => (confirming = true)}>
    Re-index full text
  </button>
</div>

<div class="tile-h">
  <span class="lab">Sync log</span>
  <span class="cnt">{runs.length}</span>
</div>

{#if runsError}
  <div class="empty">
    <p>Could not read the sync log.</p>
    <p class="mono fail">{runsError}</p>
  </div>
{:else}
  <SyncRunList {runs} {now} />
{/if}

{#if confirming}
  <Modal title="Rebuild the full-text index?" center onclose={() => (confirming = false)}>
    {#snippet body()}
      <p>
        knobas rebuilds the search index over every mirrored item. It runs concurrently, so search
        keeps working while it does — nothing is deleted and nothing needs re-syncing.
      </p>
      <p class="note">Worth doing when search misses something you can see in a room.</p>
    {/snippet}
    {#snippet footer()}
      <button class="btn" onclick={() => (confirming = false)}>Cancel</button>
      <button class="btn pri" onclick={() => void reindex()}>Re-index</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .dbbar .spacer {
    flex: 1;
  }

  .dbbar button {
    align-self: center;
  }

  .fail {
    color: var(--fail);
  }

  .note {
    margin-top: 8px;
    font: 400 11px/1.5 var(--mono);
    color: var(--muted);
  }
</style>
