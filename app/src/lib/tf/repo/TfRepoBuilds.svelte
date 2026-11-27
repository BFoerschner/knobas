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
  import TfSideEdge from "../TfSideEdge.svelte";
  import { sideWidth } from "../side-width.svelte";
  import { untrack } from "svelte";
  import { ipcErrorMessage } from "../../ipc";
  import { commitRepositories, pipelineRuns, type CommitRepository, type RepoBuilds, type RepoPipelineRow, type RunRow } from "../../ipc/entity";
  import TfKey from "../TfKey.svelte";
  import TfStateChip from "../TfStateChip.svelte";
  import { filterKeys } from "../filter-keys";
  import Marked from "../filter/Marked.svelte";
  import { heldOrder } from "../filter/ranked.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import TfRunGlyph from "../builds/TfRunGlyph.svelte";
  import { pipelineHash, runAt, runGlyph, runHash, runTone, shortTime, stateWord, testsWords, toneWord } from "../builds/builds";
  import { branchName } from "../builds/pipeline";
  import { secondsLabel } from "../../time/format";
  import { PROBLEMS_SHOWN, branchWords, compareOn, groupBranches, holdOrder, latestOn, railOf, type RailBranch, type Verdict } from "./repo-builds";
  import { buildsHash, repoHash, type BranchOption } from "./repo";
  import { itemHash } from "../job/job";
  import { Windowed } from "../../shell/windowed.svelte";
  import { EVENTS } from "../../ipc";
  import { cacheEvents } from "../../ipc/answer-cache";
  import { windowListen } from "../../ipc/shared-listen";
  import { refreshEntity, type SourceSyncStatus } from "../../ipc/sources";
  import type { EchoListen } from "../queued.svelte";
  import { IN_FLIGHT_AFTER_MS, IN_FLIGHT_AGAIN_MS, InFlightReads, hearRuns, inFlight, runsInFlight } from "../builds/runs-in-flight";
  import { pageVisible } from "../kept-pages.svelte";
  import { onFocusWhileShown } from "../while-shown.svelte";
  import { onRefreshed } from "./refreshed.svelte";

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
    shown = true,
    listen = (event, handler) => windowListen(event, handler),
    follow = (runId: string) => refreshEntity(runId, true),
    walk = (pipelineId: string) => refreshEntity(pipelineId),
    again = () => {},
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
    /**
     * Bumped by the page's *Refresh* (⌘R) and a return to the tab: the pipeline's runs are read again over what is drawn,
     * and the runs the tab draws in flight are read at their source first (#2558).
     */
    refreshed?: number;
    /** Whether the tab is the page's shown one; a held tab hears nothing and reads nothing at a source. */
    shown?: boolean;
    /** Hears `item:refreshed` and `sync:state` (#2558); the bridge's when left out. */
    listen?: EchoListen | undefined;
    /** A run in flight read at its source by id (`refresh_entity`, behind); the bridge's when left out. */
    follow?: ((runId: string) => Promise<unknown>) | undefined;
    /**
     * ⌘R's read of one of its pipelines with its newest runs at its source (#2564, ADR-0047 as amended 2026-11-27):
     * `refresh_entity`, the person's lane; the bridge's when left out.
     */
    walk?: ((pipelineId: string) => Promise<unknown>) | undefined;
    /** The page's answer (`builds`) read again over what is drawn: a run it draws moved (#2558). */
    again?: (() => Promise<unknown> | void) | undefined;
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
  const toggled = $derived(rail.filter((b) => (!onlyFailing || b.failing > 0) && (!onlyMine || heldMine.includes(b.name))));
  /**
   * The typed words decide through the app's ranking which branches keep (#2532); each keeps the place it first drew
   * in its group (`holdOrder`), so the rail's order is the groups', not the ranking's.
   */
  const railName = (b: RailBranch) => b.name;
  const order = heldOrder<RailBranch>("repo-builds-branches");
  $effect(() => order.ask(text, toggled, railName));
  const kept = $derived(order.rows(text, toggled, railName));
  const marking = $derived(order.typed(text, toggled));
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
  // A branch's row draws 26 px and a group's heading 33 (measured on the extreme repository, the WSL block, 2026-11-14).
  const railWin = new Windowed({
    count: () => railRows.length,
    key: (i) => railRows[i]!.key,
    estimate: 26,
    estimateOf: (key) => (key.startsWith("h:") ? 33 : 26),
  });

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

  /**
   * The run page reads its pipeline's runs before it draws (`pipeline_runs`), and from here nothing has read them yet: the
   * Builds page holds them, this tab only each pipeline's newest runs. So the tab reads them ahead, through the answer
   * cache, so a press on a run opens its page from memory (2026-11-16): the table's first pipelines, one in each of the
   * window's idle moments, and any run the pointer or focus reaches at once. A new answer or a *Refresh* reads them again.
   */
  const AHEAD = 12;
  let readAhead = new Set<string>();
  function ahead(pipelineId: string) {
    if (readAhead.has(pipelineId)) return;
    const asked = readAhead;
    asked.add(pipelineId);
    void untrack(() => runs(pipelineId)).catch(() => asked.delete(pipelineId));
  }
  $effect(() => {
    void builds;
    void refreshed;
    readAhead = new Set();
  });
  $effect(() => {
    const ids = (page?.ran ?? []).slice(0, AHEAD).map((x) => x.row.entity_id);
    void refreshed;
    if (ids.length === 0) return;
    let left = true;
    const idle = (then: () => void): void => {
      if (typeof requestIdleCallback === "function") requestIdleCallback(then, { timeout: 2000 });
      else setTimeout(then, 200);
    };
    const next = (at: number) => {
      if (!left || at >= ids.length) return;
      ahead(ids[at]!);
      idle(() => next(at + 1));
    };
    idle(() => next(0));
    return () => {
      left = false;
    };
  });

  /**
   * A press anywhere on a run's row is a press on its link, the run's number (TfPipeline's run rows, R-9.6): the
   * same event, so a modified press or another button does what it does on the link. A link or button in the row does
   * its own, and a selection of the row's words is not a press.
   */
  function openRow(event: MouseEvent & { currentTarget: HTMLTableRowElement }) {
    if ((event.target as Element | null)?.closest("a, button, input, select, label")) return;
    if ((window.getSelection()?.toString() ?? "") !== "") return;
    event.currentTarget.querySelector("a")?.dispatchEvent(new MouseEvent("click", event));
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
    void readPipeline(id);
  });
  /** The open pipeline's runs read again over what is drawn; a newer read wins. */
  function readPipeline(id: string): Promise<void> {
    const at = ++runsRead;
    return untrack(() => runs(id))
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
  }

  /**
   * **A run in flight moves on here as on the Builds page** (#2558, `builds/runs-in-flight.ts`, the TeamCity run fix's
   * mechanism): the tab hears a refresh that moved a run it draws (a run started in knobas is read to its end, or this
   * tab read it) and a run of one of its pipelines' sources that brought news (a scheduled sync, *Sync now* on Builds),
   * and reads its answer again over what it drew, at once and then at most once in `IN_FLIGHT_AGAIN_MS`. Once drawn,
   * shown again, or the window back, it reads the runs it draws in flight at their source. ⌘R, before the page's
   * *Refreshing…* ends, first reads each of the repository's pipelines with its newest runs at its source (#2564: a
   * TeamCity build configuration's builds above the watermark, so a run started there since the last run shows), then
   * its runs in flight, then its answers again; it syncs no source. While held hidden it hears nothing: the return
   * reads it again.
   */
  const visible = pageVisible();
  const live = () => shown && visible();
  /** Every run the tab draws: each pipeline's newest per branch, and the open pipeline's runs. */
  const drawnRuns = $derived([...(rows ?? []).flatMap((row) => row.latest), ...(pipelineRows ?? [])]);
  const drawnIds = $derived(new Set(drawnRuns.map((run) => run.entity_id)));
  /** The runs it draws in flight, newest first, as `runsInFlight` takes them: the few in flight sorted, never the thousands drawn. */
  function drawnInFlight(): RunRow[] {
    const open = new Map(untrack(() => drawnRuns).filter(inFlight).map((run) => [run.entity_id, run]));
    return [...open.values()].sort((a, b) => (runAt(b) ?? "").localeCompare(runAt(a) ?? ""));
  }
  const drawnSources = $derived(new Set((rows ?? []).map((row) => row.source_id)));
  /** The tab's answers again: the page's, and the open pipeline's runs. */
  function rereadDrawn(): Promise<unknown> {
    const id = pipeline;
    return Promise.allSettled([Promise.resolve(untrack(again)), id === null ? null : readPipeline(id)]);
  }
  let movedAt = 0;
  let movedSoon: ReturnType<typeof setTimeout> | undefined;
  function moved(): void {
    if (movedSoon !== undefined) return;
    movedSoon = setTimeout(
      () => {
        movedSoon = undefined;
        movedAt = Date.now();
        void rereadDrawn();
      },
      Math.max(0, movedAt + IN_FLIGHT_AGAIN_MS - Date.now()),
    );
  }
  $effect(() => {
    const ears = untrack(() => listen);
    const offRuns = hearRuns(ears, (entity) => live() && drawnIds.has(entity), moved);
    let dead = false;
    let offSync: (() => void) | null = null;
    void ears(EVENTS.syncState, ({ payload }) => {
      const row = payload as SourceSyncStatus;
      if (dead || !live() || row.running || row.last_changed !== true || !drawnSources.has(row.source_id)) return;
      // The answers the run moved go before the read again (`hearRuns`' reason).
      cacheEvents.syncState(row);
      moved();
    })
      .then((stop) => (dead ? stop() : (offSync = stop)))
      .catch(() => {});
    return () => {
      dead = true;
      offRuns();
      offSync?.();
      clearTimeout(movedSoon);
      movedSoon = undefined;
    };
  });
  const inFlightReads = new InFlightReads(untrack(() => follow));
  const readInFlight = () => inFlightReads.read(runsInFlight(drawnInFlight()));
  $effect(() => {
    void repoId;
    inFlightReads.reset();
  });
  $effect(() => {
    if (!ready || !live()) return;
    const soon = setTimeout(readInFlight, IN_FLIGHT_AFTER_MS);
    return () => clearTimeout(soon);
  });
  onFocusWhileShown(live, readInFlight);
  /**
   * The repository's pipelines read at their sources (#2564): one source's one at a time, since each refresh takes its
   * source's lock only when free and two at once would leave the second unread; sources side by side. A pipeline its
   * source does not read this way answers *not offered*, and nothing runs for it. A walk held off (a run of its source
   * in flight, or a read of a run in flight holding the lock) or failed says nothing: the page's own *Refreshed* stands,
   * and a run in flight writes those builds anyway, which the tab hears and reads again.
   */
  function walkPipelines(): Promise<unknown> {
    const ask = untrack(() => walk);
    const bySource = new Map<string, string[]>();
    for (const row of untrack(() => rows) ?? []) bySource.set(row.source_id, [...(bySource.get(row.source_id) ?? []), row.entity_id]);
    return Promise.allSettled(
      [...bySource.values()].map(async (ids) => {
        for (const id of ids) await Promise.resolve(id).then(ask).catch(() => {});
      }),
    );
  }
  onRefreshed(
    () => refreshed,
    async () => {
      await walkPipelines();
      inFlightReads.reset();
      await readInFlight();
      await rereadDrawn();
    },
  );
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
    <!-- The run, as a link to it on Builds that fills its cell (Björn, 2026-11-16: "click on the run … and open the build itself"). -->
    <a
      class="tf-rbt-cell"
      class:tf-err={tone === "err"}
      class:tf-info={tone === "run"}
      href={runHash(row.entity_id, run.entity_id)}
      onpointerenter={() => ahead(row.entity_id)}
      onfocus={() => ahead(row.entity_id)}
      title="Open {row.name} {run.number !== null ? `#${run.number}` : "run"} on {name}: {runGlyph(run).word}, {when(run)}"
      ><TfRunGlyph {...runGlyph(run)} /><b class="tf-rbt-num">{run.number !== null ? `#${run.number}` : "queued"}</b><span class="tf-rbt-at">{when(run)}</span></a
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
  <div class="tf-rbv" {@attach sideWidth("repo-builds")}>
    <TfSideEdge side="repo-builds" />
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
                  >{#if slash > 0}<span class="tf-pi-pre"><Marked text={b.name.slice(0, slash + 1)} typed={marking} pairs={order.marks(b, 0, slash + 1)} /></span>{/if}<Marked
                    text={b.name.slice(slash + 1)}
                    typed={marking}
                    pairs={order.marks(b, slash + 1)}
                  /></span
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
                <tr class="tf-row" onclick={openRow} {@attach runsWin.row(run.entity_id)}>
                  <td class="tf-nowrap"
                    ><a href={runHash(pipeline, run.entity_id)}><b>{run.number !== null ? `#${run.number}` : "queued"}</b></a>
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
                <td class="tf-rbt-runc">{@render cell(row, current)}</td>
                {#if !isDefault}<td class="tf-rbt-runc">{@render cell(row, def)}</td>{/if}
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
