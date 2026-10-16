<!--
  Settings › Notes and capture (#832, R-13a.6, D10): prototype-6's
  `STGV.notes`.

  Notes and quick capture hide for one release. Their data and commands stay;
  this section is where the reader turns them back on, sees how many notes
  there are, and takes them out as Markdown. The switch is the backend's
  setting, because the capture shortcut is registered in Rust: off, nothing is
  registered (`crate::capture`, `capture.notes_shown`).

  *Move to Confluence…* needs the Docs view's page picker, which TF4 builds;
  until then it says so.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { CaptureShortcut } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import { homePath, keyCaps, noteCount, notesLine } from "./notes";
  import { realNotesPorts, type NotesPorts } from "./notes-ports";

  let {
    ports = realNotesPorts,
    now = () => new Date(),
  }: {
    ports?: NotesPorts;
    now?: () => Date;
  } = $props();

  let view = $state<CaptureShortcut | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

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

  function moveToConfluence() {
    push({
      text: "Moving notes to Confluence comes with the Docs view. Export notes as Markdown keeps them until then.",
    });
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
    <button class="tf-btn tf-ghost" onclick={moveToConfluence}>Move to Confluence…</button>
  </div>
{/if}
