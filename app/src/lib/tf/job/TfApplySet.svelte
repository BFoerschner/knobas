<!--
  *Apply a link set…* (#869, R-7.8), from the job's *Also on this job* bar and
  the Connections drawer: prototype-6's `openApplySet`. Every set, a filter
  over name and description, what each holds, and *Apply* on each; a set that
  already applies to the ticket says so instead. *Apply* hands the set back:
  the job saves it with a by-hand rule and toasts the count with *Undo*.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { LinkSetRow } from "../../ipc/entity";
  import { matchesAll } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import type { Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { contentLines } from "../sets/sets";

  let {
    jobKey,
    applied,
    read,
    layers,
    onapply,
    onclose,
  }: {
    jobKey: string;
    /** The ids of the sets that apply to the ticket now. */
    applied: ReadonlySet<string>;
    read: () => Promise<LinkSetRow[]>;
    layers: Layers;
    onapply: (set: LinkSetRow) => void;
    onclose: () => void;
  } = $props();

  let sets = $state<LinkSetRow[] | null>(null);
  let failed = $state<string | null>(null);
  let typed = $state("");

  $effect(() => {
    let live = true;
    read().then(
      (rows) => {
        if (live) sets = rows;
      },
      (error: unknown) => {
        if (live) failed = ipcErrorMessage(error);
      },
    );
    return () => {
      live = false;
    };
  });

  const shown = $derived((sets ?? []).filter((set) => matchesAll(`${set.name} ${set.description}`, typed)));
</script>

<TfDialog title="Apply a link set to {jobKey}" {onclose} {layers}>
  {#snippet body()}
    <label class="tf-fsearch tf-wide"
      ><svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
        ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
      ><input bind:value={typed} placeholder="Filter link sets" aria-label="Filter link sets" /></label
    >
    {#if failed !== null}
      <p class="tf-ifail">Could not read the link sets: {failed}</p>
    {:else if sets !== null && sets.length === 0}
      <p class="tf-dim">No link sets yet.</p>
    {/if}
    <div class="tf-picks" role="list" aria-label="Link sets">
      {#each shown as set (set.id)}
        {@const on = applied.has(set.id)}
        <div class="tf-pickrow" class:tf-in={on} role="listitem">
          <span><b><Marked text={set.name} {typed} /></b><span class="tf-dim tf-pick-sub">{contentLines(set).join(", ")}</span></span>
          <span class="tf-dim">{on ? "already applies" : ""}</span>
          {#if !on}
            <button type="button" class="tf-btn tf-sm" aria-label="Apply {set.name}" onclick={() => onapply(set)}>Apply</button>
          {/if}
        </div>
      {/each}
    </div>
    <p class="tf-dim tf-small">
      Applying by hand affects only {jobKey}. To cover every ticket under an epic, add a rule in the set instead.
    </p>
    <p class="tf-dim tf-stays">Link sets stay in knobas and are never written to a source.</p>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Close</button>
    <a class="tf-btn" href="#/sets" onclick={onclose}>Manage link sets</a>
  {/snippet}
</TfDialog>
