<!--
  The **Backup** section of the settings view — spec §14's "small settings
  surface (export now / schedule / restore) … plain dialogs are fine", issue
  #69.

  The engine underneath is already headless and already ratified (issue #38,
  PR #67): this file is a surface over `backup_status`, `backup_now`,
  `set_backup_schedule` and `restore_backup`, and it changes neither side. The
  share export (#454) hangs off the same read and adds `share_export`.

  ## One read, and what re-reads it

  `backup_status` answers with the schedule, the last export, the directory and
  what is on disk — the whole section in one call, which is why there is no
  second fetch here. Every mutation re-reads: an export writes a file *and*
  prunes older ones, so the archive list after one is not the list before it
  plus a row. `set_backup_schedule` is the exception that needs no re-read: it
  answers with the redrawn status itself, from the same transaction that stored
  the change, so what this section says about the schedule cannot disagree with
  the schedule that is actually on disk.

  ## Why `next_due_at` is read and not shown

  `backup_status` carries it (`policy::next_due`, whose own doc says it is
  "for the settings dialog to show"), and this section deliberately renders no
  moment. #69 rules that the wording "should not promise a fixed time", and a
  timestamp is the most fixed-looking promise available: it would be right on a
  machine that stays awake and quietly wrong on the laptop the boundary rule
  exists for. The sentence above says the rule instead. Showing the prediction
  as well is a decision for whoever finds the rule alone insufficient.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    backupNow,
    backupStatus,
    restoreBackup,
    setBackupSchedule,
    shareDefaults,
    shareExport,
    type ArchiveFile,
    type BackupSchedule,
    type BackupStatus,
    type ShareParts,
  } from "../ipc/backup";
  import { latestRead } from "../shell/latest-read";
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
  /**
   * The share export's toggles, and whether its dialog is up.
   *
   * A live object with a separate flag, for the reason `draft` above is one:
   * `bind:checked` reads its getter again on a later tick, and a `null` by
   * then throws into a promise nothing awaits. Reset to the ratified defaults
   * on every open, so a dialog cancelled with notes switched on does not open
   * that way next time — a share export is a decision made once per export,
   * not a stored preference.
   */
  let shareDraft = $state<ShareParts>({ ...shareDefaults });
  let sharing = $state(false);
  let shareInFlight = $state(false);

  /** Whether the share dialog has anything to export. */
  const shareEmpty = $derived(!Object.values(shareDraft).some(Boolean));

  /**
   * The two kinds of archive, listed apart (#455).
   *
   * They sit in one directory and restore through one button, and they are not
   * the same thing: the retention sentence above the list is true of the
   * backups and false of the share exports, which retention never deletes. One
   * list under one heading says something wrong about half its rows.
   *
   * Split on `archive.share`, which the Rust puts on the row — the naming rule
   * (`knobas-share-…`) belongs to `backup::policy`, and a filter here that
   * matched the prefix would be a second place to keep it right. Each group
   * keeps the order the read came in, which is newest first.
   */
  const backups = $derived(status?.archives.filter((archive) => !archive.share) ?? []);
  const shares = $derived(status?.archives.filter((archive) => archive.share) ?? []);

  /**
   * Which `backup_status` is the current one.
   *
   * Two *Retry* presses over a failing read put two in flight at once, and
   * nothing makes them answer in the order they were asked. The same guard the
   * sources view puts on `list_sources`, and the same module, so the rejection
   * half cannot be dropped in one place and kept in the other (#107).
   */
  const read = latestRead<BackupStatus>();

  function load() {
    return read(backupStatus, {
      ok: (next) => {
        status = next;
        error = null;
      },
      fail: (cause) => {
        // Not an empty archive list: "no backups yet" is a claim about the
        // disk, and a section that could not ask has not earned it. The same
        // rule the sources view follows for `list_sources`.
        error = ipcErrorMessage(cause);
      },
    });
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
   * and the sentence is drawn from that answer. Redrawing from the *typed*
   * values instead would show a schedule that is merely believed to be stored
   * — the two differ whenever the stored row is not what was posted, which is
   * what `BackupSchedule::clamped` exists to make possible.
   *
   * That answer *is* a `backup_status`, so it is stamped like one. Written
   * straight to `status` it was the one write in this component outside the
   * guard, and a `backup_status` already in flight when the save started would
   * land on top of it — putting the schedule that had just been replaced back
   * on the screen while the disk holds the new one (#129).
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
      const saved = await setBackupSchedule(next);
      // The stamp is claimed here rather than around the round trip, and the
      // difference is the whole point: this answer was read *after* the write,
      // so it is newer than anything issued while the save was in flight.
      // Issuing the save through `read` would stamp it before the round trip
      // and let one of those older reads count as newer.
      await read(() => Promise.resolve(saved), {
        ok: (current) => {
          status = current;
          error = null;
          editing = false;
        },
        // Unreachable — the promise handed over is already resolved. The
        // failure path is the `catch` below, deliberately outside the stamp:
        // the currency rule decides what may be *written to the screen*, not
        // whether a save that failed is worth telling the person about.
        fail: () => {},
      });
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
  /** Open the share dialog on the ratified defaults. */
  function openShare() {
    shareDraft = { ...shareDefaults };
    sharing = true;
  }

  /**
   * Take a share export of the chosen parts.
   *
   * Re-reads afterwards for the reason *Export now* does, and a different one:
   * the archive lands in the same directory and is listed with the backups, so
   * the person who took it can see where it went and hand it over.
   *
   * A refused export leaves the dialog open — an empty selection is a decision
   * not yet made, the same rule the restore confirm follows.
   */
  async function confirmShare() {
    shareInFlight = true;
    try {
      const record = await shareExport({ ...shareDraft });
      sharing = false;
      push({ text: `Share export written to ${record.file}.`, ms: 15_000 });
      await load();
    } catch (cause) {
      push({ text: `Share export failed: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      shareInFlight = false;
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
      <button class="btn sm" onclick={openShare}>Share…</button>
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
      A <b>share export</b> is the same kind of archive cut down to the parts a colleague should
      get — links, assets, contexts and source configurations by default, with notes and time left
      out. It is never deleted by the retention above, and it restores through the same
      <i>Restore</i> below.
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

  {#if status.archives.length === 0}
    <div class="tile-h">
      <span class="lab">Archives</span>
      <span class="cnt">0</span>
    </div>
    <div class="empty">
      <p>No archives on disk yet — nothing to restore.</p>
    </div>
  {:else}
    <!--
      Two groups, never one (#455): a share export is not a backup, and the
      retention sentence above is true of one and false of the other.

      Newest first within each, as `backup::archives` sorts them. The name
      carries its own date (`knobas-YYYYMMDD-HHMMSS`), which is why there is no
      second column re-deriving one here: that format is the Rust's to own, and
      a frontend that parsed it would be a second place to keep it right.

      Each group is drawn only when it holds something, so a profile that has
      never taken a share export shows no empty heading for one.
    -->
    {#if backups.length > 0}
      <div data-group="backup">
        <div class="tile-h">
          <span class="lab">Backups</span>
          <span class="cnt">{backups.length}</span>
        </div>
        {#each backups as archive (archive.file)}
          <div class="row arc" data-file={archive.file}>
            <span class="mono">{archive.file}</span>
            <span class="r">{formatBytes(archive.bytes)}</span>
            <span class="r">
              <button class="btn sm" onclick={() => (restoring = archive)}>Restore</button>
            </span>
          </div>
        {/each}
      </div>
    {/if}

    {#if shares.length > 0}
      <div data-group="share">
        <div class="tile-h">
          <span class="lab">Share exports</span>
          <span class="cnt">{shares.length}</span>
        </div>
        <p class="sub grp">
          Made on purpose to hand over, so the retention above never deletes one. Each restores
          like any other archive — into a knobas that holds no data yet.
        </p>
        {#each shares as archive (archive.file)}
          <div class="row arc" data-file={archive.file}>
            <span class="mono">{archive.file}</span>
            <span class="r">{formatBytes(archive.bytes)}</span>
            <span class="r">
              <button class="btn sm" onclick={() => (restoring = archive)}>Restore</button>
            </span>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
{/if}

{#if restoring}
  <Modal title="Restore {restoring.file}?" center onclose={() => (restoring = null)}>
    {#snippet body()}
      <p>
        knobas reads <b>{restoring?.file}</b> ({formatBytes(restoring?.bytes ?? 0)}) back into this
        profile's database: links, contexts, notes, the asset tree, worklogs, smart lists and your
        source configurations. Stored credentials are not in the archive — the keychain is
        untouched — so a machine that has never held them asks for each source's password again.
      </p>
      <p>
        The synced mirror is not in the archive either, and a restore does not rebuild it: search
        is thin until each source has re-synced. Each restored source reads its system
        <b>from the top</b> — no archive carries a mirror, so there is no position left worth
        resuming from — which makes the first sync of each a full one, and a slow one on a large
        system.
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

{#if sharing}
  <Modal title="Share export" center onclose={() => (sharing = false)}>
    {#snippet body()}
      <p class="note top">
        Which parts of this knobas the archive carries. Everything else stays here: the activity
        stream, the synced mirror, and knobas's own settings — the recipient keeps their schedule
        and their preferences.
      </p>
      <label class="chk">
        <input type="checkbox" bind:checked={shareDraft.links} />
        Links, and the titles of what they join
      </label>
      <label class="chk">
        <input type="checkbox" bind:checked={shareDraft.assets} />
        Assets and their routes
      </label>
      <label class="chk">
        <input type="checkbox" bind:checked={shareDraft.contexts} />
        Contexts
      </label>
      <label class="chk">
        <input type="checkbox" bind:checked={shareDraft.notes} />
        Notes
      </label>
      <label class="chk">
        <input type="checkbox" bind:checked={shareDraft.time} />
        Time — the timer, its blocks and your worklogs
      </label>
      <label class="chk">
        <input type="checkbox" bind:checked={shareDraft.sources} />
        Source configurations
      </label>
      <!--
        Said here rather than only in the docs: an entity row is the *address*
        of a thing and the archive carries the whole address book, so a note's
        title travels with notes switched off even though its body does not.
        A person deciding what to hand over needs that in front of them.
      -->
      <p class="note">
        Titles travel with any part: a colleague restoring this sees the name of every ticket,
        page, asset and note knobas knows about, and the contents of only the parts ticked here.
        Credentials are never in an archive — they live in the keychain, and each source asks for
        its own on the other machine.
      </p>
      {#if shareEmpty}
        <p class="note fail">Nothing is ticked, so there is nothing to export.</p>
      {/if}
    {/snippet}
    {#snippet footer()}
      <button class="btn" onclick={() => (sharing = false)}>Cancel</button>
      <button
        class="btn pri"
        disabled={shareInFlight || shareEmpty}
        onclick={() => void confirmShare()}
      >
        {shareInFlight ? "Exporting…" : "Export"}
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

  /* The one-line note under a group heading, not inside the padded block. */
  .sub.grp {
    padding: 8px 12px 0;
    font-size: 11px;
    line-height: 1.5;
    max-width: 78ch;
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

  /*
    `.row` in `app.css` sets a five-column grid and a hover, both of which this
    row wants neither of — it is a list of files, not a list of things to open.
    Written as `.row.arc` rather than `.arc` so it outranks `.row` whatever
    order the two stylesheets end up in: a component `<style>` is scoped, not
    automatically later, and `vite build` concatenates the global sheet and the
    extracted component styles into one file.
  */
  .row.arc {
    grid-template-columns: 1fr 90px 90px;
    cursor: default;
  }

  .row.arc:hover {
    background: transparent;
  }

  .row.arc .btn {
    height: 20px;
  }

  .chk + .chk {
    margin-top: 6px;
  }

  .note.top {
    margin-top: 0;
    margin-bottom: 12px;
  }

  .note {
    margin-top: 12px;
    max-width: 62ch;
    font-size: 11px;
    line-height: 1.5;
    color: var(--muted);
  }
</style>
