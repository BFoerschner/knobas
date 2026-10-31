<!--
  The repository page's *Pull requests* tab (#838, R-8.10): this repository's
  pull requests with a filter, as prototype-6's `renderRepo(...,'prs')`. Each
  row: its number, its title, its state chip and the job it belongs to
  (`job/open-jobs.ts`). From the mirror (`list_entities`), every state, newest
  first; a row opens `#/pr/<entity id>`.
-->
<script lang="ts">
  import type { EntityRow } from "../../ipc/entity";
  import { prNumber, PR_STATES } from "../code/code";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import { itemHash } from "../job/job";
  import { jobIndex, readOpenJobs, type OpenJob, type OpenJobPorts } from "../job/open-jobs";
  import { visibleRepoPrs } from "./repo";
  import TfWaiting from "../TfWaiting.svelte";
  import { waitPhase } from "../waiting";

  let {
    ports,
    repoId,
    rows,
    job,
  }: {
    ports: OpenJobPorts;
    repoId: string;
    /** The repository's pull requests, or `null` before the read answered. */
    rows: EntityRow[] | null;
    job: string | null;
  } = $props();

  let jobs = $state<OpenJob[]>([]);

  $effect(() => {
    const current = job;
    let live = true;
    void readOpenJobs(ports, current).then((open) => {
      if (live) jobs = open;
    });
    return () => {
      live = false;
    };
  });

  const index = $derived(jobIndex(jobs));
  const filter = $derived(sessionFilter(`repo-prs:${repoId}`));
  const config = $derived<FilterConfig>({
    id: `repo-prs:${repoId}`,
    placeholder: "Filter pull requests",
    noun: "pull requests",
    one: "pull request",
    chips: [{ key: "state", label: "State", options: PR_STATES.map((value) => ({ value, label: value })) }],
  });
  const shown = $derived(rows === null ? [] : visibleRepoPrs(rows, filter.chips["state"] ?? [], filter.text));

  function open(event: MouseEvent, href: string) {
    if ((event.target as Element | null)?.closest("a")) return;
    location.hash = href;
  }
</script>

<div class="tf-rbar">
  <FilterBar {config} {filter} shown={rows === null ? null : shown.length} total={rows?.length ?? null} />
</div>
<!-- The page reads the rows; a refresh replaces them in place, so only the first read waits here (#2246). -->
<TfWaiting phase={waitPhase({ drawn: rows !== null, reading: false, failed: false })} columns={4}>
{#if shown.length === 0}
  <div class="tf-empty">No pull requests match.</div>
{:else}
  <table class="tf-tbl tf-rprs">
    <tbody>
      {#each shown as row (row.entity_id)}
        <tr class="tf-row" onclick={(event) => open(event, itemHash(row))}>
          <td class="tf-mono"><Marked text={prNumber(row.entity_id) ?? ""} typed={filter.text} /></td>
          <td><a href={itemHash(row)}><Marked text={row.title} typed={filter.text} /></a></td>
          <td>{#if row.state}<span class="tf-chip tf-{row.state.tone}">{row.state.text}</span>{/if}</td>
          <td class="tf-mono tf-dim">{index.jobOf({ entity_id: row.entity_id, kind: row.kind }) ?? ""}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}
</TfWaiting>
