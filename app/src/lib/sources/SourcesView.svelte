<!--
  The sources view (`signal-miller.html`'s `renderSources`).

  Spec §3: add a source, watch it sync, see when its credential stops working,
  and fix it. The list is the whole view; the diagnostics below it are where a
  reader goes when a sync misbehaves (task 19).

  ## What re-lists, and what does not

  `sync:state` **patches** the matching row. The event carries a whole
  `SourceSyncStatus` (contract §2.3), so re-listing on every transition would
  make a five-source sync fetch this view twenty times to learn what the event
  already said.

  A **mutation** re-lists, because add/delete change the row *set* and nothing
  else tells the view about it.
-->
<script lang="ts">
  import { listen } from "@tauri-apps/api/event";

  import { EVENTS, ipcErrorMessage } from "../ipc";
  import {
    deleteSource,
    listSources,
    syncAll,
    syncNow,
    type CredentialHealth,
    type SourceSummary,
    type SourceSyncStatus,
  } from "../ipc/sources";
  import Modal from "../shell/Modal.svelte";
  import { health as sharedHealth, type Health } from "../shell/health.svelte";
  import { push } from "../shell/toasts.svelte";
  import AddSource from "./AddSource.svelte";
  import ReenterSecret from "./ReenterSecret.svelte";
  import SourceRow from "./SourceRow.svelte";

  let {
    now = new Date(),
    health = sharedHealth,
  }: {
    /** Injectable clock, so ages and countdowns are testable. */
    now?: Date;
    /**
     * The live `source:health` store.
     *
     * A prop with the shell's singleton as its default: the view is correct
     * whatever a caller remembers to pass, and a test can hand it a store with
     * no Tauri bridge behind it.
     */
    health?: Health;
  } = $props();

  let sources = $state<SourceSummary[]>([]);
  let error = $state<string | null>(null);
  /** Live sync status per source id, from `sync:state`. */
  let statuses = $state<Record<string, SourceSyncStatus>>({});
  /** The source whose *Re-enter* strip is open, if any. */
  let fixing = $state<string | null>(null);
  /** The source a delete confirm is asking about, if any. */
  let deleting = $state<SourceSummary | null>(null);
  /** Whether the Add-source dialog is up. */
  let adding = $state(false);
  let purge = $state(false);

  async function load() {
    try {
      const rows = await listSources();
      sources = rows;
      error = null;
      // The rows carry health as of `list_sources`. Seeding the store from
      // them keeps the launcher's chips and this view reading one fact even
      // before the scheduler has emitted anything.
      for (const row of rows) health.patch(row.health);
    } catch (cause) {
      // Not a silent empty list: "No sources yet" is a claim about the
      // database, and the view does not have one to make — it knows only that
      // it could not ask.
      error = ipcErrorMessage(cause);
    }
  }

  $effect(() => {
    let dead = false;
    let off: (() => void) | undefined;

    void listen<SourceSyncStatus>(EVENTS.syncState, (event) => {
      if (!dead) statuses = { ...statuses, [event.payload.source_id]: event.payload };
    })
      .then((unlisten) => {
        // `listen` is itself an `invoke`, so it resolves a tick or more later —
        // possibly after this component is gone.
        if (dead) unlisten();
        else off = unlisten;
      })
      .catch(() => {
        // A view that cannot subscribe still lists; it simply stops moving on
        // its own.
      });

    void load();

    return () => {
      dead = true;
      off?.();
    };
  });

  /** A locally-started run, before the first `sync:state` arrives. */
  function markStarted(sourceId: string) {
    statuses = {
      ...statuses,
      [sourceId]: {
        source_id: sourceId,
        running: true,
        run_id: null,
        started_at: now.toISOString(),
        last_finished_at: statuses[sourceId]?.last_finished_at ?? null,
        last_outcome: statuses[sourceId]?.last_outcome ?? null,
        next_run_at: null,
        backoff_until: null,
      },
    };
  }

  async function sync(source: SourceSummary) {
    markStarted(source.id);
    try {
      await syncNow(source.id);
    } catch (cause) {
      statuses = { ...statuses, [source.id]: { ...statuses[source.id]!, running: false } };
      push({ text: `${source.display_name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function all() {
    try {
      for (const source of sources) if (source.enabled) markStarted(source.id);
      await syncAll();
    } catch (cause) {
      push({ text: ipcErrorMessage(cause), tone: "err" });
    }
  }

  async function confirmDelete() {
    const target = deleting;
    if (!target) return;
    deleting = null;
    try {
      await deleteSource(target.id, purge);
      push({ text: `${target.display_name} removed.` });
    } catch (cause) {
      push({ text: `Could not remove ${target.display_name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
    purge = false;
    // A mutation changes the row set, so this is the case that re-lists.
    await load();
  }

  function onhealth(next: CredentialHealth) {
    health.patch(next);
    fixing = null;
  }
</script>

<div class="view">
  <div class="room-bar">
    <h1>Sources <span class="k">{sources.length}</span></h1>
    <span class="acts">
      {#if sources.length > 0}
        <button class="btn sm" onclick={() => void all()}>Sync all</button>
      {/if}
      <button class="btn sm pri" onclick={() => (adding = true)}>Add source</button>
    </span>
  </div>

  <div class="view-b">
    {#if error}
      <div class="empty">
        <p>Could not read the configured sources.</p>
        <!-- Text: an `IpcError.message` can carry an upstream server's words. -->
        <p class="mono fail">{error}</p>
        <button class="btn" onclick={() => void load()}>Retry</button>
      </div>
    {:else}
      {#if sources.length > 0}
        <div class="src hd" aria-hidden="true">
          <span></span>
          <span>Source</span>
          <span>Address</span>
          <span>Credential</span>
          <span>Sync</span>
          <span></span>
        </div>
      {/if}

      {#each sources as source (source.id)}
        <SourceRow
          {source}
          {now}
          health={health.get(source.id)}
          status={statuses[source.id] ?? null}
          onsync={() => void sync(source)}
          onreenter={() => (fixing = source.id)}
          ondelete={() => {
            purge = false;
            deleting = source;
          }}
        />
        {#if fixing === source.id}
          <ReenterSecret
            sourceId={source.id}
            displayName={source.display_name}
            {onhealth}
            oncancel={() => (fixing = null)}
          />
        {/if}
      {/each}

      {#if sources.length === 0}
        <div class="empty">
          <p>No sources are configured yet.</p>
          <p>
            knobas mirrors work from the systems you point it at. Add one and its tickets, pull
            requests and builds become searchable here.
          </p>
          <button class="btn pri" onclick={() => (adding = true)}>Add source</button>
        </div>
      {/if}
    {/if}
  </div>
</div>

{#if adding}
  <AddSource
    onclose={() => (adding = false)}
    onsaved={(source) => {
      adding = false;
      health.patch(source.health);
      push({ text: `${source.display_name} added.` });
      // A new row is a change to the row set, so this re-lists.
      void load();
    }}
  />
{/if}

{#if deleting}
  <Modal title="Remove {deleting.display_name}?" center onclose={() => (deleting = null)}>
    {#snippet body()}
      <p>
        <b>{deleting?.display_name}</b> ({deleting?.id}) stops syncing and its configuration is
        removed. The credential in the OS keychain goes with it.
      </p>
      <label class="chk">
        <input type="checkbox" bind:checked={purge} />
        Also delete the {deleting?.item_count} mirrored items. They stop being searchable, and the
        only way back is a full re-sync.
      </label>
    {/snippet}
    {#snippet footer()}
      <button class="btn" onclick={() => (deleting = null)}>Cancel</button>
      <button class="btn danger" onclick={() => void confirmDelete()}>Delete source</button>
    {/snippet}
  </Modal>
{/if}

<style>
  /*
    The column headings. `.src.hd` reuses the row grid; `.row.hd`'s type in
    `app.css` belongs to a different grid, so the type is set here rather than
    by borrowing a class that would also re-lay the columns.
  */
  .src.hd {
    min-height: 22px;
    padding-top: 6px;
    padding-bottom: 6px;
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--faint);
    background: var(--bg);
  }

  .empty p + p {
    margin-top: 6px;
    max-width: 62ch;
  }

  .fail {
    color: var(--fail);
  }

  .chk {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin-top: 12px;
    font-size: 12px;
    line-height: 1.5;
  }

  .chk input {
    accent-color: var(--text);
    margin-top: 2px;
  }
</style>
