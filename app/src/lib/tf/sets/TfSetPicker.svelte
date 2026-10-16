<!--
  The *Add…* picker of one set editor panel (#866, R-7.7), prototype-6's
  `openPicker`: a filterable multi-select over every item of that kind, where
  an item already in the set is ticked, disabled and says *already in*. *Add*
  hands the picked ids back; the editor saves the set.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { LinkSetMemberKind } from "../../ipc/entity";
  import { matchesAll } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import type { Candidate } from "./ports";
  import { KIND_LABEL } from "./sets";

  let {
    kind,
    setName,
    have,
    read,
    onadd,
    onclose,
  }: {
    kind: LinkSetMemberKind;
    setName: string;
    /** The ids already in the set, of any kind. */
    have: ReadonlySet<string>;
    read: () => Promise<Candidate[]>;
    onadd: (picked: Candidate[]) => void;
    onclose: () => void;
  } = $props();

  /** How many rows the list draws before asking for a narrower filter, as prototype-6. */
  const DRAWN = 150;

  let all = $state<Candidate[] | null>(null);
  let failed = $state<string | null>(null);
  let typed = $state("");
  let picked = $state<string[]>([]);

  $effect(() => {
    let live = true;
    read().then(
      (rows) => {
        if (live) all = rows;
      },
      (error: unknown) => {
        if (live) failed = ipcErrorMessage(error);
      },
    );
    return () => {
      live = false;
    };
  });

  const noun = $derived(KIND_LABEL[kind].toLowerCase());
  const shown = $derived((all ?? []).filter((row) => matchesAll(`${row.name} ${row.sub}`, typed)));

  function toggle(id: string, on: boolean) {
    picked = on ? [...picked, id] : picked.filter((held) => held !== id);
  }

  function add() {
    const chosen = (all ?? []).filter((row) => picked.includes(row.entity_id));
    if (chosen.length > 0) onadd(chosen);
    else onclose();
  }
</script>

<TfDialog title="Add {noun} to {setName}" {onclose}>
  {#snippet body()}
    <label class="tf-fsearch tf-wide"
      ><input bind:value={typed} placeholder="Filter {all?.length ?? ''} {noun}" aria-label="Filter" /></label
    >
    {#if failed !== null}
      <p class="tf-ifail">Could not read the {noun}: {failed}</p>
    {/if}
    <div class="tf-picks" role="group" aria-label={KIND_LABEL[kind]}>
      {#each shown.slice(0, DRAWN) as row (row.entity_id)}
        {@const held = have.has(row.entity_id)}
        <label class:tf-in={held}
          ><input
            type="checkbox"
            checked={held || picked.includes(row.entity_id)}
            disabled={held}
            onchange={(event) => toggle(row.entity_id, event.currentTarget.checked)}
          /> <Marked text={row.name} {typed} /><span class="tf-dim">{held ? "already in" : row.sub}</span></label
        >
      {/each}
      {#if shown.length > DRAWN}
        <p class="tf-dim tf-picks-more">{shown.length - DRAWN} more; narrow the filter.</p>
      {/if}
    </div>
    <p class="tf-dim tf-stays">Link sets stay in knobas and are never written to a source.</p>
  {/snippet}
  {#snippet footer()}
    <span class="tf-dim tf-picked">{picked.length} picked</span>
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="button" class="tf-btn tf-primary" onclick={add}>Add</button>
  {/snippet}
</TfDialog>
