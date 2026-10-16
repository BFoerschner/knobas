<!--
  *Add a pattern…* (#868, R-7.2, R-7.7), prototype-6's `openPattern`: for
  pipelines a project (or any) and a name glob, for pages a space and a parent
  page (or the whole space), and a live "Matches now: N" list. The fields are
  written as a query in the search grammar (`patternQuery`), and the backend
  reads it (`preview_link_set_pattern`): what the list shows is what the set
  will match, and the reason it says when the query is not a pattern.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { LinkSetMemberKind, LinkSetPatternPreview } from "../../ipc/entity";
  import TfDialog from "../settings/TfDialog.svelte";
  import { dotClass, patternQuery } from "./sets";

  let {
    kind,
    setName,
    preview,
    onadd,
    onclose,
  }: {
    kind: LinkSetMemberKind;
    setName: string;
    preview: (query: string) => Promise<LinkSetPatternPreview>;
    onadd: (query: string) => void;
    onclose: () => void;
  } = $props();

  /** How many matches the list draws. */
  const DRAWN = 150;

  let project = $state("");
  let typed = $state("");
  let answer = $state<LinkSetPatternPreview | null>(null);
  let failed = $state<string | null>(null);
  /** The newest preview asked for; an older answer arriving late is dropped. */
  let asked = 0;

  const pipelines = $derived(kind === "pipeline");
  const query = $derived(patternQuery(kind, project, typed));
  const projects = $derived(answer?.projects ?? []);
  const addable = $derived(answer !== null && answer.refusal === null && failed === null);

  $effect(() => {
    const wanted = query;
    const turn = ++asked;
    preview(wanted).then(
      (read) => {
        if (turn !== asked) return;
        answer = read;
        failed = null;
        // A page pattern is a space: the first one the pages sit in, until one is picked.
        if (!pipelines && project === "" && read.projects[0]) project = read.projects[0].key;
      },
      (error: unknown) => {
        if (turn === asked) failed = ipcErrorMessage(error);
      },
    );
  });

  const projectLabel = (row: { key: string; name: string | null }) =>
    row.name && row.name !== row.key ? `${row.key} ${row.name}` : row.key;
</script>

<TfDialog title="Add a pattern to {setName}" {onclose}>
  {#snippet body()}
    <p class="tf-dim tf-pat-intro">
      A pattern is a saved filter. It picks up new {pipelines ? "pipelines" : "pages"} as they appear, without editing the set.
    </p>
    <div class="tf-formgrid">
      {#if pipelines}
        <span>Project</span>
        <select aria-label="Project" bind:value={project}>
          <option value="">Any</option>
          {#each projects as row (`${row.source_id}|${row.key}`)}<option value={row.key}>{projectLabel(row)}</option>{/each}
        </select>
        <span>Name matches</span>
        <input type="text" aria-label="Name matches" bind:value={typed} placeholder="e.g. Payout_*" />
      {:else}
        <span>Space</span>
        <select aria-label="Space" bind:value={project}>
          {#each projects as row (`${row.source_id}|${row.key}`)}<option value={row.key}>{projectLabel(row)}</option>{/each}
        </select>
        <span>Under page</span>
        <input type="text" aria-label="Under page" bind:value={typed} placeholder="e.g. Runbooks (empty for the whole space)" />
      {/if}
    </div>
    {#if failed !== null}
      <p class="tf-ifail">Could not read what the pattern matches: {failed}</p>
    {:else if answer?.refusal}
      <p class="tf-ifail">{answer.refusal}</p>
    {/if}
    <div class="tf-pane-h" aria-live="polite">Matches now: {answer?.refusal ? 0 : (answer?.count ?? 0)}</div>
    <div class="tf-picks tf-pat-matches" role="list" aria-label="Matches now">
      {#each answer?.refusal ? [] : (answer?.matches ?? []).slice(0, DRAWN) as row (row.entity_id)}
        <div class="tf-pat-match" role="listitem">
          <span class="tf-dot {dotClass(row.state)}"></span>{row.title ?? row.entity_id}<span class="tf-dim">{row.line ?? ""}</span>
        </div>
      {/each}
    </div>
    <p class="tf-dim tf-pat-query">Stored as <code>{query}</code></p>
    <p class="tf-dim tf-stays">Link sets stay in knobas and are never written to a source.</p>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="button" class="tf-btn tf-primary" disabled={!addable} onclick={() => onadd(query)}>Add pattern</button>
  {/snippet}
</TfDialog>
