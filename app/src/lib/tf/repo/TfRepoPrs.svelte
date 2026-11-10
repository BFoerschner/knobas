<!--
  The repository page's *Pull requests* tab (#838, R-8.10): this repository's
  pull requests with a filter, as prototype-6's `renderRepo(...,'prs')`, in
  Code's row: its number, its title over its author (`code_facts`), its state
  chip, the job it belongs to (`job/open-jobs.ts`) and when it last moved. From
  the mirror (`list_entities`), every state, newest first; a row opens
  `#/pr/<entity id>`. A pull request this session queued stands first with its
  chip, *Queued* then *Sent*, until the mirror holds its title (F-242).
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
  import { echoesOn, settleEchoes, watchEchoes } from "../queued.svelte";
  import { tick, untrack } from "svelte";
  import type { EchoListen } from "../queued.svelte";
  import { jumpAcross } from "../opener";
  import { MADE_PR, REPO_PRS, visibleRepoPrs } from "./repo";
  import TfWaiting from "../TfWaiting.svelte";
  import { waitPhase } from "../waiting";

  let {
    ports,
    repoId,
    rows,
    job,
    now,
    newPr = null,
    reread = () => {},
  }: {
    ports: OpenJobPorts & { codeFacts: (entityIds: string[]) => Promise<CodeFacts[]>; listen?: EchoListen };
    repoId: string;
    /** The repository's pull requests, or `null` before the read answered. */
    rows: EntityRow[] | null;
    job: string | null;
    /** The clock the times are drawn against. */
    now: Date;
    /** Where *New pull request* goes, where there is a branch to propose. */
    newPr?: string | null;
    /** Reads the rows again: what brings a queued one in. */
    reread?: () => void;
  } = $props();

  /** The row a press landed on takes focus, where the press left it on nothing: *Create pull request*'s form has gone (F-242). */
  function arrive(row: HTMLElement) {
    void tick().then(() => {
      const at = document.activeElement;
      if (at === null || at === document.body || at.closest("[hidden], [inert]") !== null) row.focus({ preventScroll: true });
    });
  }

  /** What this session queued that the rows don't hold yet. */
  const sent = $derived(echoesOn(repoId, MADE_PR));
  $effect(() => {
    if (rows === null) return;
    const titles = new Set(rows.map((row) => row.title.trim()));
    untrack(() => settleEchoes(repoId, (echo) => echo.kind === MADE_PR && titles.has(echo.text.trim())));
  });
  $effect(() => untrack(() => watchEchoes(REPO_PRS, () => reread(), { listen: ports.listen })));

  let jobs = $state<OpenJob[]>([]);
  /** Each pull request's author, once `code_facts` answered, as Code's rows say it: *you* for the reader's own. A failed read leaves the line off. */
  let authors = $state.raw<ReadonlyMap<string, string>>(new Map());

  $effect(() => {
    const ids = rows?.map((row) => row.entity_id) ?? [];
    if (ids.length === 0) return;
    let live = true;
    ports.codeFacts(ids).then(
      (facts) => {
        if (live) authors = new Map(facts.flatMap((fact) => (fact.author ? [[fact.entity_id, fact.mine ? "you" : (fact.author_name ?? fact.author)] as const] : [])));
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
  <FilterBar {config} {filter} shown={rows === null ? null : shown.length + sent.length} total={rows === null ? null : rows.length + sent.length} />
</div>
<!-- The page reads the rows; a refresh replaces them in place, so only the first read waits here (#2246). -->
<TfWaiting phase={waitPhase({ drawn: rows !== null, reading: false, failed: false })} columns={5}>
{#if rows !== null && rows.length === 0 && sent.length === 0}
  <div class="tf-empty">
    <p>This repository has no pull requests yet.</p>
    {#if newPr !== null}<a
        class="tf-btn tf-sm"
        href={newPr}
        onclick={(event) => jumpAcross(event, "Pull requests")}>New pull request</a>{/if}
  </div>
{:else if shown.length === 0 && sent.length === 0}
  <TfNoMatch noun="pull requests" {filter} />
{:else}
  <!-- Code's row and Code's columns: one list of pull requests, drawn one way. -->
  <table class="tf-tbl tf-rprs tf-cprs">
    <colgroup><col class="tf-cp-no" /><col /><col class="tf-cp-state" /><col class="tf-cp-job" /><col class="tf-cp-at" /></colgroup>
    <tbody>
      {#each sent as echo (echo.write ?? echo.text)}
        <tr class="tf-rsent" data-place>
          <td class="tf-prno"></td>
          <td><span class="tf-rowt" tabindex="-1" {@attach arrive}>{echo.text}</span></td>
          <td><TfStateChip text={echo.state} tone="info" /></td>
          <td></td>
          <td></td>
        </tr>
      {/each}
      {#each shown as row (row.entity_id)}
        {@const belongs = index.jobOf({ entity_id: row.entity_id, kind: row.kind })}
        {@const author = authors.get(row.entity_id)}
        <tr class="tf-row" data-place onclick={(event) => open(event, itemHash(row))}>
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
