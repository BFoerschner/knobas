<!--
  *Apply a link set…* (#869, R-7.8), from the job's *Also on this job* bar and
  the Connections drawer: prototype-6's `openApplySet`. Every set, a filter
  over name and description, and what each holds; a set that already applies
  to the ticket says so and can't be picked. Nothing is picked on arrival, so a
  press can't apply an alphabetical accident, and a line beside the primary says
  so. Typing picks the first set the filter leaves until another is picked, so
  `Enter` in the filter still applies the top match (#2289). `↓` goes from the
  filter into the list, picking as it lands, and `Enter` on a picked row applies it:
  the dialog's rule that `Enter` on a radio sends nothing guards an arrival, and
  focus only gets here by a press. *Apply* hands the set back: the job saves it
  with a by-hand rule and toasts the count with *Undo*.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { LinkSetRow } from "../../ipc/entity";
  import { matchesAll } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import type { Layers } from "../layers.svelte";
  import { goVisiting } from "../opener";
  import TfDialog from "../settings/TfDialog.svelte";
  import { intoList, withinList } from "../sets/list-keys";
  import { contentLines } from "../sets/sets";
  import TfKey from "../TfKey.svelte";

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
  /** The set the primary applies: the one picked while the filter shows it, else, once something is typed, the first shown that can apply. */
  const chosen = $derived(
    shown.find((set) => set.id === pickedId && !applied.has(set.id)) ??
      (typed.trim() === "" ? null : shown.find((set) => !applied.has(set.id))) ??
      null,
  );
  /** What the primary waits for, in words beside it (design system, Disabled). */
  const missing = $derived.by(() => {
    if (chosen !== null) return null;
    if (failed !== null) return "The link sets could not be read.";
    if (sets === null) return null;
    if (sets.length === 0) return "No link sets yet.";
    if (shown.length === 0) return "No link set matches.";
    return shown.every((set) => applied.has(set.id)) ? "These already apply." : "Pick a link set.";
  });

  /** The sets' page says where it came from, so `Esc` and *Back to* come back here. */
  function manage(event: MouseEvent) {
    event.preventDefault();
    goVisiting("#/sets");
    onclose();
  }

  function apply(event: SubmitEvent) {
    event.preventDefault();
    if (chosen !== null) onapply(chosen);
  }

  let filterEl = $state<HTMLInputElement | null>(null);
  let listEl = $state<HTMLElement | null>(null);

  function rowKey(event: KeyboardEvent) {
    withinList(event, listEl, filterEl);
    if (event.key !== "Enter" || chosen === null) return;
    // Before `TfDialog` reads it.
    event.preventDefault();
    event.stopPropagation();
    onapply(chosen);
  }
</script>

<TfDialog why={missing} {onclose} {layers}>
  {#snippet title()}Apply a link set to <TfKey value={jobKey} />{/snippet}
  {#snippet body()}
    <form id="tf-aps-{uid}" class="tf-dform" onsubmit={apply}></form>
    <label class="tf-fsearch tf-wide"
      ><svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
        ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
      ><input bind:this={filterEl} bind:value={typed} form="tf-aps-{uid}" placeholder="Filter link sets" aria-label="Filter link sets" onkeydown={(event) => intoList(event, listEl)} /></label
    >
    {#if failed !== null}
      <p class="tf-ifail">Could not read the link sets: {failed}</p>
    {:else if sets === null}
      <p class="tf-dim tf-picks-more">Reading the link sets…</p>
    {:else if sets.length === 0}
      <p class="tf-dim tf-picks-more">No link sets yet. Manage link sets starts one.</p>
    {:else if shown.length === 0}
      <p class="tf-dim tf-picks-more">No link set matches “{typed.trim()}”.</p>
    {/if}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="tf-picks" class:tf-hidden-empty={shown.length === 0} role="radiogroup" aria-label="Link sets" bind:this={listEl} onkeydown={rowKey}>
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
      Applying by hand affects only <TfKey value={jobKey} />. To cover every ticket under an epic, add a rule in the set instead.
    </p>
    <p class="tf-dim tf-stays">Link sets stay in knobas and are never written to a source.</p>
  {/snippet}
  {#snippet footer()}
    <a class="tf-btn tf-first" href="#/sets" onclick={manage}>Manage link sets</a>
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="submit" class="tf-btn tf-primary" form="tf-aps-{uid}" disabled={chosen === null}
      >{chosen === null ? "Apply" : `Apply ${chosen.name}`}</button
    >
  {/snippet}
</TfDialog>
