<!--
  Settings › Sources › *Installed adapters* (#1209, #1210, ADR-0028): one row
  per installed external adapter, under the source list. Each names its
  version and release tag (*no release* for a local manifest or a URL that is
  no release) and where it came from, and says what the last update check
  found: *Update available* with an *Update* button, a refused update with
  the checks' reason, a check that could not run. Nothing updates on its own.
  *Check for updates* asks again; knobas also asks at app start.

  *Update* checks the new release first and switches nothing (#2031). When
  its hosts, roles or limits differ from the installed release's, *Update
  <name> to <release>?* says what changed and waits for its *Update*; when
  they are the same it switches at once. Either way the switch is held to
  the artifact that was checked.

  An adapter refused at load or restore says why on its row (a restore whose
  release changed hash is one), and can still be uninstalled.

  *Uninstall…* asks once per source of the adapter what *Remove…* on its
  page asks, keep what it synced or delete it, and sends one answer each.
  Drawn only once one adapter is installed: the built-in ones are not here.
  An update or uninstall tells Settings, which reads the descriptors the
  source pages are handed again (#1406).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { AdapterStatus, UpdatePreview } from "../../ipc/sources";
  import { push } from "../../shell/toasts.svelte";
  import type { Layers } from "../layers.svelte";
  import { updateChanges } from "./adapter-preview";
  import TfAdapterFacts from "./TfAdapterFacts.svelte";
  import TfDialog from "./TfDialog.svelte";
  import type { SettingsPorts } from "./ports";

  let {
    ports,
    layers,
    refresh = 0,
    onremoved,
    onchanged,
  }: {
    ports: Pick<
      SettingsPorts,
      "listInstalledAdapters" | "checkAdapterUpdates" | "previewUpdateAdapter" | "updateAdapter" | "uninstallAdapter"
    >;
    layers: Layers;
    /** Moves when an install may have added a row (*Add a source* closed). */
    refresh?: number;
    /** An uninstall removed sources: the source list is stale. */
    onremoved: () => void;
    /** An update or uninstall changed what `list_adapters` answers: the descriptors are stale (#1406). */
    onchanged: () => void;
  } = $props();

  let rows = $state<AdapterStatus[]>([]);
  let checking = $state(false);
  let updating = $state<string | null>(null);
  /** The update whose changes *Update <name> to <release>?* shows, and what changed. */
  let confirming = $state<{ row: AdapterStatus; preview: UpdatePreview; changes: string[] } | null>(null);
  /** The adapter whose *Uninstall…* is open, and each source's answer: purge or not. */
  let uninstalling = $state<AdapterStatus | null>(null);
  let purge = $state<Record<string, boolean>>({});

  $effect(() => {
    void refresh;
    let dead = false;
    ports
      .listInstalledAdapters()
      .then((answer) => {
        if (!dead) rows = answer;
      })
      .catch(() => {});
    return () => {
      dead = true;
    };
  });

  /** A sentence that opens with a backend reason capitalises knobas' own word only. */
  function capital(text: string): string {
    return text.charAt(0).toUpperCase() + text.slice(1);
  }

  function release(row: AdapterStatus): string {
    return row.tag === null ? "no release" : row.tag;
  }

  /** What the row says beside its pin, and whether it is the err tone. */
  function standing(row: AdapterStatus): { text: string; err: boolean } | null {
    if (row.refused !== null) return { text: `Refused: ${row.refused}`, err: true };
    const update = row.update;
    if (update === null || update.state === "current") return null;
    if (update.state === "available") {
      return { text: `Update available: ${update.tag ?? update.adapter_version}`, err: false };
    }
    if (update.state === "refused") {
      return { text: `Update${update.tag === null ? "" : ` to ${update.tag}`} refused, still on ${release(row)}: ${update.reason}`, err: true };
    }
    return { text: `Could not check for updates: ${update.reason}`, err: false };
  }

  async function check() {
    checking = true;
    try {
      rows = await ports.checkAdapterUpdates();
      const newer = rows.filter((row) => row.update?.state === "available").length;
      push({ text: newer === 0 ? "Every adapter is up to date" : `${newer} update${newer === 1 ? "" : "s"} available` });
    } catch (cause) {
      push({ text: `Could not check for updates: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      checking = false;
    }
  }

  /** A refused or failed update: the reason, and the row's line read again. */
  async function failed(row: AdapterStatus, cause: unknown) {
    push({ text: `${capital(ipcErrorMessage(cause))}. ${row.name} stays on ${release(row)}.`, tone: "err" });
    rows = await ports.listInstalledAdapters().catch(() => rows);
  }

  /** *Update*: the new release checked, and asked about when it may do more or other than the installed one. */
  async function update(row: AdapterStatus) {
    updating = row.adapter_kind;
    let preview: UpdatePreview;
    try {
      preview = await ports.previewUpdateAdapter(row.adapter_kind);
    } catch (cause) {
      await failed(row, cause);
      updating = null;
      return;
    }
    const changes = updateChanges(preview.installed, preview.release);
    if (changes.length > 0) {
      updating = null;
      confirming = { row, preview, changes };
      return;
    }
    await switchTo(row, preview);
  }

  /** The switch, held to the release that was checked. */
  async function switchTo(row: AdapterStatus, preview: UpdatePreview) {
    updating = row.adapter_kind;
    try {
      const next = await ports.updateAdapter(row.adapter_kind, preview.release.artifact_sha256);
      rows = rows.map((r) => (r.adapter_kind === next.adapter_kind ? next : r));
      onchanged();
      push({ text: `Updated ${next.name} to ${release(next)}` });
    } catch (cause) {
      await failed(row, cause);
    } finally {
      updating = null;
    }
  }

  function confirmUpdate() {
    const asked = confirming;
    if (asked === null) return;
    confirming = null;
    void switchTo(asked.row, asked.preview);
  }

  function askUninstall(row: AdapterStatus) {
    purge = Object.fromEntries(row.sources.map((source) => [source.id, false]));
    uninstalling = row;
  }

  async function uninstall() {
    const row = uninstalling;
    if (row === null) return;
    uninstalling = null;
    try {
      await ports.uninstallAdapter(
        row.adapter_kind,
        row.sources.map((source) => ({ source_id: source.id, purge_items: purge[source.id] === true })),
      );
      rows = rows.filter((r) => r.adapter_kind !== row.adapter_kind);
      push({ text: `Uninstalled ${row.name}` });
    } catch (cause) {
      push({ text: `Could not uninstall ${row.name}: ${ipcErrorMessage(cause)}`, tone: "err" });
      // A failure part-way has removed the sources before it.
      rows = await ports.listInstalledAdapters().catch(() => rows);
    } finally {
      onchanged();
      if (row.sources.length > 0) onremoved();
    }
  }
</script>

{#if rows.length > 0}
  <section class="tf-stg-gap" aria-labelledby="tf-ext-h">
    <div class="tf-ext-h">
      <h2 id="tf-ext-h">Installed adapters</h2>
      <button class="tf-btn" disabled={checking} onclick={() => void check()}>Check for updates</button>
    </div>
    <div class="tf-stg-srcs">
      {#each rows as row (row.adapter_kind)}
        {@const said = standing(row)}
        <div class="tf-ext-row">
          <span class="tf-stg-sn">
            <b>{row.name} {row.adapter_version}</b>
            <span class="tf-dim">{release(row)} · {row.source_url}</span>
            {#if said !== null}
              <!-- Text: a reason quotes what the release and the checks wrote. -->
              <span class="tf-stg-si {said.err ? 'tf-stg-errt' : ''}">{said.text}</span>
            {/if}
          </span>
          <span class="tf-stg-acts tf-stg-flush">
            {#if row.update?.state === "available" && row.refused === null}
              <button class="tf-btn tf-primary" disabled={updating !== null} onclick={() => void update(row)}>
                {updating === row.adapter_kind ? "Updating…" : "Update"}
              </button>
            {/if}
            <button class="tf-btn" aria-label="Uninstall {row.name}…" onclick={() => askUninstall(row)}>Uninstall…</button>
          </span>
        </div>
      {/each}
    </div>
  </section>
{/if}

{#if uninstalling !== null}
  {@const row = uninstalling}
  <TfDialog title="Uninstall {row.name}?" {layers} onclose={() => (uninstalling = null)}>
    {#snippet body()}
      {#if row.sources.length === 0}
        <p class="tf-dlg-note">No source uses it.</p>
      {:else}
        <p class="tf-dlg-note">Each source of {row.name} is removed. Keep what it synced?</p>
        {#each row.sources as source (source.id)}
          <div class="tf-radio" role="radiogroup" aria-label="What {source.display_name} synced">
            <b>{source.display_name}</b>
            <label>
              <input type="radio" name="tf-un-{source.id}" checked={purge[source.id] !== true} onchange={() => (purge[source.id] = false)} />
              <span>Keep what it synced<small>Items stay searchable and linked, and stop updating.</small></span>
            </label>
            <label>
              <input type="radio" name="tf-un-{source.id}" checked={purge[source.id] === true} onchange={() => (purge[source.id] = true)} />
              <span>Delete what it synced<small>Links to those items go too. Your links between other items stay.</small></span>
            </label>
          </div>
        {/each}
      {/if}
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn tf-ghost" onclick={() => (uninstalling = null)}>Cancel</button>
      <button class="tf-btn tf-primary tf-danger" onclick={() => void uninstall()}>Uninstall</button>
    {/snippet}
  </TfDialog>
{/if}

{#if confirming !== null}
  {@const asked = confirming}
  {@const next = asked.preview.release}
  <TfDialog title="Update {asked.row.name} to {next.tag ?? next.adapter_version}?" {layers} onclose={() => (confirming = null)}>
    {#snippet body()}
      <p class="tf-dlg-note">This release changes what {asked.row.name} may do:</p>
      <ul class="tf-ext-changes">
        {#each asked.changes as line (line)}<li>{line}</li>{/each}
      </ul>
      <TfAdapterFacts preview={next} />
      <p class="tf-dlg-note">{asked.row.name} stays on {release(asked.row)} until you press Update.</p>
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn tf-ghost" onclick={() => (confirming = null)}>Cancel</button>
      <button class="tf-btn tf-primary" onclick={confirmUpdate}>Update</button>
    {/snippet}
  </TfDialog>
{/if}
