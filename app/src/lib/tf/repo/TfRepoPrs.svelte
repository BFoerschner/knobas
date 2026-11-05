<!--
  The repository page's *Pull requests* tab (#838, R-8.10): this repository's
  pull requests with a filter, as prototype-6's `renderRepo(...,'prs')`, in
  Code's row: its number, its title over its author (`code_facts`), its state
  chip, the job it belongs to (`job/open-jobs.ts`) and when it last moved. From
  the mirror (`list_entities`), every state, newest first; a row opens
  `#/pr/<entity id>`.
-->
<script lang="ts">
  import TfKey from "../TfKey.svelte";
  import TfStateChip from "../TfStateChip.svelte";
  import type { CodeFacts, EntityRow } from "../../ipc/entity";
  import { clockLabel } from "../../time/format";
  import { prNumber, PR_STATES } from "../code/code";
  import FilterBar from "../filter/FilterBar.svelte";
  import TfNoMatch from "../filter/TfNoMatch.svelte";
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
    now,
    newPr = null,
  }: {
    ports: OpenJobPorts & { codeFacts: (entityIds: string[]) => Promise<CodeFacts[]> };
    repoId: string;
    /** The repository's pull requests, or `null` before the read answered. */
    rows: EntityRow[] | null;
    job: string | null;
    /** The clock the times are drawn against. */
    now: Date;
    /** Where *New pull request* goes, where there is a branch to propose. */
    newPr?: string | null;
  } = $props();

  let jobs = $state<OpenJob[]>([]);
  /** Each pull request's author, once `code_facts` answered; a failed read leaves the line off. */
  let authors = $state.raw<ReadonlyMap<string, string>>(new Map());

  $effect(() => {
    const ids = rows?.map((row) => row.entity_id) ?? [];
    if (ids.length === 0) return;
    let live = true;
    ports.codeFacts(ids).then(
      (facts) => {
        if (live) authors = new Map(facts.flatMap((fact) => (fact.author ? [[fact.entity_id, fact.author] as const] : [])));
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });

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
    placeholder: "Filter PRs",
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
<TfWaiting phase={waitPhase({ drawn: rows !== null, reading: false, failed: false })} columns={5}>
{#if rows !== null && rows.length === 0}
  <div class="tf-empty">
    <p>This repository has no pull requests yet.</p>
    {#if newPr !== null}<a class="tf-btn tf-sm" href={newPr}>New pull request</a>{/if}
  </div>
{:else if shown.length === 0}
  <TfNoMatch noun="pull requests" {filter} />
{:else}
  <!-- Code's row and Code's columns: one list of pull requests, drawn one way. -->
  <table class="tf-tbl tf-rprs tf-cprs">
    <tbody>
      {#each shown as row (row.entity_id)}
        {@const belongs = index.jobOf({ entity_id: row.entity_id, kind: row.kind })}
        {@const author = authors.get(row.entity_id)}
        <tr class="tf-row" onclick={(event) => open(event, itemHash(row))}>
          <td class="tf-prno"><Marked text={prNumber(row.entity_id) ?? ""} typed={filter.text} /></td>
          <td
            ><a class="tf-rowt" href={itemHash(row)}><Marked text={row.title} typed={filter.text} /></a>{#if author}<span class="tf-sub"><Marked text={author} typed={filter.text} /></span>{/if}</td
          >
          <td>{#if row.state}<TfStateChip text={row.state.text} tone={row.state.tone} />{/if}</td>
          <td class="tf-mono tf-dim">{#if belongs !== null && belongs === job}<TfKey chip value={belongs} class="tf-chip-job" />{:else}{belongs ?? ""}{/if}</td>
          <td class="tf-dim tf-nowrap">{row.updated_at ? clockLabel(row.updated_at, now) : ""}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}
</TfWaiting>
