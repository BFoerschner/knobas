<!--
  *Clone all…* (the smoothness pass, PR #2479): clones every repository knobas
  knows and this machine lacks into the clones root, without opening each one.
  On Code's header and on Settings › Checkouts' clones root.

  A press first reads what it would do (`clone_all_plan`: the mirror and the
  disk, never a source) and says it in a dialog: how many, from which
  sources, how many are already here, and which cannot be cloned and why;
  the size is not known before a clone. *Clone N* starts the run in the
  background (`clone-all.svelte.ts`) and the button says where it is; the end
  is one toast with every failure and its reason. A press while the plan is
  read, or while a run goes, starts nothing.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { CloneAllPlan } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { cloneAllStore, repositories, type CloneAll } from "./clone-all.svelte";

  let {
    store = cloneAllStore,
    layers = sharedLayers,
    small = false,
  }: {
    store?: CloneAll;
    layers?: Layers;
    /** The settings row's size. */
    small?: boolean;
  } = $props();

  let plan = $state<CloneAllPlan | null>(null);
  let asking = $state(false);

  $effect(() => {
    void store.ensure();
  });

  const run = $derived(store.run);
  const going = $derived(store.running);
  const label = $derived(
    going && run !== null ? `Cloning ${Math.min(run.cloned + run.adopted + run.failed.length + 1, run.total)} of ${run.total}…` : "Clone all…",
  );
  const now = $derived(going && run !== null && run.current.length > 0 ? `Cloning ${run.current.join(" and ")}` : null);

  async function ask() {
    if (asking || going) return;
    asking = true;
    try {
      plan = await store.plan();
    } catch (cause) {
      push({ text: `Could not read what to clone: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      asking = false;
    }
  }

  function go() {
    const root = plan?.root ?? null;
    plan = null;
    void store.start(root);
  }

  const count = $derived(plan?.names.length ?? 0);
  const why = $derived(plan === null ? null : plan.root === null ? "No clones root is set." : count === 0 ? "Every repository is already here." : null);
</script>

<button
  type="button"
  class="tf-btn"
  class:tf-sm={small}
  aria-disabled={going || asking ? "true" : undefined}
  title={now ?? "Clone every repository knobas knows that is not on this machine"}
  onclick={() => void ask()}>{label}</button
>

{#if plan !== null}
  {@const shown = plan}
  <TfDialog title={count === 0 ? "Nothing to clone" : `Clone ${repositories(count)}?`} why={why} {layers} onclose={() => (plan = null)}>
    {#snippet body()}
      {#if shown.root === null}
        <p class="tf-dlg-note">No clones root is set. Choose where clones go in Settings › Checkouts first.</p>
      {:else if count === 0}
        <p class="tf-dlg-note">Every repository knobas knows is already on this machine.</p>
      {:else}
        <p class="tf-dlg-note">
          Into <span class="tf-mono">{shown.root}</span>, from
          {shown.sources.map((s) => `${s.name} (${s.repositories})`).join(", ")}. They clone in the background, two at a time, and the
          app stays usable; the size is known only once each has cloned.
        </p>
        <!-- The dialog's arrival: what it would clone, focusable so a second Enter on *Clone all…* lands on words, never on a button. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <p class="tf-dsub" tabindex="0" aria-label="Repositories to clone">{shown.names.join(", ")}</p>
      {/if}
      {#if shown.already > 0}
        <p class="tf-dsub">{repositories(shown.already)} already on this machine {shown.already === 1 ? "is" : "are"} left as {shown.already === 1 ? "it is" : "they are"}.</p>
      {/if}
      {#if shown.skipped.length > 0}
        <p class="tf-dsub">Not cloned: {shown.skipped.map((s) => `${s.name} (${s.reason})`).join("; ")}.</p>
      {/if}
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" onclick={() => (plan = null)}>Cancel</button>
      <button type="button" class="tf-btn tf-primary" disabled={why !== null} onclick={go}>Clone {repositories(count)}</button>
    {/snippet}
  </TfDialog>
{/if}
