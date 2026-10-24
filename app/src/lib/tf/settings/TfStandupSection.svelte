<!--
  Settings › Standup (#831, R-13a.5): prototype-6's `STGV.standup` over
  `standup_publish_target`. *Change…* opens the same picker the first
  *Publish* asks with (`PublishTargetPicker`, story 67), and writes the same
  key, so the two cannot disagree about where the next page lands.

  The target is an entity id; the row names the page by its mirror row's
  declared `path` and `title` (ADR-0007), and by its id when the mirror does
  not hold it.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { PublishTarget } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import PublishTargetPicker from "../../standup/PublishTargetPicker.svelte";
  import { publishableSources, type PublishableSource } from "../../standup/protocol";
  import type { Layers } from "../layers.svelte";
  import type { SectionPorts } from "./ports";
  import TfDialog from "./TfDialog.svelte";

  let {
    ports,
    layers,
  }: {
    ports: Pick<
      SectionPorts,
      | "standupPublishTarget"
      | "setStandupPublishTarget"
      | "getEntity"
      | "search"
      | "listSources"
      | "listAdapters"
      | "refusedWrites"
    >;
    layers: Layers;
  } = $props();

  /** `undefined` until the read answers: *unknown* is not *none*. */
  let stored = $state<PublishTarget | null | undefined>(undefined);
  let name = $state<string | null>(null);
  let failure = $state<string | null>(null);
  let picking = $state(false);
  let sources = $state<PublishableSource[]>([]);
  let chosen = $state<PublishTarget | null>(null);
  let saving = $state(false);

  async function nameOf(target: PublishTarget | null) {
    if (target === null) return (name = null);
    try {
      const { row } = await ports.getEntity(target.parent);
      name = row.path ? `${row.path} › ${row.title}` : row.title;
    } catch {
      name = target.parent;
    }
  }

  async function load() {
    try {
      stored = await ports.standupPublishTarget();
      failure = null;
      await nameOf(stored);
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    }
  }

  $effect(() => {
    void load();
  });

  async function change() {
    chosen = stored ?? null;
    picking = true;
    try {
      const [configured, descriptors] = await Promise.all([ports.listSources(), ports.listAdapters()]);
      sources = publishableSources(configured, descriptors);
    } catch (cause) {
      push({ text: `Could not read the sources: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function save() {
    if (chosen === null) return;
    saving = true;
    try {
      stored = await ports.setStandupPublishTarget(chosen);
      picking = false;
      await nameOf(stored);
    } catch (cause) {
      push({ text: `Could not save: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      saving = false;
    }
  }
</script>

<header class="tf-stg-h"><h1>Standup</h1><p>Where the standup protocol is published.</p></header>
{#if failure !== null}
  <p class="tf-stg-p tf-stg-errt">Could not read where standups are published: {failure}</p>
  <div class="tf-stg-acts"><button class="tf-btn" onclick={() => void load()}>Try again</button></div>
{:else if stored !== undefined}
  <div class="tf-stg-row">
    <div class="tf-stg-l">Publish under<small>Each day becomes a child page, named by its date.</small></div>
    <div class="tf-stg-c tf-stg-inl">
      <span>{stored === null ? "Not chosen yet. The first publish asks." : (name ?? stored.parent)}</span>
      <button class="tf-btn tf-sm" onclick={() => void change()}>Change…</button>
    </div>
  </div>
{/if}

{#if picking}
  <TfDialog title="Publish standups under" onclose={() => (picking = false)} {layers}>
    {#snippet body()}
      <p class="tf-dsub">Changing this moves where the next page lands. Pages already published stay where they are.</p>
      <div class="tf-stg-pick">
        <PublishTargetPicker
          {sources}
          value={stored ?? null}
          search={ports.search}
          refusedWrites={ports.refusedWrites}
          onchange={(target) => (chosen = target)}
        />
      </div>
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn" onclick={() => (picking = false)}>Cancel</button>
      <button class="tf-btn tf-primary" disabled={saving || chosen === null} onclick={() => void save()}>Save</button>
    {/snippet}
  </TfDialog>
{/if}
