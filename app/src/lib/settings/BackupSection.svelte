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
  import { backupNow, backupStatus, type BackupStatus } from "../ipc/backup";
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
  <span class="acts">
    <button class="btn sm" disabled={exporting} onclick={() => void exportNow()}>
      {exporting ? "Exporting…" : "Export now"}
    </button>
  </span>
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
</style>
