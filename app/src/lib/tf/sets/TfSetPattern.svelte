<!--
  *Add a pattern…* (#868, R-7.2, R-7.7), prototype-6's `openPattern`: for
  pipelines a project (or any) and a name glob, for pages a space and a parent
  page (or the whole space), and a live "Matches now: N" list. It waits for a
  project, a name or a space with a line, as pipelines and pages both ask for
  something: only a single space is taken for the reader. The fields are
  written as a query in the search grammar (`patternQuery`), and the backend
  reads it (`preview_link_set_pattern`): what the list shows is what the set
  will match, and the reason it says when the query is not a pattern.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { LinkSetMemberKind, LinkSetPatternPreview } from "../../ipc/entity";
  import TfDialog from "../settings/TfDialog.svelte";
  import { patternQuery, patternShown } from "./sets";
  import { stateWord } from "../state-word";
  import { toneClass } from "../tone";

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

  const uid = $props.id();

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
  /** A nested project by its path, so a parent sorts before its subprojects (#1019). */
  const projects = $derived(
    [...(answer?.projects ?? [])].sort((a, b) => (pipelines ? projectLabel(a).localeCompare(projectLabel(b)) : 0)),
  );
  /** A pipeline pattern with neither a project nor a name would add every pipeline, and a page pattern is a space: each asks first. */
  const chosen = $derived(project !== "" || (pipelines && typed.trim() !== ""));
  const addable = $derived(chosen && answer !== null && answer.refusal === null && failed === null);
  /** What the query matches now; a refused query matches nothing. */
  const matched = $derived(answer && answer.refusal === null ? answer : { count: 0, matches: [] });

  $effect(() => {
    const wanted = query;
    const turn = ++asked;
    preview(wanted).then(
      (read) => {
        if (turn !== asked) return;
        answer = read;
        failed = null;
        // One space leaves nothing to choose between, so it is taken (the enabled rule).
        if (!pipelines && project === "" && read.projects.length === 1) project = read.projects[0]!.key;
      },
      (error: unknown) => {
        if (turn === asked) failed = ipcErrorMessage(error);
      },
    );
  });

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (addable) onadd(query);
  }

  /** prototype-6's `Payments › Payouts` for a project in a chain; else the key and its name. */
  function projectLabel(row: { key: string; name: string | null; path: string[] }): string {
    if (row.path.length) return row.path.join(" › ");
    return row.name && row.name !== row.key ? `${row.key} ${row.name}` : row.key;
  }
</script>

<TfDialog why={!chosen ? (pipelines ? "Pick a project or type a name." : "Pick a space.") : failed !== null || answer?.refusal ? "Change it: knobas can’t read this pattern." : null} title="Add a pattern to {setName}" {onclose}>
  {#snippet body()}
    <form id="tf-pat-{uid}" class="tf-dform" onsubmit={submit}></form>
    <p class="tf-dim tf-pat-intro">
      A pattern is a saved filter. It picks up new {pipelines ? "pipelines" : "pages"} as they appear, without editing the set.
    </p>
    <div class="tf-formgrid">
      {#if pipelines}
        <label for="tf-pat-project-{uid}">Project</label>
        <select id="tf-pat-project-{uid}" aria-label="Project" bind:value={project}>
          <option value="">Any</option>
          {#each projects as row (`${row.source_id}|${row.key}`)}<option value={row.key}>{projectLabel(row)}</option>{/each}
        </select>
        <label for="tf-pat-typed-{uid}">Name matches</label>
        <input id="tf-pat-typed-{uid}" type="text" aria-label="Name matches" form="tf-pat-{uid}" data-arrive bind:value={typed} placeholder="e.g. Payout_*" />
      {:else}
        <label for="tf-pat-project-{uid}">Space</label>
        <select id="tf-pat-project-{uid}" aria-label="Space" bind:value={project}>
          {#if project === ""}<option value="" disabled>Choose a space…</option>{/if}
          {#each projects as row (`${row.source_id}|${row.key}`)}<option value={row.key}>{projectLabel(row)}</option>{/each}
        </select>
        <label for="tf-pat-typed-{uid}">Under page</label>
        <input id="tf-pat-typed-{uid}" type="text" aria-label="Under page" form="tf-pat-{uid}" bind:value={typed} placeholder="e.g. Runbooks (empty for the whole space)" />
      {/if}
    </div>
    {#if failed !== null}
      <p class="tf-ifail">Could not read what the pattern matches: {failed}</p>
    {:else if chosen && answer?.refusal}
      <p class="tf-ifail">{answer.refusal}</p>
    {/if}
    {#if chosen}<div class="tf-pane-h" aria-live="polite">Matches now: {matched.count}</div>{/if}
    {#if chosen && answer !== null && matched.count === 0}
      <p class="tf-dim tf-picks-more">Nothing matches this yet. A pattern with no matches still picks up what appears later.</p>
    {/if}
    <div class="tf-picks tf-pat-matches" class:tf-hidden-empty={matched.count === 0 || !chosen} role="list" aria-label="Matches now">
      {#each matched.matches.slice(0, DRAWN) as row (row.entity_id)}
        <div class="tf-pat-match" role="listitem">
          <!-- A dot only where the source reports a state: tone is for state, and a row of grey dots says nothing. -->
          {#if row.state}<span class="tf-dot {toneClass(row.state)}" title={stateWord(row.state.text)}></span>{/if}{row.title ?? row.entity_id}<span class="tf-dim">{row.line ?? ""}</span>
        </div>
      {/each}
    </div>
    {#if chosen}<p class="tf-dim tf-pat-query">Pattern: <code>{patternShown(kind, query)}</code></p>{/if}
    <p class="tf-dim tf-stays">Link sets stay in knobas and are never written to a source.</p>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="submit" class="tf-btn tf-primary" form="tf-pat-{uid}" disabled={!addable}>Add pattern</button>
  {/snippet}
</TfDialog>
