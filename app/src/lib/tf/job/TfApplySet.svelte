<!--
  *Apply a link set…* (#869, R-7.8), from the job's *Also on this job* bar and
  the Connections drawer: prototype-6's `openApplySet`. Every set, a filter
  over name and description, and what each holds; a set that already applies
  to the ticket says so and can't be picked. One set is picked, the first one
  the filter leaves that can apply until another is, and the footer's primary
  applies it, so `Enter` in the filter applies the top match (#2289). *Apply*
  hands the set back: the job saves it with a by-hand rule and toasts the count
  with *Undo*.
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

  const uid = $props.id();
  let pickedId = $state<string | null>(null);
  /** The set the primary applies: the one picked while the filter shows it, else the first shown that can apply. */
  const chosen = $derived(
    shown.find((set) => set.id === pickedId && !applied.has(set.id)) ?? shown.find((set) => !applied.has(set.id)) ?? null,
  );

  function apply(event: SubmitEvent) {
    event.preventDefault();
    if (chosen !== null) onapply(chosen);
  }
</script>

<TfDialog title="Apply a link set to {jobKey}" {onclose} {layers}>
  {#snippet body()}
    <form id="tf-aps-{uid}" class="tf-dform" onsubmit={apply}></form>
    <label class="tf-fsearch tf-wide"
      ><svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
        ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
      ><input bind:value={typed} form="tf-aps-{uid}" placeholder="Filter link sets" aria-label="Filter link sets" /></label
    >
    {#if failed !== null}
      <p class="tf-ifail">Could not read the link sets: {failed}</p>
    {:else if sets !== null && sets.length === 0}
      <p class="tf-dim">No link sets yet.</p>
    {/if}
    <div class="tf-picks" role="radiogroup" aria-label="Link sets">
      {#each shown as set (set.id)}
        {@const on = applied.has(set.id)}
        <label class="tf-pickrow" class:tf-in={on}>
          <input
            type="radio"
            name="tf-aps-{uid}"
            value={set.id}
            disabled={on}
            checked={chosen?.id === set.id}
            onchange={() => (pickedId = set.id)}
          />
          <span><b><Marked text={set.name} {typed} /></b><span class="tf-dim tf-pick-sub">{contentLines(set).join(", ")}</span></span>
          <span class="tf-dim">{on ? "already applies" : ""}</span>
        </label>
      {/each}
    </div>
    <p class="tf-dim tf-small">
      Applying by hand affects only {jobKey}. To cover every ticket under an epic, add a rule in the set instead.
    </p>
    <p class="tf-dim tf-stays">Link sets stay in knobas and are never written to a source.</p>
  {/snippet}
  {#snippet footer()}
    <a class="tf-btn tf-first" href="#/sets" onclick={onclose}>Manage link sets</a>
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="submit" class="tf-btn tf-primary" form="tf-aps-{uid}" disabled={chosen === null}
      >{chosen === null ? "Apply" : `Apply ${chosen.name}`}</button
    >
  {/snippet}
</TfDialog>
