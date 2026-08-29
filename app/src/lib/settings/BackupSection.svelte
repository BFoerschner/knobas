<!--
  The **Backup** section of the settings view — spec §14's "small settings
  surface (export now / schedule / restore) … plain dialogs are fine", issue
  #69.

  The engine underneath is already headless and already ratified (issue #38,
  PR #67): this file is a surface over `backup_status`, `backup_now`,
  `set_backup_schedule` and `restore_backup`, and it changes neither side.

  ## One read, and what re-reads it

  `backup_status` answers with the schedule, the last export, the directory and
  what is on disk — the whole section in one call, which is why there is no
  second fetch here. Every mutation re-reads: an export writes a file *and*
  prunes older ones, so the archive list after one is not the list before it
  plus a row. `set_backup_schedule` is the exception that needs no re-read: it
  answers with the redrawn status itself, from the same transaction that stored
  the change, so the next-run line can never disagree with the schedule that
  produced it.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    backupNow,
    backupStatus,
    restoreBackup,
    setBackupSchedule,
    type ArchiveFile,
    type BackupSchedule,
    type BackupStatus,
  } from "../ipc/backup";
  import Modal from "../shell/Modal.svelte";
  import { ago } from "../shell/time";
  import { push } from "../shell/toasts.svelte";
  import { formatBytes } from "../sources/diagnostics";
  import { nightlySentence, retentionSentence } from "./schedule";

  let {
    now = new Date(),
  }: {
    /** Injectable clock, so the ages are testable rather than waited for. */
    now?: Date;
  } = $props();

  let status = $state<BackupStatus | null>(null);
  let error = $state<string | null>(null);
  /** Whether an export is in flight — see `exportNow` for why it is here. */
  let exporting = $state(false);
  /**
   * The schedule the dialog edits.
   *
   * A copy, not the live one: *Cancel* has to leave the section reading what
   * is actually stored, and editing `status.schedule` in place would have
   * already changed the sentence behind the dialog.
   *
   * **Always an object, with a separate flag for whether the dialog is up.**
   * The obvious `BackupSchedule | null` does not survive `bind:value`: Svelte
   * reads a binding's getter again on a later tick, and by then closing the
   * dialog has made the object `null` — so every close threw
   * `Cannot read properties of null` into a promise nothing awaits, which no
   * assertion sees and which fails the run anyway. Keeping the draft alive
   * past the close costs one stale object and removes the whole class.
   */
  let draft = $state<BackupSchedule>({ enabled: true, hour: 3, minute: 0, keep: 7 });
  let editing = $state(false);
  let saving = $state(false);
  /** The archive a restore confirm is asking about, if any. */
  let restoring = $state<ArchiveFile | null>(null);
  let restoreInFlight = $state(false);

  async function load() {
    try {
      status = await backupStatus();
      error = null;
    } catch (cause) {
      // Not an empty archive list: "no backups yet" is a claim about the disk,
      // and a section that could not ask has not earned it. The same rule the
      // sources view follows for `list_sources`.
      error = ipcErrorMessage(cause);
    }
  }

  $effect(() => {
    void load();
  });

  /** Open the dialog on a copy of the schedule that is in force. */
  function openSchedule() {
    if (!status) return;
    draft = { ...status.schedule };
    editing = true;
  }

  /**
   * A number field's value, never `NaN`.
   *
   * An `<input type="number">` a person has cleared reads as an empty string,
   * which `Number()` makes `NaN` and `JSON.stringify` makes `null` — and the
   * Rust `u32` on the other side refuses to decode that, so the save comes
   * back as a rejection instead of a saved schedule. Clearing a field before
   * typing into it is the ordinary way to use one, so this is the common path.
   *
   * Clamped to the same range `BackupSchedule::clamped` uses, and for its
   * reason: `keep: 0` would delete the archive the next export had just
   * written.
   */
  function bounded(value: string, min: number, max: number, fallback: number): number {
    const parsed = Number.parseInt(value, 10);
    if (Number.isNaN(parsed)) return fallback;
    return Math.min(max, Math.max(min, parsed));
  }

  /**
   * Store the edited schedule and redraw from the answer.
   *
   * `set_backup_schedule` returns the whole status, read back after the write,
   * so the next-run line and the sentence can never disagree with the schedule
   * that produced them. Redrawing from the *typed* values instead would show a
   * schedule that is merely believed to be stored.
   */
  async function saveSchedule() {
    const next: BackupSchedule = {
      enabled: draft.enabled,
      hour: bounded(String(draft.hour), 0, 23, 3),
      minute: bounded(String(draft.minute), 0, 59, 0),
      keep: bounded(String(draft.keep), 1, 365, 7),
    };
    saving = true;
    try {
      status = await setBackupSchedule(next);
      error = null;
      editing = false;
    } catch (cause) {
      push({ text: `Could not save the schedule: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      saving = false;
    }
  }

  /**
   * Restore the archive the confirm was asking about.
   *
   * Nothing is re-read afterwards, and that is not an omission: a restore
   * replaces the *database*, and every store in this window — credential
   * health, the room tabs, the launcher's corpus, this section's own status —
   * was read from the old one. Re-reading this one panel would make a single
   * corner of a stale window agree with the disk, which reads as the restore
   * having half worked. Saying "restart knobas" is the honest answer until
   * something owns re-hydrating the whole shell.
   */
  async function confirmRestore() {
    const target = restoring;
    if (!target) return;
    restoreInFlight = true;
    try {
      await restoreBackup(target.file);
      restoring = null;
      push({
        text: `Restored from ${target.file}. Restart knobas so every view reads the restored database.`,
        ms: 30_000,
      });
    } catch (cause) {
      // The refusal a person can act on — "already holds knobas data (n rows
      // in knobas.entity)" — is the message itself. The dialog stays open:
      // this is a decision that has not been made yet, not one that failed.
      push({ text: `Restore refused: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      restoreInFlight = false;
    }
  }

  /**
   * *Export now* — take a backup whatever the schedule says.
   *
   * The flag is not cosmetic. `pg_dump` over a real corpus takes long enough
   * that a button which does not visibly change reads as inert, and the
   * reader's answer to an inert button is to press it again; two archives a
   * second apart is a retention window one night shorter, for nothing.
   *
   * The re-read afterwards is a *re-read*, not an append: an export prunes
   * past `keep`, so the eighth one with the ratified `keep: 7` removes a file
   * as well as writing one. A list patched from the record would offer to
   * restore an archive that is no longer there.
   */
  async function exportNow() {
    exporting = true;
    try {
      const record = await backupNow();
      push({ text: `Backed up to ${record.file}.` });
      await load();
    } catch (cause) {
      // `pg_dump`'s own stderr rides in the message, because it is the only
      // thing that says *why*: a full disk and a missing tool are different
      // problems, and "Export failed" distinguishes neither.
      push({ text: `Export failed: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      exporting = false;
    }
  }
</script>

<div class="tile-h">
  <span class="lab">Backup</span>
  <!--
    Both actions are behind the read, not merely disabled by it. `backup_status`
    rejects with `not_ready` for the whole of bring-up, and an *Export now* in
    that window is a button that cannot work — it would reject on the same
    missing state and teach the reader nothing. Same rule the sources view
    applies to *Sync now* on a source that needs a password.
  -->
  {#if status && !error}
    <span class="acts">
      <button class="btn sm" onclick={openSchedule}>Schedule…</button>
      <button class="btn sm" disabled={exporting} onclick={() => void exportNow()}>
        {exporting ? "Exporting…" : "Export now"}
      </button>
    </span>
  {/if}
</div>

{#if error}
  <div class="empty">
    <p>Could not read the backup settings.</p>
    <!-- Text: an `IpcError.message` can carry `pg_dump`'s own stderr. -->
    <p class="mono fail">{error}</p>
    <button class="btn" onclick={() => void load()}>Retry</button>
  </div>
{:else if status}
  <div class="sec-b">
    <p>{nightlySentence(status.schedule)}</p>
    <p class="sub">{retentionSentence(status.schedule.keep)}</p>
    <p class="sub">
      Archives are written to <span class="mono">{status.directory}</span>. They contain
      everything knobas owns — links, contexts, notes, assets, worklogs — but not the synced
      mirror, which re-syncs.
    </p>

    <p class="sub">
      {#if status.last}
        Last export <span class="mono">{status.last.file}</span>, {formatBytes(status.last.bytes)},
        {ago(status.last.taken_at, now)}.
      {:else}
        No backup has been taken yet.
      {/if}
    </p>
  </div>

  <div class="tile-h">
    <span class="lab">Archives</span>
    <span class="cnt">{status.archives.length}</span>
  </div>

  {#if status.archives.length === 0}
    <div class="empty">
      <p>No archives on disk yet — nothing to restore.</p>
    </div>
  {:else}
    <!--
      Newest first, as `backup::archives` sorts them. The name carries its own
      date (`knobas-YYYYMMDD-HHMMSS`), which is why there is no second column
      re-deriving one here: that format is the Rust's to own, and a frontend
      that parsed it would be a second place to keep it right.
    -->
    {#each status.archives as archive (archive.file)}
      <div class="row g3 arc" data-file={archive.file}>
        <span class="mono">{archive.file}</span>
        <span class="r">{formatBytes(archive.bytes)}</span>
        <span class="r">
          <button class="btn sm" onclick={() => (restoring = archive)}>Restore</button>
        </span>
      </div>
    {/each}
  {/if}
{/if}

{#if restoring}
  <Modal title="Restore {restoring.file}?" center onclose={() => (restoring = null)}>
    {#snippet body()}
      <p>
        knobas reads <b>{restoring?.file}</b> ({formatBytes(restoring?.bytes ?? 0)}) back into this
        profile's database: links, contexts, notes, the asset tree, worklogs, smart lists and your
        source configurations. Stored credentials are not in the archive — the keychain is
        untouched — so each source asks for its password again.
      </p>
      <p>
        The synced mirror is not in the archive either, because it re-syncs. Every source starts
        from nothing and fills itself back in on its next run, so search is thin until it has.
      </p>
      <p>
        This only works into a knobas that <b>holds no data yet</b>. If this one already has
        anything of its own, the restore is refused rather than merged or overwritten — merging an
        archive into an existing corpus, with a preview of what changes, arrives in a later
        milestone.
      </p>
    {/snippet}
    {#snippet footer()}
      <button class="btn" onclick={() => (restoring = null)}>Cancel</button>
      <button class="btn danger" disabled={restoreInFlight} onclick={() => void confirmRestore()}>
        Restore
      </button>
    {/snippet}
  </Modal>
{/if}

{#if editing}
  <Modal title="Nightly backup" center onclose={() => (editing = false)}>
    {#snippet body()}
      <label class="chk">
        <input type="checkbox" bind:checked={draft.enabled} />
        Take a backup automatically
      </label>
      <div class="flds">
        <span class="fld">
          <label for="bk-hour">Hour</label>
          <input id="bk-hour" type="number" min="0" max="23" bind:value={draft.hour} />
        </span>
        <span class="fld">
          <label for="bk-minute">Minute</label>
          <input id="bk-minute" type="number" min="0" max="59" bind:value={draft.minute} />
        </span>
        <span class="fld">
          <label for="bk-keep">Keep</label>
          <input id="bk-keep" type="number" min="1" max="365" bind:value={draft.keep} />
        </span>
      </div>
      <!--
        The sentence, live, over the values being typed — so the boundary rule
        is read at the moment the time is chosen rather than after saving it.
      -->
      <p class="note">{nightlySentence(draft)}</p>
      <p class="note">{retentionSentence(draft.keep)}</p>
    {/snippet}
    {#snippet footer()}
      <button class="btn" onclick={() => (editing = false)}>Cancel</button>
      <button class="btn pri" disabled={saving} onclick={() => void saveSchedule()}>Save</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .sec-b {
    padding: 12px;
    font-size: 12px;
    line-height: 1.6;
  }

  .sec-b p {
    max-width: 78ch;
  }

  .sec-b p + p {
    margin-top: 8px;
  }

  .sub {
    color: var(--muted);
  }

  .fail {
    color: var(--fail);
  }

  .chk {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 12px;
  }

  .chk input {
    accent-color: var(--text);
  }

  .flds {
    display: flex;
    gap: 12px;
    margin-top: 12px;
  }

  .fld {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--muted);
  }

  .fld input {
    width: 76px;
    height: 24px;
    padding: 0 6px;
    border: 1px solid var(--hair2);
    border-radius: 2px;
    background: var(--bg);
    color: var(--text);
    font: 400 12px var(--mono);
  }

  .arc {
    grid-template-columns: 1fr 90px 90px;
    cursor: default;
  }

  .arc:hover {
    background: transparent;
  }

  .arc .btn {
    height: 20px;
  }

  .note {
    margin-top: 12px;
    max-width: 62ch;
    font-size: 11px;
    line-height: 1.5;
    color: var(--muted);
  }
</style>
