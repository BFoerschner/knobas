<!--
  Settings › Notes and capture (#832, R-13a.6, D10): prototype-6's
  `STGV.notes`.

  Notes and quick capture hide for one release. Their data and commands stay;
  this section is where the reader turns them back on, sees how many notes
  there are, and takes them out as Markdown. The switch is the backend's
  setting, because the capture shortcut is registered in Rust: off, nothing is
  registered (`crate::capture`, `capture.notes_shown`).

  *Move to Confluence…* (#941) opens the standup's page picker
  (`PublishTargetPicker`): each note becomes a page under the one picked, one
  `create_page` each through the write queue (`move_notes_to_confluence`).
  The notes stay (D10).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { CaptureShortcut, PublishTarget } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import PublishTargetPicker from "../../standup/PublishTargetPicker.svelte";
  import { publishableSources, type PublishableSource } from "../../standup/protocol";
  import type { Layers } from "../layers.svelte";
  import { homePath, keyCaps, noteCount, notesLine } from "./notes";
  import { realNotesPorts, type NotesPorts } from "./notes-ports";
  import TfDialog from "./TfDialog.svelte";

  let {
    ports = realNotesPorts,
    now = () => new Date(),
    layers,
  }: {
    ports?: NotesPorts;
    now?: () => Date;
    layers?: Layers;
  } = $props();

  let view = $state<CaptureShortcut | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let moving = $state(false);
  let sources = $state<PublishableSource[]>([]);
  let chosen = $state<PublishTarget | null>(null);
  let sending = $state(false);

  function load() {
    error = null;
    ports
      .captureShortcut()
      .then((answer) => (view = answer))
      .catch((cause) => (error = ipcErrorMessage(cause)));
  }

  $effect(() => {
    load();
  });

  async function toggle() {
    if (view === null) return;
    busy = true;
    try {
      view = await ports.setNotesShown(!view.shown);
    } catch (cause) {
      push({ text: `Could not change the setting: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      busy = false;
    }
  }

  async function exportNotes() {
    busy = true;
    try {
      const written = await ports.exportNotes();
      push({ text: `Saved ${noteCount(written.count)} as Markdown to ${homePath(written.folder)}` });
    } catch (cause) {
      push({ text: `Could not export the notes: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      busy = false;
    }
  }

  async function openMove() {
    chosen = null;
    try {
      const [configured, descriptors] = await Promise.all([ports.listSources(), ports.listAdapters()]);
      sources = publishableSources(configured, descriptors);
    } catch (cause) {
      push({ text: `Could not read the sources: ${ipcErrorMessage(cause)}`, tone: "err" });
      return;
    }
    // The picker presumes the one Confluence when it mounts, so it mounts
    // once the sources are read.
    moving = true;
  }

  const pageCount = (n: number) => (n === 1 ? "1 page" : `${n} pages`);

  async function move() {
    if (chosen === null) return;
    sending = true;
    try {
      const queued = await ports.moveNotes(chosen.parent);
      moving = false;
      push({ text: `Queued ${pageCount(queued.length)} in Confluence. Your notes stay in knobas.` });
    } catch (cause) {
      push({ text: `Could not move the notes: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      sending = false;
    }
  }

  const shown = $derived(view?.shown ?? false);
  const help = $derived.by(() => {
    if (!shown) return "Inactive while notes are hidden.";
    if (view?.refusal) return `Not registered: ${view.refusal}`;
    return "Opens a small window over whatever you are doing.";
  });
</script>

<header class="tf-stg-h">
  <h1>Notes and capture</h1>
  <p>Pages are the place for documents in this version. Your notes are kept, and can come back.</p>
</header>
{#if error !== null}
  <p class="tf-stg-p tf-stg-errt">Could not read the notes setting: {error}</p>
  <div class="tf-stg-acts"><button class="tf-btn" onclick={load}>Try again</button></div>
{:else if view !== null}
  <div class="tf-stg-row">
    <div class="tf-stg-l">
      Show notes and quick capture<small
        >Off by default in this release. On, notes are back in search and the capture shortcut works again.</small
      >
    </div>
    <div class="tf-stg-c">
      <button
        class="tf-stg-sw"
        role="switch"
        aria-checked={shown}
        aria-label="Show notes"
        disabled={busy}
        onclick={() => void toggle()}><i></i></button
      >
    </div>
  </div>
  <div class="tf-stg-row">
    <div class="tf-stg-l">Capture shortcut<small>{help}</small></div>
    <div class="tf-stg-c">
      {#if view.accelerator === null}
        <span>Not set</span>
      {:else}
        {#each keyCaps(view.accelerator) as cap, index (index)}<kbd>{cap}</kbd>{/each}
      {/if}
    </div>
  </div>
  <div class="tf-stg-row">
    <div class="tf-stg-l">Your notes</div>
    <div class="tf-stg-c"><span>{notesLine(view.note_count, view.last_note_at, now())}</span></div>
  </div>
  <div class="tf-stg-acts">
    <button class="tf-btn" disabled={busy} onclick={() => void exportNotes()}>Export notes as Markdown</button>
    <button class="tf-btn tf-ghost" disabled={view.note_count === 0} onclick={() => void openMove()}
      >Move to Confluence…</button
    >
  </div>
{/if}

{#if moving}
  <TfDialog title="Move notes to Confluence" onclose={() => (moving = false)} {layers}>
    {#snippet body()}
      <p class="tf-dsub">Each note becomes a page under the one you pick. Your notes stay in knobas.</p>
      <div class="tf-stg-pick">
        <PublishTargetPicker {sources} search={ports.search} onchange={(target) => (chosen = target)} />
      </div>
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn" onclick={() => (moving = false)}>Cancel</button>
      <button class="tf-btn tf-primary" disabled={sending || chosen === null} onclick={() => void move()}
        >Move {noteCount(view?.note_count ?? 0)}</button
      >
    {/snippet}
  </TfDialog>
{/if}
