<!--
  Builds (#848, R-9.1, R-9.4a, ADR-0022): the tracer of the Builds frame.
  The shell draws it over the declared pipeline and run kinds, so nothing
  here names an adapter: the source's word is its configured name, drawn
  once, as the header line over its pipelines.

  - **Left**, the pipeline list: with a job current, *This job <key> / All*
    (#850, *This job* by default) and the *Failing* and *Running* chips; the
    page's search is the launcher's `#b`, which `/` opens (#2425). *All* draws *Pinned* first
    (knobas-local, #849), then each source's project tree as deep as it goes
    (folds with their count and *N failing*, own pipelines before
    subgroups, a chain of single children as one row, a fold open on a
    failure, the selection or anything narrowing, else as the reader left
    it). *This job* draws the same tree pruned to the job's pipelines, each
    with its reasons, under "N of M pipelines are relevant to KEY". The
    list is a tree of one-line rows (the rail-tree pass, 2026-11-13): each
    pipeline a glyph of its latest run (`TfRunGlyph`), its name in err tone
    when that run failed with the prefix its fold's pipelines share drawn
    dim, and a time only when it failed, runs or waits (`railTime`); a
    hover gives its groups, name and latest run. A fold's summary counts
    what fails and runs under it; *Collapse all* / *Expand all* sets every
    fold as the reader's own. Up and Down step along the rows, Left and
    Right shut and open a fold. The selected one is drawn inverted and kept
    in view. The list is windowed (#2211) over its flattened rows
    (`listRows`): headings, pipelines and the summaries of folds, a fold's
    rows following its summary while it is open.
  - **Right**, the selected pipeline's page (#851, `TfPipeline.svelte`):
    header with *Pin* and, last, the view's *Sync now* (#2287; a bare
    *Builds* header holds it while no pipeline is drawn), history chart, and the Runs, Parameters and Triggers
    tabs.

  `#/builds` selects the first failing pipeline, else the first;
  `#/builds/<entity id>` selects that one. `#/build/<pipeline>/<run>` is the
  run page (#853, `TfRun.svelte`) with its pipeline selected on the left;
  `#/build/<run>` finds the run's pipeline and moves to that address.
-->
<script lang="ts" module>
  import { SvelteMap } from "svelte/reactivity";
  import TfKey from "../TfKey.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import { replaceAddress } from "../opener";

  import type { CommitRepository, JobPipeline, PipelineRow, RunRow, SourceView } from "../../ipc/entity";

  /** The reads and the write this page makes, injectable so a test needs no bridge. */
  export interface BuildsPorts {
    pipelines: () => Promise<PipelineRow[]>;
    runs: (pipelineId: string) => Promise<RunRow[]>;
    /** Pin or unpin (#849); the bridge's when omitted. */
    setPinned?: (pipelineId: string, pinned: boolean) => Promise<boolean>;
    /** The job's pipelines and their reasons, by the job's key (#850); the bridge's when omitted. */
    jobPipelines?: (job: string) => Promise<JobPipeline[]>;
    /** A run's whole view, for the pipeline a bare run address names (#853); the bridge's when omitted. */
    view?: (runId: string) => Promise<SourceView | null>;
    /** The repositories of the list's commits, for *By repository* (the repo-builds pass); the bridge's when omitted. */
    commitRepositories?: (commitIds: string[]) => Promise<CommitRepository[]>;
  }

  /** The folds the reader opened or closed this session, by key (R-9.3). */
  const remembered = new SvelteMap<string, boolean>();

  /** Forget the folds: a new session, which is what each test is. */
  export function forgetFolds(): void {
    remembered.clear();
  }
</script>

<script lang="ts">
  import TfScope from "../filter/TfScope.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import TfRailBar from "../TfRailBar.svelte";
  import TfScroller from "../TfScroller.svelte";
  import TfBareHeader from "../TfBareHeader.svelte";
  import { flushSync, tick, untrack } from "svelte";
  import type { RoleFrameProps } from "../page-entry";

  import {
    commitRepositories,
    getEntity,
    jobPipelines as readJobPipelines,
    pipelineRuns,
    pipelines as readPipelines,
    setPipelinePinned,
    type PipelineReason,
  } from "../../ipc/entity";
  import { listSources } from "../../ipc/sources";
  import { resolveTicket } from "../job/job";
  import { Windowed } from "../../shell/windowed.svelte";
  import { sessionFilter, sessionValue } from "../filter/filter-state.svelte";
  import type { EscTarget } from "../escape";
  import TfCrumbs, { type Crumb } from "../TfCrumbs.svelte";
  import TfPipeline from "./TfPipeline.svelte";
  import TfRun from "./TfRun.svelte";
  import TfRunGlyph from "./TfRunGlyph.svelte";
  import { pipelineOfRun } from "./run";
  import { RUN_BRIDGE } from "./pipeline-ports";
  import {
    CHIPS,
    foldHolds,
    foldOpen,
    groupsOf,
    recentNewestFirst,
    isFailing,
    keptPipelines,
    latestTime,
    listRows,
    pipelineHash,
    pipelineOf,
    pinnedOf,
    railGlyph,
    railTime,
    reasonsById,
    reasonsLine,
    repoTreeOf,
    runHash,
    treeOf,
    type Fold,
    type Scope,
  } from "./builds";

  let {
    ready,
    head,
    rest,
    job,
    jobAt = null,
    back = null,
    ports = { pipelines: readPipelines, runs: (id) => pipelineRuns(id) },
    sync,
    syncRevision = 0,
  }: {
    /** Whether the database answers yet: the reads wait for it. */
    ready: boolean;
    /** `builds` for the list, `build` for a run's address. */
    head: string;
    /** What follows the head in the address. */
    rest: string[];
    /** The chosen job (spec #2227), for *This job*. */
    job: string | null;
    /** The chosen job's entity id (#1525), which its ticket is read by; its key when omitted. */
    jobAt?: string | null;
    /** Where `Esc` goes, which the breadcrumb's hint names; `null` where it does nothing (#2228). */
    back?: EscTarget | null;
    ports?: BuildsPorts;
  } & RoleFrameProps = $props();

  const filter = sessionFilter("builds-pipelines");
  const lastPipeline = sessionValue("builds-last-pipeline");
  const chips = $derived(filter.chips["state"] ?? []);

  /**
   * The pipelines as `pipelines` answers them: the read is CI's, so the mirror
   * narrows it to the CI role's fillers and the page drops nothing (#1295,
   * ADR-0029 as amended on 2026-10-19).
   */
  let all = $state.raw<PipelineRow[] | null>(null);
  let failed = $state<string | null>(null);

  $effect(() => {
    if (!ready || all !== null) return;
    ports.pipelines().then(
      (rows) => (all = rows.map(recentNewestFirst)),
      (error: unknown) => (failed = message(error)),
    );
  });

  /** Bumped on every read again of the list, so an older one answering late is dropped. */
  let listReads = 0;
  /**
   * The list read again, kept drawn meanwhile: when *Sync now*'s runs end
   * (#2106), and on the pipeline page's *Refresh* (#2137), whose row of the
   * pipeline is among it. A failed read keeps the list and says so, until a
   * read again answers.
   */
  async function rereadPipelines() {
    const at = ++listReads;
    try {
      const rows = await ports.pipelines();
      if (at !== listReads) return;
      all = rows.map(recentNewestFirst);
      failed = null;
    } catch (error) {
      if (at === listReads) failed = message(error);
    }
  }

  let pipelinesAt = untrack(() => syncRevision);
  $effect(() => {
    const at = syncRevision;
    if (!ready || at === pipelinesAt) return;
    pipelinesAt = at;
    void untrack(() => rereadPipelines());
  });

  // One default on every page (design system, Layout): *All*. A ticket card's row turns *This job* on.
  const scope = $derived<Scope>(job !== null && filter.scope === "job" ? "job" : "all");
  let reasons = $state<Map<string, PipelineReason[]> | null>(null);
  /** What the job's ticket is read by: the address it was opened at (#1537), else its key. */
  const jobRef = $derived(job === null ? null : (jobAt ?? job));
  $effect(() => {
    const key = jobRef;
    reasons = null;
    if (!ready || key === null) return;
    const read = ports.jobPipelines ?? jobPipelinesByKey;
    read(key).then(
      (rows) => {
        if (jobRef === key) reasons = reasonsById(rows);
      },
      // A job whose reasons cannot be read keeps nothing: *This job* says so
      // through its empty state, and *All* still lists everything.
      () => {
        if (jobRef === key) reasons = new Map();
      },
    );
  });
  /** The job's pipelines, whatever the scope: a bare `#/builds` with nothing remembered opens the job's first. */
  const jobsOwn = $derived(all === null || reasons === null ? [] : all.filter((p) => reasons!.has(p.entity_id)));
  /** The pipelines *This job* narrows the list to, before the words and chips narrow them. */
  const onJob = $derived(scope === "job" ? jobsOwn : []);
  const kept = $derived(
    all === null ? [] : keptPipelines(scope === "job" ? onJob : all, chips),
  );
  /** The pipeline on the right: the one the list picks, or on a run's address the run's own. */
  function selectedOf(list: PipelineRow[]): PipelineRow | null {
    if (head !== "builds") return list.find((p) => p.entity_id === rest[0]) ?? null;
    // A bare `#/builds` is the pipeline you were on: going back to Builds never changes the page under you.
    const had = rest.length === 0 && lastPipeline.value !== null && list.find((p) => p.entity_id === lastPipeline.value);
    // The pipeline a bare `#/builds` drew stays while it is listed: picked as the first failing, it stopped failing
    // when a sync brought a passing run, and the page swapped to another under the pointer (the still-rows pass).
    const drew = rest.length === 0 && bareDrew !== null && list.find((p) => p.entity_id === bareDrew);
    return had || drew || pipelineOf(list, rest, jobsOwn);
  }
  /** What a bare `#/builds` drew, for as long as the page lives. */
  let bareDrew = $state<string | null>(null);
  /**
   * A bare `#/builds` with a job and nothing remembered opens the job's first pipeline, which only its reasons know: until
   * they are read the page draws no pipeline, so it never draws one pipeline's runs and swaps to another's under the
   * pointer (the still-rows pass, 2026-11-11).
   */
  const deciding = $derived(
    all !== null && head === "builds" && rest.length === 0 && job !== null && reasons === null &&
      !(lastPipeline.value !== null && all.some((p) => p.entity_id === lastPipeline.value)),
  );
  const selected = $derived(all === null || deciding ? null : selectedOf(all));
  $effect(() => {
    if (selected !== null && (rest.length > 0 || head !== "builds")) lastPipeline.value = selected.entity_id;
    if (selected !== null && rest.length === 0 && head === "builds" && bareDrew !== selected.entity_id) bareDrew = selected.entity_id;
  });
  /** The page you are on where the rail's filter leaves it out; `null` while the job's reasons are still being read. */
  const outside = $derived(
    selected === null || (scope === "job" && reasons === null) || kept.some((p) => p.entity_id === selected.entity_id) ? null : selected,
  );

  // `#/build/<run id>` (a job's or the inbox's row) names the run alone: its
  // pipeline is the one whose recent runs hold it, else the one its view names.
  /** A bare run address no pipeline was found for. */
  let lost = $state<string | null>(null);
  $effect(() => {
    if (all === null || head !== "build" || rest.length !== 1) return;
    // A run's address that names a pipeline and no run is the pipeline's page.
    if (selected !== null) return replaceAddress(pipelineHash(selected.entity_id));
    const id = rest[0]!;
    const owner = all.find((p) => p.recent.some((r) => r.entity_id === id));
    if (owner !== undefined) {
      replaceAddress(runHash(owner.entity_id, id));
      return;
    }
    const ids = all.map((p) => p.entity_id);
    void (ports.view ?? RUN_BRIDGE.view)(id).then(
      (view) => {
        const found = pipelineOfRun(view, ids);
        if (found !== null && rest[0] === id) replaceAddress(runHash(found, id));
        else lost = id;
      },
      () => (lost = id),
    );
  });
  const pinned = $derived(scope === "job" ? [] : pinnedOf(kept));

  /**
   * *By project* or *By repository* (the repo-builds pass): the source's own tree, or a fold for each repository the
   * pipeline's runs built. A run names its commit (`commit_id`) already, so the repositories are one keyed read of
   * those commits, made only while *By repository* is on and kept by the answer cache.
   */
  const grouping = sessionValue("builds-group");
  const byRepository = $derived(grouping.value === "repository");
  /** Each commit the list's runs name, by its repositories; `null` until the read answered. */
  let commitRepos = $state.raw<Map<string, { id: string; title: string }[]> | null>(null);
  /** Why the read failed: said over the list, with *Try again*, since every pipeline then sits under *No repository*. */
  let commitReposFailed = $state<string | null>(null);
  /** Bumped by *Try again*: the same commits are asked again. */
  let commitsRetry = $state(0);
  let commitsAsked = "";
  $effect(() => {
    void commitsRetry;
    if (!byRepository || all === null) return;
    const ids = [...new Set(all.flatMap((p) => p.recent.flatMap((run) => (run.commit_id === null ? [] : [run.commit_id]))))].sort();
    const asked = JSON.stringify(ids);
    if (asked === commitsAsked) return;
    commitsAsked = asked;
    untrack(() => ports.commitRepositories ?? commitRepositories)(ids).then(
      (rows) => {
        if (asked !== commitsAsked) return;
        const map = new Map<string, { id: string; title: string }[]>();
        for (const row of rows) map.set(row.commit_id, [...(map.get(row.commit_id) ?? []), { id: row.repo_id, title: row.repo_title }]);
        commitRepos = map;
        commitReposFailed = null;
      },
      // A read that fails groups nothing by repository: every pipeline is under *No repository*, and the list says why.
      (error: unknown) => {
        if (asked !== commitsAsked) return;
        commitRepos = new Map();
        commitReposFailed = message(error);
        commitsAsked = "";
      },
    );
  });
  /** The repositories a pipeline's recent runs built, each once. */
  function reposOf(pipeline: PipelineRow): { id: string; title: string }[] {
    const seen = new Map<string, { id: string; title: string }>();
    for (const run of pipeline.recent) for (const repo of (run.commit_id === null ? null : commitRepos?.get(run.commit_id)) ?? []) seen.set(repo.id, repo);
    return [...seen.values()];
  }
  const trees = $derived(byRepository ? (commitRepos === null ? [] : repoTreeOf(kept, reposOf)) : treeOf(kept));
  /** Whether anything narrows the list, which opens every fold. */
  const narrowed = $derived(chips.length > 0 || scope === "job");
  /**
   * A fold the reader has not opened or shut keeps the state it first drew in (the still-rows pass, 2026-11-11): one
   * opens by itself while a pipeline in it fails, and a sync bringing a pass shut it, or a failure opened it, and the
   * rail's rows moved under the pointer. A changed filter decides again; the open pipeline's fold still opens.
   */
  let foldsDrew = new Map<string, boolean>();
  let foldsBasis = "";
  /** What the list draws, flattened: the windowed list's rows. */
  const rows = $derived.by(() => {
    const basis = JSON.stringify([narrowed, scope, chips]);
    if (basis !== foldsBasis) {
      foldsDrew = new Map();
      foldsBasis = basis;
    }
    const at = selected?.entity_id ?? null;
    return listRows(pinned, trees, scope, (f) => {
      const own = remembered.get(f.key);
      if (own !== undefined) return own;
      if (!foldsDrew.has(f.key)) foldsDrew.set(f.key, foldOpen(undefined, f, at, narrowed));
      return foldsDrew.get(f.key)! || foldHolds(f, at);
    });
  });
  // A pipeline or a fold is one 26 px line; headings are measured once drawn. From 40 rows, not the default 300: the
  // scale estate's 75 rows drawn whole were a third of Builds' first open, a 194 ms task at 4x with the runs beside them
  // (the Windows pass, 2026-11-14), as the Inbox windows from 40.
  const win = new Windowed({ count: () => rows.length, key: (i) => rows[i]!.key, estimate: 26, from: 40, overscan: 300 });
  /** Whether any drawn fold is open: the tools' button then shuts them all, else opens them all. */
  const anyOpen = $derived(rows.some((row) => row.kind === "fold" && row.open));

  /** The job's key resolved to its ticket, then its pipelines (#850). */
  async function jobPipelinesByKey(key: string) {
    const sources = await listSources();
    const ticket = await resolveTicket(key, sources, getEntity);
    return ticket === null ? [] : readJobPipelines(ticket.row.entity_id);
  }

  function setScope(next: Scope) {
    filter.scope = next;
  }

  async function togglePin(pipeline: PipelineRow) {
    const write = ports.setPinned ?? setPipelinePinned;
    const now = await write(pipeline.entity_id, !pipeline.pinned);
    if (all !== null) all = all.map((p) => (p.entity_id === pipeline.entity_id ? { ...p, pinned: now } : p));
  }

  /** A row's indent class: 14px a level, as prototype-6's, up to {@link DEEPEST}. */
  const DEEPEST = 8;
  function indent(depth: number): string {
    return `tf-d${Math.min(depth, DEEPEST)}`;
  }

  function rememberFold(f: Fold, event: Event) {
    const open = (event.currentTarget as HTMLDetailsElement).open;
    // Only the reader's own toggle is remembered, not the one the open
    // attribute raises when the fold is drawn.
    if (open !== foldOpen(remembered.get(f.key), f, selected?.entity_id ?? null, narrowed)) remembered.set(f.key, open);
  }

  /**
   * *Collapse all* / *Expand all*: every fold, as if the reader had toggled each. The unfiltered tree's too, so a fold
   * a chip or *This job* hides is set when it comes back (a filter can join single children another way).
   */
  function setAllFolds(open: boolean) {
    const walk = (folds: readonly Fold[]) => {
      for (const f of folds) {
        remembered.set(f.key, open);
        walk(f.folds);
      }
    };
    for (const tree of [...trees, ...treeOf(all ?? [])]) walk(tree.folds);
  }

  /** A drawn row's depth, read off its indent class. */
  function depthOf(row: HTMLElement): number {
    return Number(/\btf-d(\d)\b/.exec(row.className)?.[1] ?? 0);
  }

  /**
   * The rail's keys, the docs tree's table (`docs/tree-keys.ts`, F-444) over the rows drawn: Up and Down step, Home and
   * End go to the ends, Right opens a fold and then steps into it, Left shuts it, and from a pipeline goes to its fold.
   * A fold opened or shut so is the reader's own toggle.
   */
  function railKeys(event: KeyboardEvent) {
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || list === undefined) return;
    const row = event.target instanceof HTMLElement ? event.target : null;
    if (row === null || !row.matches(".tf-pl-rows .tf-tr")) return;
    const drawn = [...list.querySelectorAll<HTMLElement>(".tf-pl-rows .tf-tr")];
    const at = drawn.indexOf(row);
    const fold = row.dataset["fold"];
    const open = fold !== undefined && (row.parentElement as HTMLDetailsElement | null)?.open === true;
    let next: HTMLElement | undefined;
    switch (event.key) {
      case "ArrowDown":
        next = drawn[at + 1];
        break;
      case "ArrowUp":
        next = drawn[at - 1];
        break;
      case "Home":
        next = drawn[0];
        break;
      case "End":
        next = drawn.at(-1);
        break;
      case "ArrowRight":
        if (fold !== undefined && !open) remembered.set(fold, true);
        else if (open && drawn[at + 1] !== undefined && depthOf(drawn[at + 1]!) > depthOf(row)) next = drawn[at + 1];
        break;
      case "ArrowLeft":
        if (fold !== undefined && open) remembered.set(fold, false);
        else next = drawn.slice(0, at).reverse().find((above) => above.dataset["fold"] !== undefined && depthOf(above) < depthOf(row));
        break;
      default:
        return;
    }
    event.preventDefault();
    next?.focus();
    next?.scrollIntoView?.({ block: "nearest" });
  }

  function message(error: unknown): string {
    if (error && typeof error === "object" && "message" in error) return String(error.message);
    return String(error);
  }

  let list: HTMLElement | undefined = $state();

  function toggle(chip: string) {
    filter.chips["state"] = chips.includes(chip) ? chips.filter((c) => c !== chip) : [...chips, chip];
  }

  // The selected pipeline stays in view as the list scrolls on its own (R-9.4a).
  $effect(() => {
    if (selected === null || list === undefined) return;
    const id = selected.entity_id;
    void tick().then(() => {
      // Drawn first if the window left it out.
      win.reveal(rows.findIndex((row) => row.kind === "pipeline" && row.pipeline.entity_id === id));
      flushSync();
      list?.querySelector('[aria-current="true"]')?.scrollIntoView?.({ block: "nearest" });
    });
  });


  /** Below 900 px, whether the pipelines rail is unfolded under its bar. */
  let railOpen = $state(false);
</script>

<!-- The header while no pipeline or run is drawn: the view's name and its *Sync now* (#2287). -->
{#snippet bare()}
  <TfBareHeader title="Builds" {sync} />
{/snippet}

{#snippet crumbs(parts: Crumb[])}
  <TfCrumbs {parts} {back} />
{/snippet}

<!-- A pipeline's name, the prefix its fold's pipelines share drawn dim (`sharedPrefix`). -->
{#snippet name(pipeline: PipelineRow, pre: string)}
  <span class="tf-pi-nm" class:tf-bad={isFailing(pipeline)}
    >{#if pre !== ""}<span class="tf-pi-pre">{pre}</span>{/if}{pipeline.name.slice(pre.length)}</span
  >
{/snippet}

{#snippet item(pipeline: PipelineRow, depth: number, key: string, pre: string, tag: string | null)}
  {@const glyph = railGlyph(pipeline)}
  {@const time = railTime(pipeline)}
  {@const latest = latestTime(pipeline)}
  {@const why = scope === "job" ? reasonsLine(reasons?.get(pipeline.entity_id) ?? [], job ?? "") : ""}
  <a
    class={`tf-tr tf-pi ${indent(depth)}`}
    href={pipelineHash(pipeline.entity_id)}
    aria-current={pipeline.entity_id === selected?.entity_id}
    title={`${[...groupsOf(pipeline), pipeline.name].join(" › ")}\n${glyph.word}${latest !== "" ? ` ${latest}` : ""}`}
    {@attach win.row(key)}
  >
    <TfRunGlyph {...glyph} />
    {@render name(pipeline, pre)}
    {#if tag !== null}<span class="tf-pi-src">{tag}</span>{/if}
    {#if why !== ""}<span class="tf-pi-why"><TfKeyed text={why} /></span>{/if}
    {#if time !== null}<span class={`tf-pi-when${time.tone !== null ? ` tf-${time.tone}` : ""}`}>{time.text}</span>{/if}
  </a>
{/snippet}

{#snippet foldView(f: Fold, depth: number, open: boolean, key: string)}
  <!-- The summary alone: a fold's rows follow it while it is open, and a closed one has none (#2200's class, #2211). -->
  <details class="tf-pl-fold" {open} ontoggle={(event) => rememberFold(f, event)} {@attach win.row(key)}>
    <summary class={`tf-tr tf-tfold ${indent(depth)}`} data-fold={f.key}
      ><span class="tf-fold-nm">{f.name}</span><span class="tf-fold-at"
        >{#if f.failing > 0}<span class="tf-count tf-err">{f.failing}<span class="tf-sr">{" failing"}</span></span>{/if}{#if f.running > 0}<span
            class="tf-count tf-info">{f.running}<span class="tf-sr">{" running"}</span></span
          >{/if}<span class="tf-fold-n">{f.count}<span class="tf-sr">{f.count === 1 ? " pipeline" : " pipelines"}</span></span></span
      ></summary
    >
  </details>
{/snippet}

<div class="tf-bv" class:tf-rail-shut={!railOpen}>
  <TfRailBar label="Pipeline" value={selected?.name ?? "All"} open={railOpen} controls="tf-pipelines-rail" onpress={() => (railOpen = !railOpen)} onclose={() => (railOpen = false)} />
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <nav
    id="tf-pipelines-rail"
    class="tf-pl tf-rail"
    aria-label="Pipelines"
    onclick={(event) => {
      if ((event.target as Element).closest("a[href]")) railOpen = false;
    }}
    onkeydown={railKeys}
  >
    <div class="tf-pl-top">
      <div class="tf-pl-group">
        {#if job !== null}
          <TfScope
            scopes={[{ id: "job", label: "This job", count: reasons === null ? null : jobsOwn.length, title: `The pipelines ${job} reaches` }, { id: "all", label: "All" }]}
            {scope}
            onpick={(id) => setScope(id as Scope)}
          />
        {/if}
        <div class="tf-seg" role="group" aria-label="Group by">
          <button type="button" aria-pressed={!byRepository} onclick={() => (grouping.value = null)}>By project</button>
          <button type="button" aria-pressed={byRepository} onclick={() => (grouping.value = "repository")}>By repository</button>
        </div>
      </div>
      <div class="tf-pl-tools">
        <div class="tf-fchips" role="group" aria-label="Show only">
          {#each CHIPS as chip (chip)}
            <button type="button" aria-pressed={chips.includes(chip)} onclick={() => toggle(chip)}>{chip}</button>
          {/each}
        </div>
        {#if rows.some((row) => row.kind === "fold")}
          <div class="tf-trail">
            <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => setAllFolds(!anyOpen)}>{anyOpen ? "Collapse all" : "Expand all"}</button>
          </div>
        {/if}
      </div>
    </div>
    <div class="tf-pl-list" bind:this={list}>
      {#if failed !== null}
        <p class="tf-pl-note">Pipelines could not be read: {failed}</p>
      {:else if all !== null && all.length === 0}
        <p class="tf-pl-note">No pipelines yet. A CI source's pipelines show here once it has synced.</p>
      {:else if all !== null && scope === "job" && reasons !== null}
        <p class="tf-scope-note">
          {onJob.length} of {all.length} pipelines are relevant to <span class="tf-mono">{job}</span>{#if kept.length !== onJob.length}, {kept.length} shown{/if}.
          <button type="button" class="tf-linkish" onclick={() => setScope("all")}>Show all</button>
        </p>
      {/if}
      {#if all !== null && all.length > 0 && kept.length === 0 && (scope === "all" || reasons !== null)}
        <p class="tf-pl-note">
          No pipeline matches.
          {#if scope === "job"}<button type="button" class="tf-linkish" onclick={() => setScope("all")}>Search all pipelines</button>{/if}
        </p>
      {/if}
      {#if outside !== null}
        <!-- The filter hides the page you are on, so the rail would show no place at all: it stays, until you leave it. -->
        <div class="tf-pl-pin">
          <a class="tf-tr tf-pi tf-d0" href={pipelineHash(outside.entity_id)} aria-current="true" title={outside.name}>
            <TfRunGlyph {...railGlyph(outside)} />
            {@render name(outside, "")}
            <span class="tf-pi-why">{#if scope === "job" && !jobsOwn.some((p) => p.entity_id === outside.entity_id)}Not in <TfKey value={job ?? ""} />{:else}Hidden by the filter{/if}</span>
          </a>
        </div>
      {/if}
      {#if byRepository && commitRepos === null && all !== null && all.length > 0}
        <p class="tf-pl-note">Reading which repositories each pipeline builds…</p>
      {:else if byRepository && commitReposFailed !== null}
        <p class="tf-pl-note">
          Which repositories the pipelines build could not be read: {commitReposFailed}.
          <button type="button" class="tf-linkish" onclick={() => (commitsRetry += 1)}>Try again</button>
        </p>
      {/if}
      <div class="tf-pl-rows" {@attach win.list}>
        {#each win.slice(rows) as row (row.key)}
          {#if row.kind === "head"}
            <h3 {@attach win.row(row.key)}>{row.text}</h3>
          {:else if row.kind === "pipeline"}
            {@render item(row.pipeline, row.depth, row.key, row.pre, row.tag)}
          {:else}
            {@render foldView(row.fold, row.depth, row.open, row.key)}
          {/if}
        {/each}
      </div>
    </div>
  </nav>
  <TfScroller class="tf-bmain">
    {#if head === "build" && selected !== null && rest.length >= 2}
      <TfRun pipeline={selected} {rest} {job} runs={ports.runs} {crumbs} {sync} />
    {:else if head === "build"}
      {@render crumbs([{ name: "Builds", href: "#/builds" }, { name: "Run" }])}
      {@render bare()}
      {#if all !== null && (rest.length !== 1 || lost === rest[0])}
        <TfNothingHere message="knobas does not hold this run. It may not have synced yet." href="#/builds" action="All pipelines" />
      {/if}
    {:else if deciding}
      {@render crumbs([{ name: "Builds" }])}
      {@render bare()}
    {:else if selected !== null}
      <!-- One crumb at both of a pipeline's addresses: `#/builds` draws this page already, so nothing above it links. -->
      {@render crumbs([{ name: "Builds" }, ...(selected.group !== null ? [{ name: selected.group }] : []), { name: selected.name }])}
      <TfPipeline
        pipeline={selected}
        {rest}
        {job}
        runs={ports.runs}
        onpin={() => togglePin(selected!)}
        rereadRow={rereadPipelines}
        {sync}
      />
    {:else if all !== null && all.length > 0}
      {@render crumbs([{ name: "Builds" }, { name: "Pipeline" }])}
      {@render bare()}
      <TfNothingHere message="knobas does not hold this pipeline. It may not have synced yet." href="#/builds" action="All pipelines" />
    {:else}
      {@render crumbs([{ name: "Builds" }])}
      {@render bare()}
    {/if}
  </TfScroller>
</div>
