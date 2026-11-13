<!--
  A repository's Builds tab (the repo-builds pass, `mockups/builds/repo-builds.md`): the pipelines that built the
  repository's commits, branch by branch. The page reads them once (`repo_pipelines`, `TfRepo.svelte`) and hands the
  answer here, to the tab's count and to the Branches tab's *Builds* column.

  - **Left**, the branches as the Builds rail draws its rows (`builds.css`): the default branch, yours (the checkout's,
    those naming the job), those an open pull request is opened from, then every other, each by its newest run. Each
    is the glyph of its pipelines' worst newest run, its name with its namespace dim, and *N failing* or its pull
    request. A filter by name, *Failing* and *Mine*. Rows keep the place they first drew.
  - **Right**, the branch shown (`builds/<branch>`; yours, else the default) against the default branch: what fails
    only on it, as the run page's problem box with its log, then a table of each pipeline's newest run here beside
    the default branch's, and in a word what that means. The pipelines that never ran on it are one line under it.
  - **A pipeline** (`builds/<branch>/<pipeline>`): its runs on the default branch or on every branch of the
    repository, without leaving it; *Open in Builds* for its whole page.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { ipcErrorMessage } from "../../ipc";
  import { commitRepositories, pipelineRuns, type CommitRepository, type RepoBuilds, type RepoPipelineRow, type RunRow } from "../../ipc/entity";
  import TfKey from "../TfKey.svelte";
  import TfStateChip from "../TfStateChip.svelte";
  import { filterKeys } from "../filter-keys";
  import TfNothingHere from "../TfNothingHere.svelte";
  import TfRunGlyph from "../builds/TfRunGlyph.svelte";
  import { pipelineHash, runAt, runGlyph, runHash, runTone, shortTime, stateWord, testsWords, toneWord } from "../builds/builds";
  import { branchName } from "../builds/pipeline";
  import { secondsLabel } from "../../time/format";
  import { PROBLEMS_SHOWN, branchWords, compareOn, groupBranches, holdOrder, latestOn, railOf, type RailBranch, type Verdict } from "./repo-builds";
  import { buildsHash, repoHash, type BranchOption } from "./repo";
  import { itemHash } from "../job/job";
  import { Windowed } from "../../shell/windowed.svelte";

  let {
    repoId,
    branches,
    defaultBranch,
    mine,
    job,
    branch,
    pipeline = null,
    builds,
    failed = null,
    runs = (id: string) => pipelineRuns(id),
    commitsOf = (ids: string[]) => commitRepositories(ids),
    refreshed = 0,
  }: {
    repoId: string;
    /** The repository's branches, as the page's branch list reads them; `null` until it answered. */
    branches: readonly BranchOption[] | null;
    defaultBranch: string | null;
    /** Your branches (`yoursOf`): the rail's *Yours*, and the one the tab opens on. */
    mine: readonly string[];
    /** The chosen job's key, whose chip marks your branch. */
    job: string | null;
    /** The branch the address names (`builds/<branch>`), else `null`. */
    branch: string | null;
    /** The pipeline the address names (`builds/<branch>/<pipeline>`), whose runs open here. */
    pipeline?: string | null;
    /** The page's answer (`repo_pipelines`); `null` until it answered. */
    builds: RepoBuilds | null;
    /** Why the page's read failed, while it has no answer to draw. */
    failed?: string | null;
    /** A pipeline's runs, injectable so a test needs no bridge. */
    runs?: ((pipelineId: string) => Promise<RunRow[]>) | undefined;
    /** The repositories of the runs' commits, injectable so a test needs no bridge. */
    commitsOf?: ((commitIds: string[]) => Promise<CommitRepository[]>) | undefined;
    /** Bumped by the page's *Refresh* and a return to the tab: the pipeline's runs are read again over what is drawn. */
    refreshed?: number;
  } = $props();

  const rows = $derived(builds?.pipelines ?? null);
  /** The branches an open pull request is opened from, and each one's key (`#142`). */
  const pulls = $derived(new Map((builds?.pull_requests ?? []).map((pull) => [pull.branch, pull])));

  /** Whose answer the held places are: another repository's are not this one's. */
  let heldFor: string | null = null;
  $effect(() => {
    if (heldFor === repoId) return;
    heldFor = repoId;
    firstDraw = null;
    railPlaces.clear();
    tablePlaces.clear();
  });

  /** A branch this session is still making has run nothing yet, and is not listed. */
  const listed = $derived((branches ?? []).filter((b) => b.made === undefined).map((b) => b.name));
  /** The default branch the list names; while it names none, its first, so no branch is made up. */
  const def = $derived(defaultBranch ?? listed[0] ?? "");
  /** The tab draws once the reads that decide its rows have answered: the builds and the branch list. */
  const ready = $derived(rows !== null && branches !== null);
  const rail = $derived(rows === null ? [] : railOf(rows, listed));

  /**
   * What the tab first drew with, held for the visit (rows stay where they drew, 2026-11-11): your branches, the
   * branches a pull request was open from, and the branch it opened on when the address names none. A checkout read or
   * a new pull request that lands later moves no branch from one group to another.
   */
  let firstDraw = $state<{ mine: readonly string[]; pulled: readonly string[]; opened: string } | null>(null);
  $effect(() => {
    if (!ready || firstDraw !== null) return;
    const yours = mine;
    firstDraw = { mine: yours, pulled: [...pulls.keys()], opened: yours.find((name) => rail.some((b) => b.name === name)) ?? def };
  });
  const heldMine = $derived(firstDraw?.mine ?? mine);
  const heldPulls = $derived(firstDraw?.pulled ?? [...pulls.keys()]);
  /** Each branch's place in the rail and each pipeline's in a branch's table, from the first time they drew. */
  const railPlaces = new Map<string, number>();
  const tablePlaces = new Map<string, Map<string, number>>();

  let text = $state("");
  let onlyFailing = $state(false);
  let onlyMine = $state(false);
  const kept = $derived(
    rail.filter(
      (b) =>
        (!onlyFailing || b.failing > 0) &&
        (!onlyMine || heldMine.includes(b.name)) &&
        b.name.toLowerCase().includes(text.trim().toLowerCase()),
    ),
  );
  const groups = $derived(
    groupBranches(kept, def, heldMine, heldPulls).map((group) => ({
      ...group,
      branches: holdOrder(railPlaces, group.branches, (b) => b.name),
    })),
  );
  /**
   * The rail as the rows it draws, each group's heading before its branches: a repository of thousands of branches
   * draws only the rows near the rail's scroll (the extremes pass, 2026-11-13), as the Builds page's own rail does.
   */
  const railRows = $derived(
    groups.flatMap((group): ({ key: string; head: string } | { key: string; branch: RailBranch })[] => [
      { key: `h:${group.head}`, head: group.head },
      ...group.branches.map((branch) => ({ key: `b:${branch.name}`, branch })),
    ]),
  );
  const railWin = new Windowed({ count: () => railRows.length, key: (i) => railRows[i]!.key, estimate: 30 });

  /** The branch shown: the address's, else the one the tab opened on. */
  const current = $derived(branch ?? firstDraw?.opened ?? def);
  const isDefault = $derived(current === def);
  const page = $derived.by(() => {
    if (rows === null) return null;
    const compared = compareOn(rows, current, def);
    let places = tablePlaces.get(current);
    if (places === undefined) tablePlaces.set(current, (places = new Map()));
    return { ...compared, ran: holdOrder(places, compared.ran, (x) => x.row.entity_id) };
  });
  const only = $derived(page?.ran.filter((x) => x.verdict === "only") ?? []);
  const busy = $derived(page?.ran.filter((x) => x.verdict === "running" || x.verdict === "queued").length ?? 0);
  const also = $derived(page?.ran.filter((x) => x.verdict === "also").length ?? 0);

  function verdictWord(verdict: Verdict): { text: string; tone: string } {
    switch (verdict) {
      case "only":
        return isDefault ? { text: "Failing", tone: "tf-err" } : { text: "Only on this branch", tone: "tf-err" };
      case "running":
        return { text: "Running", tone: "tf-info" };
      case "queued":
        return { text: "Queued", tone: "" };
      case "also":
        return { text: `Fails on ${def} too`, tone: "" };
      case "fixed":
        return { text: "Fixed on this branch", tone: "tf-ok" };
      case "pass":
        return { text: "", tone: "" };
    }
  }

  function when(run: RunRow): string {
    const at = runAt(run);
    return at === null ? "" : shortTime(at);
  }

  function branchHref(name: string): string {
    return buildsHash(repoId, name);
  }

  /** A pipeline's runs inside the tab, at the branch shown. */
  function pipelineHref(pipelineId: string): string {
    return buildsHash(repoId, current, pipelineId);
  }

  /** The pipeline shown inside the tab: its row, and its runs as `pipeline_runs` answers them. */
  const shownPipeline = $derived(pipeline === null ? null : (rows?.find((row) => row.entity_id === pipeline) ?? null));
  let pipelineRows = $state.raw<RunRow[] | null>(null);
  let pipelineFailed = $state<string | null>(null);
  let runsRead = 0;
  /** Each run's commit by the repository it is in: a run is this repository's when its commit is (`commit_repositories`). */
  let runRepos = $state.raw<Map<string, string> | null>(null);
  let runsFor: string | null = null;
  $effect(() => {
    const id = pipeline;
    void refreshed;
    // Another pipeline's runs are not drawn under this one's name; a read again of the same keeps them meanwhile.
    if (runsFor !== id) {
      runsFor = id;
      pipelineRows = null;
      pipelineFailed = null;
      runRepos = null;
    }
    if (id === null) return;
    const at = ++runsRead;
    untrack(() => runs(id))
      .then(async (answer) => {
        const commits = [...new Set(answer.flatMap((run) => (run.commit_id === null ? [] : [run.commit_id])))];
        const named = commits.length === 0 ? [] : await untrack(() => commitsOf(commits));
        if (at !== runsRead) return;
        runRepos = new Map(named.map((c) => [c.commit_id, c.repo_id]));
        pipelineRows = answer;
      })
      .catch((error: unknown) => {
        if (at === runsRead) pipelineFailed = ipcErrorMessage(error);
      });
  });
  /** Which runs: the default branch's, or every branch's of this repository. */
  let runScope = $state<"main" | "every">("every");
  const branchNames = $derived(new Set(rail.map((b) => b.name)));
  /**
   * The runs this repository's: by their commit's repository where the mirror holds the commit (a pipeline that builds
   * two repositories has two `main`s), else by a branch name this repository has.
   */
  /** A pipeline's runs on a busy repository are thousands: drawn near the view (the extremes pass, 2026-11-13). */
  const runsWin = new Windowed({ count: () => shownRuns.length, key: (i) => shownRuns[i]!.entity_id, estimate: 37 });
  const shownRuns = $derived(
    (pipelineRows ?? []).filter((run) => {
      const name = run.branch === null ? null : branchName(run.branch);
      if (name === null || (runScope === "main" && name !== def)) return false;
      const repo = run.commit_id === null ? undefined : runRepos?.get(run.commit_id);
      return repo !== undefined ? repo === repoId : branchNames.has(name);
    }),
  );
</script>

{#snippet cell(row: RepoPipelineRow, name: string)}
  {@const run = latestOn(row, name)}
  {#if run === null}
    <span class="tf-rb-none" title="Never ran on {name}">Not run</span>
  {:else}
    {@const tone = runTone(run)}
    <a
      class="tf-rbt-cell"
      class:tf-err={tone === "err"}
      class:tf-info={tone === "run"}
      href={runHash(row.entity_id, run.entity_id)}
      title="{row.name} on {name}: {runGlyph(run).word}, {when(run)}"
      ><TfRunGlyph {...runGlyph(run)} />{run.number !== null ? `#${run.number} ` : ""}{when(run)}</a
    >
  {/if}
{/snippet}

{#if failed !== null && rows === null}
  <p class="tf-dim">Builds could not be read: {failed}</p>
{:else if rows !== null && rows.length === 0}
  <TfNothingHere
    message="No pipeline has built a commit of this repository yet. Its builds show here once a CI source has synced runs of its commits."
    href="#/builds"
    action="Every pipeline"
  />
{:else if !ready}
  <p class="tf-dim">Reading builds…</p>
{:else if rows !== null && page !== null}
  <div class="tf-rbv">
    <nav class="tf-pl tf-rbrail" aria-label="Branches">
      <div class="tf-pl-top">
        <label class="tf-fsearch"
          ><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
            ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
          ><input placeholder="Filter branches" aria-label="Filter branches" bind:value={text} {@attach filterKeys()} /></label
        >
        <div class="tf-pl-tools">
          <div class="tf-fchips" role="group" aria-label="Show only">
            <button type="button" aria-pressed={onlyFailing} onclick={() => (onlyFailing = !onlyFailing)}>Failing</button>
            <button type="button" aria-pressed={onlyMine} onclick={() => (onlyMine = !onlyMine)}>Mine</button>
          </div>
          <span class="tf-rb-count">{kept.length === rail.length ? `${rail.length} branches` : `${kept.length} of ${rail.length}`}</span>
        </div>
      </div>
      <div class="tf-pl-list">
        <div class="tf-pl-rows" {@attach railWin.list}>
          {#each railWin.slice(railRows) as item (item.key)}
            {#if "head" in item}
            <h3 {@attach railWin.row(item.key)}>{item.head}</h3>
            {:else}
              {@const b = item.branch}
              {@const slash = b.name.lastIndexOf("/")}
              <a class="tf-tr tf-pi tf-d0 tf-rbr" href={branchHref(b.name)} aria-current={b.name === current} title="{b.name}: {branchWords(b)}" {@attach railWin.row(item.key)}>
                <TfRunGlyph tone={b.tone} word={toneWord(b.tone)} />
                <span class="tf-pi-nm" class:tf-bad={b.failing > 0}
                  >{#if slash > 0}<span class="tf-pi-pre">{b.name.slice(0, slash + 1)}</span>{/if}{b.name.slice(slash + 1)}</span
                >
                {#if b.failing > 0}<span class="tf-pi-when tf-err">{b.failing} failing</span>{:else if b.tone === "run"}<span
                    class="tf-pi-when tf-info">running</span
                  >{:else if pulls.get(b.name)?.cell}<span class="tf-pi-when">{pulls.get(b.name)?.cell}</span>{/if}
              </a>
            {/if}
          {:else}
            <p class="tf-pl-note">No branch matches.</p>
          {/each}
        </div>
      </div>
    </nav>

    {#if pipeline !== null}
      <section class="tf-rbp" aria-label="Runs of {shownPipeline?.name ?? pipeline}">
        <p class="tf-rb-back"><a href={branchHref(current)}>Builds of <span class="tf-mono">{current}</span></a></p>
        <div class="tf-rbp-h">
          <h2 class="tf-rbp-pl">{shownPipeline?.name ?? pipeline}</h2>
          <div class="tf-trail"><a class="tf-btn tf-sm tf-ghost" href={pipelineHash(pipeline)}>Open in Builds</a></div>
        </div>
        <p class="tf-rbp-sub">
          {#if shownPipeline !== null}{[shownPipeline.source_name, ...shownPipeline.path].join(" › ")}.{/if} Its runs on this repository's branches.
        </p>
        <div class="tf-seg" role="group" aria-label="Runs on">
          <button type="button" aria-pressed={runScope === "main"} onclick={() => (runScope = "main")}>{def}</button>
          <button type="button" aria-pressed={runScope === "every"} onclick={() => (runScope = "every")}>Every branch</button>
        </div>
        {#if pipelineFailed !== null}
          <p class="tf-dim">Runs could not be read: {pipelineFailed}</p>
        {:else if pipelineRows === null}
          <p class="tf-dim">Reading runs…</p>
        {:else if shownRuns.length === 0}
          <p class="tf-rb-notrun">No run of it on {runScope === "main" ? def : "a branch of this repository"} yet.</p>
        {:else}
          <table class="tf-tbl tf-rbt tf-rbruns">
            <colgroup><col class="tf-rbt-no" /><col /><col class="tf-rbt-run" /><col class="tf-rbt-dur" /><col class="tf-rbt-run" /></colgroup>
            <thead><tr><th>Run</th><th>Branch</th><th>Tests</th><th>Duration</th><th>When</th></tr></thead>
            <tbody {@attach runsWin.list}>
              {#if runsWin.before > 0}<tr class="tf-wgap" aria-hidden="true"><td colspan="5" {@attach runsWin.room("before")}></td></tr>{/if}
              {#each runsWin.slice(shownRuns) as run (run.entity_id)}
                {@const name = run.branch === null ? "" : branchName(run.branch)}
                <tr {@attach runsWin.row(run.entity_id)}>
                  <td class="tf-nowrap"
                    ><a href={runHash(pipeline, run.entity_id)}>{run.number !== null ? `#${run.number}` : ""}</a>
                    <TfStateChip text={runTone(run) === "q" ? "Queued" : stateWord(run.state?.text ?? "")} tone={run.state?.tone ?? null} /></td
                  >
                  <td class="tf-mono tf-rbt-br"
                    >{name}{#if job !== null && heldMine.includes(name)}{" "}<TfKey chip value={job} class="tf-jobchip-sm" />{/if}</td
                  >
                  <td class="tf-nowrap">{run.tests !== null ? testsWords(run.tests) : ""}</td>
                  <td class="tf-nowrap tf-num">{run.duration_seconds !== null ? secondsLabel(run.duration_seconds) : ""}</td>
                  <td class="tf-nowrap tf-dim tf-num">{when(run)}</td>
                </tr>
              {/each}
              {#if runsWin.after > 0}<tr class="tf-wgap" aria-hidden="true"><td colspan="5" {@attach runsWin.room("after")}></td></tr>{/if}
            </tbody>
          </table>
        {/if}
      </section>
    {:else}
    <section class="tf-rbp" aria-label="Builds of {current}">
      <div class="tf-rbp-h">
        <h2>{current}</h2>
        {#if job !== null && heldMine.includes(current)}<TfKey chip value={job} class="tf-jobchip-sm" />{/if}
        {#if pulls.get(current) !== undefined}{@const pull = pulls.get(current)!}<a class="tf-chip tf-info" href={itemHash({ entity_id: pull.entity_id, kind: "pr" })}
            >{pull.cell ?? "Pull request"} open</a
          >{/if}
        {#if !isDefault}
          <div class="tf-trail">
            <a class="tf-btn tf-sm tf-ghost" href={repoHash(repoId, "compare", encodeURIComponent(current))}>Compare with {def}</a>
          </div>
        {/if}
      </div>
      <p class="tf-rbp-sub">{isDefault ? "The default branch. Every other branch here is compared with it." : `Compared with ${def}.`}</p>

      <!-- The verdict, the one loud thing on the page: a failure this branch alone has is a problem, as the run page draws one. -->
      {#each only.slice(0, PROBLEMS_SHOWN) as { row } (row.entity_id)}
        {@const run = latestOn(row, current)!}
        {@const there = latestOn(row, def)}
        <div class="tf-problem">
          <h3>{row.name} {isDefault ? `fails on ${def}` : there === null ? `fails here and has not run on ${def}` : `fails here and passes on ${def}`}</h3>
          <p class="tf-dim">{run.tests !== null ? `${testsWords(run.tests)} in ` : ""}{run.number !== null ? `#${run.number}, ` : ""}{when(run)}.</p>
          <div class="tf-acts">
            <a class="tf-btn tf-sm" href="{runHash(row.entity_id, run.entity_id)}/log">Log</a>
            <a class="tf-btn tf-sm" href={runHash(row.entity_id, run.entity_id)}>Open run</a>
            <a class="tf-btn tf-sm tf-ghost" href={pipelineHref(row.entity_id)}>All its runs</a>
          </div>
        </div>
      {:else}
        <p class="tf-rbp-ok">
          <TfRunGlyph {...busy > 0 ? { tone: "run" as const, word: "Running" } : { tone: "ok" as const, word: "Passed" }} />
          <span
            >{#if isDefault}Every pipeline that ran on {def} passed{busy > 0 ? `, and ${busy} still running` : ""}.{:else}Nothing fails here
              that passes on {def}{also > 0 ? `. ${also} ${also === 1 ? "pipeline fails" : "pipelines fail"} on ${def} as well` : ""}{busy > 0
                ? `. ${busy} still running`
                : ""}.{/if}</span
          >
        </p>
      {/each}
      {#if only.length > PROBLEMS_SHOWN}<p class="tf-rbp-sub">{only.length - PROBLEMS_SHOWN} more fail only here; they lead the table.</p>{/if}

      {#if page.ran.length > 0}
        <table class="tf-tbl tf-rbt">
          <colgroup><col /><col class="tf-rbt-run" />{#if !isDefault}<col class="tf-rbt-run" />{/if}<col class="tf-rbt-vc" /></colgroup>
          <thead>
            <tr>
              <th>Pipeline</th>
              <th>{isDefault ? def : "This branch"}</th>
              {#if !isDefault}<th>{def}</th>{/if}
              <th><span class="tf-sr">Verdict</span></th>
            </tr>
          </thead>
          <tbody>
            {#each page.ran as { row, verdict } (row.entity_id)}
              {@const said = verdictWord(verdict)}
              {@const here = latestOn(row, current)!}
              <tr>
                <td class="tf-rbt-pl"
                  ><a href={pipelineHref(row.entity_id)} title={[...row.path, row.name].join(" › ")}
                    ><TfRunGlyph {...runGlyph(here)} /><span class="tf-pi-nm" class:tf-bad={runTone(here) === "err"}>{row.name}</span></a
                  ></td
                >
                <td>{@render cell(row, current)}</td>
                {#if !isDefault}<td>{@render cell(row, def)}</td>{/if}
                <td class="tf-rbt-v {said.tone}">{said.text}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
      {#if page.notRun.length > 0}
        <p class="tf-rb-notrun">
          Not run on this branch: {#each page.notRun as row, at (row.entity_id)}{#if at > 0},{" "}{/if}<a href={pipelineHref(row.entity_id)}
              >{row.name}</a
            >{/each}.
        </p>
      {/if}
      <p class="tf-rb-notrun">
        {rows.length}
        {rows.length === 1 ? "pipeline builds" : "pipelines build"} this repository. <a href="#/builds">Every pipeline in Builds</a>
      </p>
    </section>
    {/if}
  </div>
{/if}
