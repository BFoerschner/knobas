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

  ## The chooser (#508, #509)

  *Import from* is where an estate file comes from, and it has **two entries**:
  the file a person picked off the disk, and the hcloud importer.
  `IMPORT_PRODUCERS` in `../ipc/assets` is the list, the Docker importer (spec
  #491, story 67) adds the third, and what the chosen entry decides on the
  backend is the **origin key** it declares — the Import's second matching rule
  (`CONTEXT.md`, *Origin key*), used when a file entry's id is not one the tree
  holds. That is why the id is sent with the preview *and* with the apply: it is
  half of what decides the plan.

  **What the entries differ by is where their text comes from**, and that is the
  one thing this dialog branches on: an `<input type="file">` for the estate
  file, a produce command for an importer (`IMPORTER_IDS`). Everything after the
  text — the preview, the three groups, the Import button — is the same code for
  both, because a producer returns an estate file in the checked-in shape and
  there is nothing downstream that could tell which made it.

  Changing the chooser clears whatever the previous entry had got to. A file
  read under *Estate file* and previewed under `estate_file` is not a file that
  may be applied under `hcloud`: the producer is half of what decides the plan,
  so a stale preview would be a preview of a plan nobody is about to run.

  ## The estate file is read in the webview

  `<input type="file">` and the browser's own `File`, not a Tauri file dialog:
  the dialog plugin would be a new dependency and a new capability grant for a
  gesture the web platform already has, and the `?fake-ipc` harness cannot open
  a native dialog at all. What crosses the bridge is the file's **text**; the
  parse, and every refusal, is `assets::preview_import`'s, so a file that is
  not an estate file is refused once and in one voice.

  There is no port around the read, unlike everything else this view reaches
  for: `File.text()` is the platform's, jsdom implements it, and a seam here
  would be a seam around the one line no backend is behind.

  ## An importer's three questions (#509)

  `produceEstateFile` answers one of three ways and each is a question or an
  answer, never both:

  * **`token_needed`** — nothing is stored under this importer's keychain
    account. The token field appears, and once the run succeeds the backend
    keeps it, so the field is gone next time: *asked once*. It also appears
    after an `unauthorized` refusal, which is the other moment a reader has a
    token to give and the only way back from a credential the far end stopped
    accepting.
  * **`landing_needed`** — the live system holds servers this estate does not.
    The picker walks the tree the way `MoveDialog` does, one read per level, and
    *where you have walked to is where they land*. Asked **once per run**, not
    once per server, because where a provider's servers go is one decision.
  * **`ready`** — the file, which is previewed straight away (a reader who ran
    an importer has already said what they want to see) and offered as a
    download, so what was produced can be kept, read, or edited by hand and
    brought back through *Estate file*.

  The download is an `<a download>` over a blob the webview already holds. No
  backend write and no filesystem capability: the text crossed the bridge to be
  previewed, and handing the same string to the browser costs nothing more.
-->
<script lang="ts">
  import { ipcErrorMessage, isIpcError } from "../ipc";
  import { IMPORT_PRODUCERS, IMPORTER_IDS } from "../ipc/assets";
  import type {
    applyEstateImport,
    assetTree,
    AssetRow,
    ImportOutcome,
    ImportPreview,
    previewEstateImport,
    produceEstateFile,
    PropertyValue,
  } from "../ipc/assets";
  import { latestRead } from "../shell/latest-read";
  import Modal from "../shell/Modal.svelte";

  let {
    preview: previewImport,
    apply: applyImport,
    produce: produceFile,
    tree,
    onclose,
    onimported,
  }: {
    preview: typeof previewEstateImport;
    apply: typeof applyEstateImport;
    /** Running an importer (#509). */
    produce: typeof produceEstateFile;
    /** The estate, one level per read — the *land under* picker's walk. */
    tree: typeof assetTree;
    onclose: () => void;
    /** The import is done. The Tree re-reads; this dialog is finished. */
    onimported: (outcome: ImportOutcome) => void;
  } = $props();

  /** The text of the file the reader chose or produced, once it is there. */
  let file = $state<string | null>(null);
  /** What the file is called, for the heading. */
  let chosen = $state<string | null>(null);
  let preview = $state<ImportPreview | null>(null);
  /** What the last attempt refused with, in the dialog rather than as a toast. */
  let failure = $state<string | null>(null);
  let busy = $state(false);

  /** Which of {@link IMPORT_PRODUCERS} this import is from. */
  let producer = $state(IMPORT_PRODUCERS[0].id);

  /** Whether the chosen producer reads a live system rather than a disk. */
  const isImporter = $derived(IMPORTER_IDS.includes(producer));

  /**
   * The token field, shown only when the backend says one is owed — on a first
   * run, and again after a refusal. Not a field that is always there: a
   * credential the keychain already holds is one nobody should be asked to
   * type, which is the whole of *asks for the token once*.
   */
  let tokenWanted = $state(false);
  let token = $state("");
  /** The servers `landing_needed` named, or `null` while none is owed. */
  let landing = $state<string[] | null>(null);
  /** What the last successful run said it would create. */
  let produced = $state<string[]>([]);

  /** Where the *land under* picker is standing: the crumb, and its foot. */
  let crumb = $state<AssetRow[]>([]);
  let rows = $state<AssetRow[]>([]);
  const columnRead = latestRead<AssetRow[]>();
  const landUnder = $derived(crumb.at(-1)?.id ?? null);
  const landingName = $derived(crumb.at(-1)?.name ?? "the top of the estate");

  /** Unique per instance, so two stacked dialogs cannot share a field id. */
  const fieldId = `estate-file-${Math.random().toString(36).slice(2, 9)}`;
  const producerId = `import-from-${Math.random().toString(36).slice(2, 9)}`;
  const tokenId = `importer-token-${Math.random().toString(36).slice(2, 9)}`;

  /**
   * The chooser moved: everything the previous producer got to is gone.
   *
   * A preview is a plan drawn under one producer's origin key, so carrying one
   * across would offer *Import* on a plan the backend is not about to run.
   */
  function chose_producer() {
    file = null;
    chosen = null;
    preview = null;
    failure = null;
    tokenWanted = false;
    token = "";
    landing = null;
    produced = [];
    crumb = [];
    rows = [];
    if (downloadHref !== null) URL.revokeObjectURL(downloadHref);
    downloadHref = null;
  }

  /** Stand at the foot of `path` — the top of the estate when it is empty. */
  function stand(path: AssetRow[]) {
    crumb = path;
  }

  /**
   * The level the picker is standing on, re-read whenever it moves.
   *
   * Driven off the crumb rather than off the click that changed it, which is
   * `MoveDialog`'s rule: one place decides what is on screen, so a click and a
   * step back cannot leave the crumb and the rows disagreeing.
   */
  $effect(() => {
    if (landing === null) return;
    const parent = landUnder;
    void columnRead(() => tree(parent), {
      ok: (answer) => {
        rows = answer;
      },
      fail: (cause) => {
        rows = [];
        failure = ipcErrorMessage(cause);
      },
    });
  });

  /**
   * One run of the chosen importer, and the preview of whatever it produced.
   *
   * The token is sent **only when the reader has just typed one**; otherwise
   * the backend reads its keychain. It is cleared from this component the
   * moment the run succeeds — the keychain is where a credential lives, and a
   * copy held in a field until the dialog closes is a copy for no reason.
   */
  async function run() {
    if (busy) return;
    busy = true;
    failure = null;
    try {
      const answer = await produceFile(producer, token.trim() || null, landUnder);
      if (answer.state === "token_needed") {
        tokenWanted = true;
        return;
      }
      if (answer.state === "landing_needed") {
        landing = answer.servers;
        return;
      }
      tokenWanted = false;
      token = "";
      landing = null;
      produced = answer.new_servers;
      file = answer.file;
      offer(answer.file);
      chosen = producerLabel();
      preview = await previewImport(answer.file, producer);
    } catch (rejection) {
      failure = ipcErrorMessage(rejection);
      // The one refusal a reader can act on: a token the far end stopped
      // accepting is re-entered here rather than in a settings screen no
      // importer has (ADR-0015 — an importer is not a source).
      if (isIpcError(rejection) && rejection.code === "unauthorized") tokenWanted = true;
    } finally {
      busy = false;
    }
  }

  function producerLabel(): string {
    return IMPORT_PRODUCERS.find((entry) => entry.id === producer)?.label ?? producer;
  }

  /** What the produced file is offered for download as. */
  const downloadName = $derived(`${producer}-estate.json`);

  /**
   * The blob the download link points at, minted **once per produced file**.
   *
   * Not a `$derived`: `createObjectURL` mints a URL that lives until the
   * document unloads or somebody revokes it, so a derived one would mint a
   * fresh leak on every redraw of a dialog nothing about the file had changed
   * in. Minted here, and the one before it revoked, so at most one is
   * outstanding.
   */
  let downloadHref = $state<string | null>(null);

  function offer(text: string) {
    if (downloadHref !== null) URL.revokeObjectURL(downloadHref);
    downloadHref = URL.createObjectURL(new Blob([text], { type: "application/json" }));
  }

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
      preview = await previewImport(text, producer);
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
      onimported(await applyImport(file, producer));
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
        <label class="l" for={producerId}>Import from</label>
        <select
          class="inp"
          id={producerId}
          bind:value={producer}
          onchange={chose_producer}
        >
          {#each IMPORT_PRODUCERS as option (option.id)}
            <option value={option.id}>{option.label}</option>
          {/each}
        </select>
      </div>

      {#if !isImporter}
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
      {:else}
        {#if tokenWanted}
          <div class="fld">
            <label class="l" for={tokenId}>API token</label>
            <input
              class="inp"
              id={tokenId}
              type="password"
              autocomplete="off"
              placeholder="Read-only is enough"
              bind:value={token}
            />
            <p class="hint">
              Kept in this machine's keychain, under this importer's own name —
              never in the database and never in a share export. You are asked
              once.
            </p>
          </div>
        {/if}

        <!--
          *Land under*, asked only when there is something to ask about
          (#509). The picker is `MoveDialog`'s walk: where you have walked to
          is where the servers land, so the top of the estate is an ordinary
          destination rather than a special button.
        -->
        {#if landing !== null}
          <div class="fld">
            <span class="l">Land under</span>
            <p class="hint">
              {landing.length === 1
                ? "1 server is not in the tree yet"
                : `${landing.length} servers are not in the tree yet`}: {landing.join(", ")}.
            </p>
            <nav class="crumb">
              <button class="crumbed" onclick={() => stand([])}>Estate</button>
              {#each crumb as step, at (step.id)}
                <span class="faint">/</span>
                <button class="crumbed" onclick={() => stand(crumb.slice(0, at + 1))}>
                  {step.name}
                </button>
              {/each}
            </nav>
            <ul class="lst pick">
              {#each rows as row (row.id)}
                <li>
                  <button class="into" onclick={() => stand([...crumb, row])}>
                    {row.name}
                  </button>
                  <span class="faint">{row.type_label}</span>
                </li>
              {:else}
                <li class="none">Nothing inside {landingName}.</li>
              {/each}
            </ul>
            <p class="lead">They will land in <strong>{landingName}</strong>.</p>
          </div>
        {/if}

        <div class="fld">
          <button class="btn" disabled={busy} onclick={() => void run()}>
            {#if busy}
              Reading…
            {:else if landing !== null}
              Put them in {landingName}
            {:else}
              Read {producerLabel()}
            {/if}
          </button>
        </div>
      {/if}

      {#if failure}
        <p class="fail" role="alert">{failure}</p>
      {/if}

      <!--
        The produced file, offered for keeping. An `<a download>` over a blob
        the webview already holds: no backend write and no filesystem
        capability, because the text crossed the bridge to be previewed anyway.
      -->
      {#if isImporter && file !== null && downloadHref !== null}
        <p class="lead">
          <a class="dl" href={downloadHref} download={downloadName}>
            Download {downloadName}
          </a>
          {#if produced.length > 0}
            — {produced.length === 1
              ? "1 server is new"
              : `${produced.length} servers are new`}
          {/if}
        </p>
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

        <!--
          The names that found no monitor (#445).

          Its own group and not a line in *Would change*, because it is not a
          change: the name is already on the asset, or is about to be, and what
          this says is that nothing in the mirror answers to it. That is also
          why it survives a second import of an unchanged file, when every
          group above it is empty — which is exactly when a reader wonders why
          their monitors are not attached.

          Not a fault, and it does not read as one: the estate file names the
          monitors it expects, and a name resolves the moment Kuma publishes
          one called that.
        -->
        {#if preview.unresolved.length > 0}
          <section class="grp">
            <h3 class="l">Named but not in Kuma — {preview.unresolved.length}</h3>
            <ul class="lst">
              {#each preview.unresolved as waiting (waiting.asset_id + waiting.monitor_name)}
                <li>
                  <span class="nm">{waiting.asset_name}</span>
                  <span class="faint">{waiting.monitor_name}</span>
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

  .hint {
    margin: 4px 0 0;
    font-size: 11px;
    color: var(--faint);
    line-height: 1.5;
  }

  .crumb {
    display: flex;
    gap: 6px;
    align-items: baseline;
    flex-wrap: wrap;
    margin: 6px 0;
    font-size: 12px;
  }

  .crumbed,
  .into {
    background: none;
    border: 0;
    padding: 0;
    font: inherit;
    color: var(--text);
    cursor: pointer;
    text-align: left;
  }

  .crumbed:hover,
  .into:hover {
    text-decoration: underline;
  }

  .pick {
    max-height: 22vh;
    overflow-y: auto;
  }

  .dl {
    color: var(--text);
  }
</style>
