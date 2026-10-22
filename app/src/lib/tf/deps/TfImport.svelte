<!--
  *Import…* from Dependencies' *Add ▾* (#1066, R-11.20, R-11.23): the rooms
  UI's `ImportDialog` (#439, #508–#510) in the new shell's look, over the same
  three commands.

  Choose where the file comes from, read the preview, then press *Import*. The
  preview writes nothing. An estate file is read in the webview; an importer
  (hcloud, Docker host) is run, and may first ask for a token (once, or again
  after `unauthorized`) and where the servers it found should land. Changing
  the chooser clears everything, since the producer is half of what decides
  the plan. The apply sends the file again under the same producer.
-->
<script lang="ts">
  import { ipcErrorMessage, isIpcError } from "../../ipc";
  import { IMPORT_PRODUCERS, type AssetRow, type ImportOutcome, type ImportPreview } from "../../ipc/assets";
  import { latestRead } from "../../shell/latest-read";
  import { readSecretsInMemory, tokenKeptHint } from "../../shell/secret-store";
  import TfDialog from "../settings/TfDialog.svelte";
  import type { ImportPorts } from "./ports";
  import TfImportPreview from "./TfImportPreview.svelte";

  let {
    ports,
    onclose,
    onimported,
  }: {
    ports: ImportPorts;
    onclose: () => void;
    /** Done: what the file calls the estate, and what the import wrote. */
    onimported: (name: string, outcome: ImportOutcome) => void;
  } = $props();

  const uid = $props.id();
  let producer = $state(IMPORT_PRODUCERS[0].id);
  const entry = $derived(IMPORT_PRODUCERS.find((p) => p.id === producer) ?? IMPORT_PRODUCERS[0]);
  let file = $state<string | null>(null);
  let preview = $state.raw<ImportPreview | null>(null);
  let failure = $state<string | null>(null);
  let busy = $state(false);
  let tokenWanted = $state(false);
  // Where a token is kept (#1615): the keychain unless this process keeps
  // credentials in memory. The keychain wording stands until `app_status` says otherwise.
  let secretsInMemory = $state(false);
  $effect(() => {
    let dead = false;
    void readSecretsInMemory().then((read) => {
      if (!dead) secretsInMemory = read;
    });
    return () => {
      dead = true;
    };
  });
  let token = $state("");
  /** The servers `landing_needed` named, or `null` while none is owed. */
  let landing = $state.raw<string[] | null>(null);
  let produced = $state.raw<string[]>([]);
  let skipped = $state.raw<string[]>([]);
  let crumb = $state.raw<AssetRow[]>([]);
  let rows = $state.raw<AssetRow[]>([]);
  let downloadHref = $state<string | null>(null);
  const columnRead = latestRead<AssetRow[]>();
  const landUnder = $derived(crumb.at(-1)?.id ?? null);
  const landingName = $derived(crumb.at(-1)?.name ?? "the top of the estate");
  const downloadName = $derived(`${producer}-estate.json`);

  function choseProducer() {
    picks += 1;
    file = null;
    preview = null;
    failure = null;
    tokenWanted = false;
    token = "";
    landing = null;
    produced = [];
    skipped = [];
    crumb = [];
    rows = [];
    offer(null);
  }

  /** The produced file as a download, one blob URL at a time. */
  function offer(text: string | null) {
    if (downloadHref !== null) URL.revokeObjectURL(downloadHref);
    downloadHref = text === null ? null : URL.createObjectURL(new Blob([text], { type: "application/json" }));
  }

  // The *land under* picker reads the level it stands on whenever it moves.
  $effect(() => {
    if (landing === null) return;
    const parent = landUnder;
    void columnRead(() => ports.assetTree(parent), {
      ok: (answer) => (rows = answer),
      fail: (cause) => {
        rows = [];
        failure = ipcErrorMessage(cause);
      },
    });
  });

  // A blob URL lives until revoked: the last one goes when the dialog does.
  $effect(() => () => {
    if (downloadHref !== null) URL.revokeObjectURL(downloadHref);
  });

  /** Which pick is the newest, so a slower preview of an earlier file is dropped. */
  let picks = 0;

  async function chose(event: Event & { currentTarget: HTMLInputElement }) {
    const picked = event.currentTarget.files?.[0];
    if (!picked) return;
    const mine = ++picks;
    busy = true;
    failure = null;
    preview = null;
    file = null;
    try {
      const text = await picked.text();
      const answer = await ports.previewEstateImport(text, producer);
      if (mine !== picks) return;
      file = text;
      preview = answer;
    } catch (error) {
      if (mine === picks) failure = ipcErrorMessage(error);
    } finally {
      busy = false;
    }
  }

  /** One run of the chosen importer, then the preview of what it produced. */
  async function run() {
    if (busy) return;
    // A changed chooser drops this run: its file would be applied under the other producer.
    const mine = picks;
    const from = producer;
    busy = true;
    failure = null;
    try {
      // `{ parent: null }` is the top of the estate; a bare `null` is *not asked yet*.
      const answer = await ports.produceEstateFile(from, token.trim() || null, landing === null ? null : { parent: landUnder });
      if (mine !== picks) return;
      if (answer.state === "token_needed") {
        tokenWanted = true;
        return;
      }
      tokenWanted = false;
      token = "";
      if (answer.state === "landing_needed") {
        landing = answer.servers;
        return;
      }
      landing = null;
      produced = answer.new_servers;
      skipped = answer.skipped;
      file = answer.file;
      offer(answer.file);
      const answered = await ports.previewEstateImport(answer.file, from);
      if (mine !== picks) return;
      preview = answered;
    } catch (error) {
      if (mine !== picks) return;
      failure = ipcErrorMessage(error);
      if (isIpcError(error) && error.code === "unauthorized") tokenWanted = true;
    } finally {
      busy = false;
    }
  }

  async function submit() {
    if (busy || file === null || preview === null) return;
    busy = true;
    failure = null;
    try {
      const outcome = await ports.applyEstateImport(file, producer);
      offer(null);
      onimported(preview.name, outcome);
    } catch (error) {
      failure = ipcErrorMessage(error);
    } finally {
      busy = false;
    }
  }
</script>

<TfDialog title="Import an estate file" className="tf-im-dialog" {onclose}>
  {#snippet body()}
    <p class="tf-dim tf-note">Nothing is written until you press Import.</p>
    <div class="tf-formgrid">
      <label for="{uid}-from">Import from</label>
      <select id="{uid}-from" bind:value={producer} onchange={choseProducer}>
        {#each IMPORT_PRODUCERS as option (option.id)}<option value={option.id}>{option.label}</option>{/each}
      </select>
      {#if !entry.importer}
        <label for="{uid}-file">Estate file</label>
        <input id="{uid}-file" type="file" accept="application/json,.json" onchange={chose} />
      {:else if tokenWanted}
        <label for="{uid}-token">API token</label>
        <input id="{uid}-token" type="password" autocomplete="off" placeholder="Read-only is enough" bind:value={token} />
      {/if}
    </div>
    {#if entry.importer && tokenWanted}
      <p class="tf-dim tf-note">
        {tokenKeptHint(secretsInMemory)}
      </p>
    {/if}
    {#if entry.importer && landing !== null}
      <p class="tf-note">
        {landing.length === 1 ? "1 server is not in the tree yet" : `${landing.length} servers are not in the tree yet`}: {landing.join(", ")}.
      </p>
      <nav class="tf-im-crumb" aria-label="Land under">
        <button type="button" class="tf-linkish" onclick={() => (crumb = [])}>Estate</button>
        {#each crumb as step, at (step.id)}
          <span class="tf-dim">/</span>
          <button type="button" class="tf-linkish" onclick={() => (crumb = crumb.slice(0, at + 1))}>{step.name}</button>
        {/each}
      </nav>
      <ul class="tf-im-pick">
        {#each rows as row (row.id)}
          <li><button type="button" class="tf-linkish" onclick={() => (crumb = [...crumb, row])}>{row.name}</button> <span class="tf-dim">{row.type_label}</span></li>
        {:else}
          <li class="tf-dim">Nothing inside {landingName}.</li>
        {/each}
      </ul>
      <p class="tf-note">They will land in <b>{landingName}</b>.</p>
    {/if}
    {#if entry.importer}
      <div>
        <button type="button" class="tf-btn tf-sm" disabled={busy} onclick={() => void run()}>
          {busy ? "Reading…" : landing !== null ? `Put them in ${landingName}` : `Read ${entry.label}`}
        </button>
      </div>
    {/if}
    {#if failure !== null}<p class="tf-ifail" role="alert">{failure}</p>{/if}
    {#if entry.importer && file !== null && downloadHref !== null}
      <p class="tf-note">
        <a href={downloadHref} download={downloadName}>Download {downloadName}</a>
        {#if produced.length > 0}— {produced.length === 1 ? "1 is new" : `${produced.length} are new`}{/if}
      </p>
    {/if}
    {#if skipped.length > 0}
      <p class="tf-dim tf-note">
        {skipped.length === 1 ? "1 container engine was" : `${skipped.length} container engines were`} not read, because
        {skipped.length === 1 ? "it carries" : "they carry"} no <code>docker_context</code>: {skipped.join(", ")}.
      </p>
    {/if}
    {#if preview !== null}<TfImportPreview {preview} />{/if}
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="button" class="tf-btn tf-primary" disabled={busy || preview === null} onclick={() => void submit()}>
      {busy ? "Working…" : "Import"}
    </button>
  {/snippet}
</TfDialog>

<style>
  .tf-im-crumb {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    font-size: var(--t-sm);
  }
  .tf-im-pick {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 3px;
    max-height: 22vh;
    overflow-y: auto;
    font-size: var(--t-sm);
  }
</style>
