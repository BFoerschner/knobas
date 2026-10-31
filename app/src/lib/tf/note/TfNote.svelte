<!--
  A note's read-only page, `#/note/<note id>` (#1504), while notes are shown
  (D10): the title, the body as it was typed with each resolved `[[ref]]` a
  link to its entity, the note's links as rows that open their other end, and
  *Delete* behind a confirm. No editing and no Markdown: the page lives one
  release.

  Two notes do not stay here. While notes hide, the address sends the reader
  to Settings › Notes and capture, as #1085 ruled. A note titled by the standup
  protocol rule ("Standup 2026-09-03") is that day's protocol and opens Time ›
  Standup at its date, where protocols are drawn and edited. Both replace the
  address, so *Back* does not return to a page that only redirects.

  A deleted note is a `not_found` (`get_note`), so it reads the shell's
  *Nothing lives at …*. After *Delete* the reader returns where they came
  from, else to Settings › Notes, with a toast and no undo: the `deleted`
  activity line is final.
-->
<script module lang="ts">
  import { captureShortcut, deleteNote, getNote, type NoteDetail } from "../../ipc/entity";
  import { replaceAddress } from "../opener";

  /** The page's reads, its one write, and where it sends the reader. */
  export interface NotePorts {
    getNote: (noteId: string) => Promise<NoteDetail>;
    deleteNote: (noteId: string) => Promise<boolean>;
    /** *Show notes and quick capture* (D10). */
    notesShown: () => Promise<boolean>;
    /** Put `hash` in place of this address. */
    replace: (hash: string) => void;
    /** Whether there is an address to go back to. */
    canGoBack: () => boolean;
    back: () => void;
  }

  /** Where notes are drawn while they hide, and where a delete from the first address lands (D10). */
  const NOTES_SETTINGS = "#/settings/notes";

  /** Called through, not bound at load: a test's mock of the bridge may not carry every command. */
  const BRIDGE: NotePorts = {
    getNote: (noteId) => getNote(noteId),
    deleteNote: (noteId) => deleteNote(noteId),
    notesShown: async () => (await captureShortcut()).shown,
    replace: replaceAddress,
    canGoBack: () => history.length > 1,
    back: () => history.back(),
  };
</script>

<script lang="ts">
  import { ipcErrorMessage, isIpcError } from "../../ipc";
  import { readingFor } from "../../detail/relations";
  import { push } from "../../shell/toasts.svelte";
  import { protocolDayOf } from "../../standup/protocol";
  import { entityAddress } from "../address";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import type { EscTarget } from "../escape";
  import TfCrumbs from "../TfCrumbs.svelte";
  import { tokenise } from "./note-body";

  let {
    id,
    ready,
    back = null,
    ports = BRIDGE,
    layers = sharedLayers,
  }: {
    id: string;
    ready: boolean;
    /** Where `Esc` goes, which the breadcrumb's hint names; `null` where it does nothing (#2228). */
    back?: EscTarget | null;
    ports?: NotePorts;
    layers?: Layers;
  } = $props();

  /** What the page holds: nothing yet, the note, no such note, or a failed read. */
  let read = $state<{ note: NoteDetail } | { missing: true } | { failed: string } | null>(null);
  let confirming = $state(false);
  let deleting = $state(false);
  let deleteFailed = $state<string | null>(null);

  $effect(() => {
    if (!ready) return;
    const noteId = id;
    let live = true;
    read = null;
    confirming = false;
    deleteFailed = null;
    void (async () => {
      // A setting that cannot be read does not hide a note the reader asked for.
      const shown = await ports.notesShown().catch(() => true);
      if (!live) return;
      if (!shown) return ports.replace(NOTES_SETTINGS);
      let note: NoteDetail;
      try {
        note = await ports.getNote(noteId);
      } catch (cause) {
        if (live) read = isIpcError(cause) && cause.code === "not_found" ? { missing: true } : { failed: ipcErrorMessage(cause) };
        return;
      }
      if (!live) return;
      // A protocol opens where `itemHash` sends it: Time › Standup at its date.
      if (protocolDayOf(note.note.title) !== null) return ports.replace(entityAddress("note", note.note.id, note.note.title));
      read = { note };
    })();
    return () => {
      live = false;
    };
  });

  async function remove(note: NoteDetail) {
    deleting = true;
    try {
      // `false` is a note already gone, which is where the reader wanted it.
      await ports.deleteNote(note.note.id);
    } catch (cause) {
      deleteFailed = ipcErrorMessage(cause);
      confirming = false;
      return;
    } finally {
      deleting = false;
    }
    confirming = false;
    push({ text: `Deleted ${note.note.title}` });
    if (ports.canGoBack()) ports.back();
    else ports.replace(NOTES_SETTINGS);
  }

  /** A reading as a row starts: "captured in" is "Captured in". */
  const capital = (reading: string) => reading.charAt(0).toUpperCase() + reading.slice(1);
</script>

<div class="tf-full">
  {#if read === null}
    <!-- Not read yet, or on its way elsewhere. -->
  {:else if "missing" in read}
    <div class="tf-empty">
      <p>Nothing lives at <span class="tf-mono">{location.hash}</span> in the new shell.</p>
      <a class="tf-btn" href="#/tickets">Go to Tickets</a>
    </div>
  {:else if "failed" in read}
    <TfCrumbs parts={[{ name: "Note" }]} {back} />
    <div class="tf-empty"><p class="tf-cfail">Could not read {id}: {read.failed}</p></div>
  {:else}
    {@const detail = read.note}
    <TfCrumbs parts={[{ name: "Note" }]} {back} />
    <header class="tf-fhead">
      <div>
        <h1>{detail.note.title}</h1>
        <div class="tf-sub">A note, read-only in this release.</div>
      </div>
      <div class="tf-acts">
        <button type="button" class="tf-btn tf-danger" disabled={deleting} onclick={() => (confirming = true)}>Delete</button>
      </div>
    </header>
    <div class="tf-item-body">
      {#if deleteFailed !== null}<p class="tf-cfail" role="alert">Could not delete the note: {deleteFailed}</p>{/if}
      <div class="tf-panel tf-item-textp">
        <p class="tf-note-body">{#each tokenise(detail.note.body_md, detail.refs) as token, n (n)}{#if token.kind === "text"}{token.text}{:else if token.ref.target === null}{token.text}{:else if token.ref.target.deleted_at !== null}<span
                class="tf-note-gone"
                title="Withdrawn upstream">{token.ref.target.title}</span
              >{:else}<a
                href={entityAddress(token.ref.target.kind, token.ref.target.entity_id, token.ref.target.title)}
                >{token.ref.target.title}</a
              >{/if}{/each}</p>
      </div>
      {#if detail.links.length > 0}
        <h2 class="tf-cgroup-h">Links</h2>
        <ul class="tf-item-rows" aria-label="Links">
          {#each detail.links as entry (entry.link.id)}
            {@const other = entry.other}
            <li>
              <span class="tf-item-meta">{capital(readingFor(entry, detail.note.id))}</span>
              <!-- A context has no page of its own (#1427), and a withdrawn end opens nothing. -->
              {#if other.kind === "ctx" || other.deleted_at !== null}
                <span class:tf-note-gone={other.deleted_at !== null}>{other.title}</span>
              {:else}
                <a href={entityAddress(other.kind, other.entity_id, other.title)}>{other.title}</a>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</div>

{#if confirming && read !== null && "note" in read}
  {@const detail = read.note}
  <TfDialog title="Delete {detail.note.title}?" {layers} onclose={() => (confirming = false)}>
    {#snippet body()}
      <p class="tf-dlg-note">The note's text goes for good. Links to it stay, marked deleted.</p>
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" onclick={() => (confirming = false)}>Cancel</button>
      <button type="button" class="tf-btn tf-primary tf-danger" disabled={deleting} onclick={() => void remove(detail)}>Delete</button>
    {/snippet}
  </TfDialog>
{/if}
