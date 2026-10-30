<!--
  Settings › Backup and export (#831, R-13a.7): prototype-6's `STGV.backup`
  over the commands the rooms UI's `BackupSection` uses: `backup_status`,
  `backup_now`, `set_backup_schedule`, `share_export` and `restore_backup`.

  knobas has no file picker and `restore_backup` takes an archive in the
  backup folder, so *Restore from a file…* and *Import…* open a dialog that
  lists the archives in that folder: backups for the one, share exports for
  the other. Both restore only into a knobas that holds no data yet, which the
  dialog says, since a merge with a preview is not built.

  Every write re-reads, because an export also prunes past `keep`;
  `set_backup_schedule` answers with the status itself.

  The folder is read again when either dialog opens and when the window comes
  back into focus (#1470): the list is the only way in (#831), and the dialog
  asks the reader to put a file there. Until the dialog's own read lands it
  lists nothing rather than the folder as it was. *Open the backup folder*
  (#1471) opens that folder, since it sits under the hidden `~/Library`; the
  `open_backup_folder` command picks the running profile's (#1704).
-->
<script lang="ts">
  import { pageVisible } from "../kept-pages.svelte";
  import { onFocusWhileShown } from "../while-shown.svelte";
  import { ipcErrorMessage } from "../../ipc";
  import { shareDefaults, type ArchiveFile, type BackupSchedule, type BackupStatus, type ShareParts } from "../../ipc/backup";
  import { latestRead } from "../../shell/latest-read";
  import { push } from "../../shell/toasts.svelte";
  import { formatBytes } from "../../sources/diagnostics";
  import type { Layers } from "../layers.svelte";
  import type { SectionPorts } from "./ports";
  import { boundedNumber, lastBackupWords, scheduleWords, shareTicks } from "./sections";
  import TfDialog from "./TfDialog.svelte";

  let {
    ports,
    layers,
    now,
    onrestored = () => {},
  }: {
    ports: Pick<
      SectionPorts,
      | "backupStatus"
      | "backupNow"
      | "setBackupSchedule"
      | "shareExport"
      | "restoreBackup"
      | "smartLists"
      | "ticketViews"
      | "linkSets"
      | "openBackupFolder"
    >;
    layers: Layers;
    now: Date;
    /** A restore brought the archive's sources: the source list is read again (#2059). */
    onrestored?: () => void;
  } = $props();

  let status = $state<BackupStatus | null>(null);
  let error = $state<string | null>(null);
  let savedLists = $state(false);
  /** Whether a ticket view exists (#812): its tick, on `savedLists`' terms. */
  let savedViews = $state(false);
  /** Whether a link set exists (#870): its tick, on `savedLists`' terms. */
  let savedSets = $state(false);
  /** The share ticks: the ratified defaults, smart lists, ticket views and link sets only when one exists. */
  let parts = $state<ShareParts>({ ...shareDefaults, smart_lists: false, ticket_views: false, link_sets: false });
  let busy = $state(false);
  /** Which archives the open dialog lists, or `null` when none is open. */
  let choosing = $state<"backup" | "share" | null>(null);

  const read = latestRead<BackupStatus>();
  /**
   * Whether the open dialog waits for its own read of the folder (#1470).
   * `latestRead` lands only the newest read, so whatever lands after `choose`
   * started one is that read or a newer one.
   */
  let reading = $state(false);
  async function load() {
    await read(ports.backupStatus, {
      ok: (next) => {
        status = next;
        error = null;
        reading = false;
      },
      // A failure needs no `reading = false`: the failure line replaces the
      // dialog, and *Try again* lands a read that clears it.
      fail: (cause) => (error = ipcErrorMessage(cause)),
    });
  }

  /** Open a dialog over the folder as it is now, not as the section found it (#1470). */
  function choose(kind: "backup" | "share") {
    choosing = kind;
    reading = true;
    void load();
  }

  // A file dropped in Finder shows when the reader comes back (#1470), as the
  // repo page's *Local changes* does; not while the page is hidden (#2249).
  onFocusWhileShown(pageVisible(), () => void load());

  async function openFolder() {
    try {
      await ports.openBackupFolder();
    } catch (cause) {
      push({ text: `Could not open the backup folder: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  $effect(() => {
    void load();
    void ports
      .smartLists()
      .then((lists) => {
        savedLists = lists.some((list) => list.saved);
        parts = { ...parts, smart_lists: savedLists };
      })
      // Not knowing is the safe half: no tick, and the part goes off.
      .catch(() => (savedLists = false));
    void ports
      .ticketViews()
      .then((views) => {
        savedViews = views.length > 0;
        parts = { ...parts, ticket_views: savedViews };
      })
      .catch(() => (savedViews = false));
    void ports
      .linkSets()
      .then((sets) => {
        savedSets = sets.length > 0;
        parts = { ...parts, link_sets: savedSets };
      })
      .catch(() => (savedSets = false));
  });

  async function saveSchedule(next: BackupSchedule) {
    try {
      const saved = await ports.setBackupSchedule(next);
      // Read after the write, so newer than any read still in flight.
      await read(() => Promise.resolve(saved), { ok: (current) => (status = current), fail: () => {} });
    } catch (cause) {
      push({ text: `Could not save the schedule: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function backUpNow() {
    busy = true;
    try {
      const made = await ports.backupNow();
      push({ text: `Backed up: ${formatBytes(made.bytes)}` });
      await load();
    } catch (cause) {
      push({ text: `Backup failed: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      busy = false;
    }
  }

  async function exportShare() {
    busy = true;
    try {
      const made = await ports.shareExport({
        ...parts,
        smart_lists: savedLists && parts.smart_lists,
        ticket_views: savedViews && parts.ticket_views,
        link_sets: savedSets && parts.link_sets,
      });
      push({ text: `Saved ${made.file} to ${status?.directory ?? "the backup folder"}`, ms: 15_000 });
      await load();
    } catch (cause) {
      push({ text: `Export failed: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      busy = false;
    }
  }

  async function restore(archive: ArchiveFile) {
    busy = true;
    try {
      await ports.restoreBackup(archive.file);
      choosing = null;
      onrestored();
      push({ text: `Restored from ${archive.file}. Restart knobas so every view reads the restored database.`, ms: 30_000 });
    } catch (cause) {
      // The dialog stays: the refusal ("already holds knobas data") is the answer.
      push({ text: `Restore refused: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      busy = false;
    }
  }

  const ticks = $derived(shareTicks(savedLists, savedViews, savedSets));
  const nothingTicked = $derived(!ticks.some((tick) => parts[tick.part]));
  const listed = $derived(status?.archives.filter((archive) => archive.share === (choosing === "share")) ?? []);
</script>

<header class="tf-stg-h">
  <h1>Backup and export</h1>
  <p>The database is yours. Back it up, restore it, or share part of it.</p>
</header>
{#if error !== null}
  <p class="tf-stg-p tf-stg-errt">Could not read the backup settings: {error}</p>
  <div class="tf-stg-acts"><button class="tf-btn" onclick={() => void load()}>Try again</button></div>
{:else if status !== null}
  {@const current = status}
  <div class="tf-stg-row">
    <div class="tf-stg-l">Last backup</div>
    <div class="tf-stg-c"><span>{lastBackupWords(current.last, current.directory, now)}</span></div>
  </div>
  <div class="tf-stg-row">
    <label class="tf-stg-l" for="tf-bak-every">Back up</label>
    <div class="tf-stg-c">
      <select
        id="tf-bak-every"
        class="tf-stg-in"
        value={current.schedule.enabled ? "daily" : "never"}
        onchange={(event) => void saveSchedule({ ...current.schedule, enabled: event.currentTarget.value === "daily" })}
      >
        <option value="daily">{scheduleWords(current.schedule)}</option>
        <option value="never">Never</option>
      </select>
    </div>
  </div>
  <div class="tf-stg-row">
    <label class="tf-stg-l" for="tf-bak-keep">Keep</label>
    <div class="tf-stg-c tf-stg-inl">
      <input
        id="tf-bak-keep"
        type="number"
        min="1"
        max="365"
        class="tf-stg-in tf-stg-n70"
        value={current.schedule.keep}
        onchange={(event) => {
          const keep = boundedNumber(event.currentTarget.value, 1, 365, current.schedule.keep);
          event.currentTarget.value = String(keep);
          if (keep !== current.schedule.keep) void saveSchedule({ ...current.schedule, keep });
        }}
      />
      <span>backups</span>
    </div>
  </div>
  <div class="tf-stg-acts">
    <button class="tf-btn" disabled={busy} onclick={() => void backUpNow()}>Back up now</button>
    <button class="tf-btn tf-ghost" onclick={() => choose("backup")}>Restore from a file…</button>
  </div>
  <div class="tf-stg-sec">
    <h2>Share with a colleague</h2>
    <p class="tf-stg-p">An export they can import. Passwords never go in it.</p>
    <div class="tf-stg-checks">
      {#each ticks as tick (tick.part)}
        <label class="tf-ck"><input type="checkbox" bind:checked={parts[tick.part]} /> {tick.label}</label>
      {/each}
    </div>
    <div class="tf-stg-acts">
      <button class="tf-btn tf-primary" disabled={busy || nothingTicked} onclick={() => void exportShare()}>Export…</button>
      <button class="tf-btn tf-ghost" onclick={() => choose("share")}>Import…</button>
      <button class="tf-btn tf-ghost" onclick={() => void openFolder()}>Open the backup folder</button>
    </div>
  </div>

  {#if choosing !== null}
    <TfDialog
      title={choosing === "share" ? "Import a share export" : "Restore from a file"}
      onclose={() => (choosing = null)}
      {layers}
    >
      {#snippet body()}
        <p class="tf-dsub">
          {choosing === "share" ? "Share exports" : "Backups"} in <span class="tf-mono">{current.directory}</span>. Put a
          file there to see it here.
        </p>
        <div class="tf-stg-acts">
          <button class="tf-btn tf-sm" onclick={() => void openFolder()}>Open the backup folder</button>
        </div>
        <p class="tf-dsub">
          This works only into a knobas that holds no data yet; one that has data refuses it. Passwords are not in the
          file, so each source asks for its own again.
        </p>
        {#if reading}
          <p class="tf-stg-note">Reading the folder…</p>
        {:else if listed.length === 0}
          <p class="tf-stg-note">{choosing === "share" ? "No share exports there yet." : "No backups there yet."}</p>
        {:else}
          <table class="tf-tbl tf-stg-gap">
            <tbody>
              {#each listed as archive (archive.file)}
                <tr>
                  <td class="tf-mono">{archive.file}</td>
                  <td class="tf-num tf-stg-r">{formatBytes(archive.bytes)}</td>
                  <td class="tf-num tf-stg-r">
                    <button class="tf-btn tf-sm" disabled={busy} onclick={() => void restore(archive)}>
                      {choosing === "share" ? "Import" : "Restore"}
                    </button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      {/snippet}
      {#snippet footer()}
        <button class="tf-btn" onclick={() => (choosing = null)}>Cancel</button>
      {/snippet}
    </TfDialog>
  {/if}
{/if}
