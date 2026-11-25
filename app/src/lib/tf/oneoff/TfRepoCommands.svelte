<!--
  A repository's own commands (ADR-0060, section 22): what its `.knobas/commands.toml` holds, read from the checkout on
  disk, each with its whole command line in the code face, what it takes and a tick. *Add to my commands* copies the
  ticked ones into Settings › Commands, each scoped to this repository: from then on they are the person's own. A copy
  whose line the file has since changed shows both lines. Nothing here runs.
-->
<script lang="ts">
  import type { OneOffRepoCommands } from "../../ipc/oneoff";
  import { ipcErrorMessage } from "../../ipc";
  import { oneoffs as sharedStore, type OneOffs } from "../../render/oneoff.svelte";
  import { push } from "../../shell/toasts.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { KIND_WORDS } from "../settings/command-page";

  let {
    entityId,
    repo,
    read,
    store = sharedStore,
    layers = sharedLayers,
    onclose,
  }: { entityId: string; repo: string; read: OneOffRepoCommands; store?: OneOffs; layers?: Layers; onclose: () => void } = $props();

  /** The most commands shown; a file holds a handful. */
  const SHOWN = 50;
  let ticked = $state<Record<string, boolean>>({});
  const picked = $derived(read.offered.filter((o) => ticked[o.name] === true && o.refused == null).map((o) => o.name));
  let busy = $state(false);

  async function add() {
    busy = true;
    try {
      await store.copyRepoCommands(entityId, picked);
      push({ text: `Added ${picked.length === 1 ? picked[0] : `${picked.length} commands`} from ${repo} to your commands` });
      onclose();
    } catch (cause) {
      push({ text: `Could not add them: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      busy = false;
    }
  }
</script>

<TfDialog title="Commands from {repo}" subtitle={read.file ?? null} {onclose} {layers} why={picked.length === 0 ? "Tick the ones to add." : null}>
  {#snippet body()}
    <p class="tf-dsub">
      These are the repository's. Nothing from it runs: a command you add is copied into your own list, scoped to {repo}, and a later change of the file changes
      nothing by itself. Read each line first: it runs as you, in your checkout.
    </p>
    {#if read.failed != null}<div class="tf-banner tf-err tf-dbanner" role="alert">The file could not be read: {read.failed}</div>{/if}
    <ul class="tf-oo-offer">
      {#each read.offered.slice(0, SHOWN) as o (o.name)}
        <li>
          <label class="tf-stg-tick">
            <input type="checkbox" disabled={o.refused != null || (o.copied != null && o.theirs == null)} bind:checked={ticked[o.name]} />
            <b>{o.name}</b>
            <span class="tf-dim">{o.kind != null ? KIND_WORDS[o.kind] : "Cannot be read"}{o.copied != null ? (o.theirs != null ? ", changed since you added it" : ", added") : ""}</span>
          </label>
          <code class="tf-mono tf-oo-offerline">{o.command}</code>
          {#if o.theirs != null}<code class="tf-mono tf-oo-offerline tf-dim">Yours: {o.theirs}</code>{/if}
          {#if o.refused != null}<small class="tf-stg-errt">{o.refused}</small>{/if}
        </li>
      {/each}
    </ul>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="button" class="tf-btn tf-primary" disabled={picked.length === 0 || busy} onclick={() => void add()}>Add to my commands</button>
  {/snippet}
</TfDialog>
