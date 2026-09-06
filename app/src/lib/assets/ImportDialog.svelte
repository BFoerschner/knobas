<!--
  *Import an estate file* — the dialog behind the Import button in the Assets
  room bar (issue #439, spec #427 stories 21–25).

  ## Two steps, and the first one writes nothing

  Choose a file, read the preview, then press Import. The preview is a command
  of its own that writes nothing at all, so a reader can open a file they are
  not sure about and close this dialog again with the estate untouched — which
  is the whole reason story 21 asks for a preview rather than a confirmation.

  ## Three groups, in the order a reader asks about them

  *New* first, because it is why somebody presses Import; *Would change* next,
  because it is the part that touches what is already there; *Already in the
  tree* last and collapsed to a count, because on a second import of the same
  file it is the whole file and a list of thirty-two rows saying "nothing
  happens" is a list nobody reads.

  Each changed property says which value wins and why: `set` draws the file's
  value, `kept` draws the stored one and says it was edited here. That is
  story 24's promise made visible *before* the write rather than discovered
  after it.

  ## The file is read in the webview

  `<input type="file">` and the browser's own `File`, not a Tauri file dialog:
  the dialog plugin would be a new dependency and a new capability grant for a
  gesture the web platform already has, and the `?fake-ipc` harness cannot open
  a native dialog at all. What crosses the bridge is the file's **text**; the
  parse, and every refusal, is `assets::preview_import`'s, so a file that is
  not an estate file is refused once and in one voice.

  There is no port around the read, unlike everything else this view reaches
  for: `File.text()` is the platform's, jsdom implements it, and a seam here
  would be a seam around the one line no backend is behind.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import type {
    applyEstateImport,
    ImportOutcome,
    ImportPreview,
    previewEstateImport,
    PropertyValue,
  } from "../ipc/assets";
  import Modal from "../shell/Modal.svelte";

  let {
    preview: previewImport,
    apply: applyImport,
    onclose,
    onimported,
  }: {
    preview: typeof previewEstateImport;
    apply: typeof applyEstateImport;
    onclose: () => void;
    /** The import is done. The Tree re-reads; this dialog is finished. */
    onimported: (outcome: ImportOutcome) => void;
  } = $props();

  /** The text of the file the reader chose, once it has been read. */
  let file = $state<string | null>(null);
  /** What the file is called, for the heading. */
  let chosen = $state<string | null>(null);
  let preview = $state<ImportPreview | null>(null);
  /** What the last attempt refused with, in the dialog rather than as a toast. */
  let failure = $state<string | null>(null);
  let busy = $state(false);

  /** Unique per instance, so two stacked dialogs cannot share a field id. */
  const fieldId = `estate-file-${Math.random().toString(36).slice(2, 9)}`;

  /** How a property value reads on one line of the preview. */
  function shown(value: PropertyValue | null): string {
    if (value === null) return "—";
    return value.kind === "number" ? String(value.value) : value.value;
  }

  async function chose(event: Event & { currentTarget: HTMLInputElement }) {
    const picked = event.currentTarget.files?.[0];
    if (!picked) return;
    busy = true;
    failure = null;
    preview = null;
    file = null;
    chosen = picked.name;
    try {
      const text = await picked.text();
      // Held for the apply, which sends the file again rather than a plan:
      // the backend re-decides inside its own transaction, so what is written
      // is what this file says at the moment it is written.
      file = text;
      preview = await previewImport(text);
    } catch (rejection) {
      // In the dialog and in the backend's own words: a file that is not an
      // estate file is refused by name, and the answer to a dialog belongs in
      // the dialog.
      failure = ipcErrorMessage(rejection);
    } finally {
      busy = false;
    }
  }

  async function submit() {
    if (busy || file === null || preview === null) return;
    busy = true;
    failure = null;
    try {
      onimported(await applyImport(file));
    } catch (rejection) {
      failure = ipcErrorMessage(rejection);
    } finally {
      busy = false;
    }
  }

  /** Whether this import would do anything at all. */
  const changes = $derived(
    preview === null
      ? 0
      : preview.new.length + preview.changes.length + preview.monitor_links.length,
  );
</script>

<Modal
  title="Import an estate file"
  subtitle={chosen ?? "Nothing is written until you press Import"}
  wide
  {onclose}
>
  {#snippet body()}
    <div class="fields">
      <div class="fld">
        <label class="l" for={fieldId}>Estate file</label>
        <input
          class="inp"
          id={fieldId}
          type="file"
          accept="application/json,.json"
          onchange={chose}
        />
      </div>

      {#if failure}
        <p class="fail" role="alert">{failure}</p>
      {/if}

      {#if preview}
        <p class="lead">
          <strong>{preview.name}</strong>
          {#if changes === 0}
            — everything in this file is already in the tree, and importing it
            again would change nothing.
          {/if}
        </p>

        <section class="grp">
          <h3 class="l">New — {preview.new.length}</h3>
          {#if preview.new.length === 0}
            <p class="none">Nothing in this file is new.</p>
          {:else}
            <ul class="lst">
              {#each preview.new as entry (entry.id)}
                <li>
                  <span class="nm">{entry.name}</span>
                  <span class="faint">{entry.type_label ?? "Route"}</span>
                  <span class="id">{entry.id}</span>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        <section class="grp">
          <h3 class="l">Would change — {preview.changes.length}</h3>
          {#if preview.changes.length === 0}
            <p class="none">Nothing already in the tree would change.</p>
          {:else}
            <ul class="lst">
              {#each preview.changes as change (change.id)}
                <li class="chg">
                  <span class="nm">{change.name}</span>
                  <ul class="props">
                    {#each change.properties as property (property.key)}
                      <li>
                        <span class="k">{property.label}</span>
                        <!--
                          `.wins` is the value that will be in force and
                          `.refused` the one that will not, whichever side of
                          the arrow each happens to be. Naming them after the
                          *fates* rather than after `from` and `to` is what
                          keeps the kept branch readable: there the stored
                          value wins and the file's is struck through, which is
                          the whole of story 24 — a line drawing only the
                          winner would not say a file had tried.
                        -->
                        {#if property.plan === "set"}
                          <span class="faint">{shown(property.from)}</span>
                          <span class="faint">→</span>
                          <span class="wins">{shown(property.to)}</span>
                        {:else}
                          <span class="wins">{shown(property.from)}</span>
                          <span class="faint">stays — you edited it here</span>
                          <span class="refused">{shown(property.to)}</span>
                        {/if}
                      </li>
                    {/each}
                    {#each change.monitors as name (name)}
                      <li>
                        <span class="k">monitors</span>
                        <span class="wins">+ {name}</span>
                      </li>
                    {/each}
                  </ul>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        {#if preview.monitor_links.length > 0}
          <section class="grp">
            <h3 class="l">Monitors to attach — {preview.monitor_links.length}</h3>
            <ul class="lst">
              {#each preview.monitor_links as link (link.asset_id + link.monitor_id)}
                <li>
                  <span class="nm">{link.asset_name}</span>
                  <span class="faint">monitored by</span>
                  <span class="wins">{link.monitor_name}</span>
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        <section class="grp">
          <!--
            A count and not a list. On a second import this group is the whole
            file, and thirty-two rows saying "nothing happens" is the part of
            the preview that would stop anybody reading the two above it.
          -->
          <h3 class="l">Already in the tree — {preview.known.length}</h3>
        </section>
      {/if}
    </div>
  {/snippet}

  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button
      class="btn pri"
      disabled={busy || preview === null}
      onclick={() => void submit()}
    >
      {busy ? "Working…" : "Import"}
    </button>
  {/snippet}
</Modal>

<style>
  .fields {
    display: grid;
    gap: 14px;
    max-height: 58vh;
    overflow-y: auto;
  }

  .fld .l,
  .grp .l {
    display: block;
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--faint);
    margin: 0 0 6px;
  }

  .lead {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
    line-height: 1.5;
  }

  .none {
    margin: 0;
    font-size: 12px;
    color: var(--faint);
  }

  .lst {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
    font-size: 12px;
  }

  .lst > li {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }

  .lst .nm {
    color: var(--text);
  }

  .lst .id {
    margin-left: auto;
    font: 400 11px var(--mono);
    color: var(--faint);
  }

  .chg {
    display: grid;
    gap: 3px;
  }

  .props {
    list-style: none;
    margin: 0;
    padding: 0 0 0 12px;
    display: grid;
    gap: 2px;
  }

  .props > li {
    display: flex;
    gap: 8px;
    align-items: baseline;
    font-size: 11px;
  }

  .props .k {
    min-width: 10ch;
    color: var(--muted);
  }

  .props .wins,
  .lst .wins {
    color: var(--text);
  }

  .props .refused {
    color: var(--faint);
    text-decoration: line-through;
  }
</style>
