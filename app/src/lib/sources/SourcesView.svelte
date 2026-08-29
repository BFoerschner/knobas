<!--
  The sources view (`signal-miller.html`'s `renderSources`).

  Spec §3: add a source, watch it sync, see when its credential stops working,
  and fix it. The list is the whole view; the diagnostics below it are where a
  reader goes when a sync misbehaves (task 19).

  ## What re-lists, and what does not

  A **running** `sync:state` only patches the matching row. The event carries a
  whole `SourceSyncStatus` (contract §2.3), so re-listing to learn what the
  event already said would make a five-source sync fetch this view twice per
  source for nothing.

  A **terminal** `sync:state` re-lists, because the row's sync columns are the
  half the event does *not* carry: "synced 10 min ago" and the item count come
  off `SourceSummary` — `last_run` and `item_count` — and only `list_sources`
  moves those. Refreshing the diagnostics and not the row left the view
  disagreeing with itself, the panel showing the run that had just finished
  above a row still showing the state before it (#83).

  A **mutation** re-lists too, because add/delete change the row *set* and
  nothing else tells the view about it.

  A **credential fixed in the strip** re-lists as well, for a different reason:
  it does not change the row set, it changes the shared health store *out of
  band*. `load()` replaces that store wholesale (see below), so a read already
  in flight when the password was typed lands afterwards and writes the
  rejected credential back over the green chip. Re-listing here is what makes
  that read stale, so its answer is dropped instead of applied (#144).

  Nothing else in this view skips the re-list. Adding a path that changes a row
  or the health store without one puts it back in the same race.
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
  import { latestRead } from "../shell/latest-read";
  import Modal from "../shell/Modal.svelte";
  import { health as sharedHealth, type Health } from "../shell/health.svelte";
  import { push } from "../shell/toasts.svelte";
  import AddSource from "./AddSource.svelte";
  import Diagnostics from "./Diagnostics.svelte";
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
  /** Bumped on every finished run, so the diagnostics re-read themselves. */
  let transitions = $state(0);
  let purge = $state(false);

  /**
   * Only the newest `list_sources` is allowed to write what it read.
   *
   * A terminal `sync:state` arrives once per source, so a five-source *Sync
   * all* now puts five `list_sources` in flight at once and nothing makes them
   * answer in the order they were asked. Whichever landed last used to win, so
   * a slow early read could write the snapshot from *before* the run that had
   * just finished — #83's own symptom, arriving through the fix for it. A read
   * that has been overtaken drops its answer instead, including its failure:
   * a stale rejection must not blank a list that has since been read fine.
   *
   * `latestRead` rather than a counter written out here, because the settings
   * surface needs the same guard and a hand-copied one loses the rejection
   * half (#107).
   */
  const read = latestRead<SourceSummary[]>();

  function load() {
    return read(listSources, {
      ok: (rows) => {
        sources = rows;
        error = null;
        // The rows carry health as of `list_sources`, and they are the whole
        // set — so this *replaces* rather than patches. Patching kept the
        // launcher's chips and this view reading one fact, but it could only
        // ever add: a source deleted below stayed in the store, drawing its
        // top-strip monogram and its room tab until the window was restarted.
        health.replace(rows.map((row) => row.health));
      },
      fail: (cause) => {
        // Not a silent empty list: "No sources yet" is a claim about the
        // database, and the view does not have one to make — it knows only
        // that it could not ask.
        error = ipcErrorMessage(cause);
      },
    });
  }

  $effect(() => {
    let dead = false;
    let off: (() => void) | undefined;

    void listen<SourceSyncStatus>(EVENTS.syncState, (event) => {
      if (dead) return;
      statuses = { ...statuses, [event.payload.source_id]: event.payload };
      // A run that has *finished* is a new row in the sync log and new numbers
      // in the `.dbbar`. Counting transitions rather than re-fetching here
      // keeps the decision to re-read where the reading lives.
      if (!event.payload.running) {
        transitions += 1;
        // …and it is also a new `last_run` and a new `item_count` on the row
        // itself, which live on `SourceSummary` and arrive only from
        // `list_sources`. One signal, both readings: the panel and the row
        // above it are one view and a reader compares them (#83). The re-list
        // carries the shell's credential health with it, because `load()`
        // replaces that store from the same rows — a run that has just
        // discovered a rejected credential says so in the top strip too.
        void load();
      }
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
    // The reading `set_source_secret` answered with, applied at once: the
    // person is watching the chip they just pressed a button to fix, and a
    // round trip later is not when they are looking.
    health.patch(next);
    fixing = null;
    // …and then a re-list, because that patch is *out of band* and `load()`
    // replaces this store wholesale. A read issued before the password was
    // typed and landing after it writes the rejected credential back over the
    // green chip, with no action of the reader's — and re-listing on every
    // terminal `sync:state` is what makes that window an ordinary one rather
    // than an exotic one (#144). Issuing the read is itself the fix: it makes
    // the in-flight one stale, so its answer is dropped rather than applied.
    //
    // `void`, not `await`, and for the same reason as `onsaved` below: this is
    // a synchronous callback prop whose caller does not hold what it returns,
    // so an `async` version would leave a promise nobody is holding. Nothing
    // here needs the answer either — `latestRead` stamps the read the moment
    // it is called, before its first `await`, so the older read is already
    // overtaken by the time this line returns. `confirmDelete` awaits because
    // it is already async and the re-list is the last half of what it does.
    void load();
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

      {#if sources.length > 0}
        <!--
          Below the list, not beside it: the diagnostics answer a question a
          reader asks *after* looking at a row and finding it odd.
          `reloadKey` moves on every sync transition, so a run that just
          finished appears in the log without the reader refreshing.
        -->
        <Diagnostics {now} reloadKey={transitions} />
      {/if}

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
