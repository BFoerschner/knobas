<!--
  A repository's Builds tab (the repo-builds pass, `mockups/builds/repo-builds.md`): the pipelines that built the
  repository's commits (`repo_pipelines`, one statement over the mirror, kept by the answer cache), branch by branch.

  - **Left**, the branches as the Builds rail draws its rows (`builds.css`): the default branch, yours (the checkout's,
    those naming the job), then every other by its newest run. Each is the glyph of its pipelines' worst newest run,
    its name with its namespace dim, and *N failing* where something failed. A filter by name and a *Failing* chip.
  - **Right**, the branch shown (`builds/<branch>`; yours, else the default) against the default branch: what fails
    only on it, as the run page's problem box with its log, then a table of each pipeline's newest run here beside
    the default branch's, and in a word what that means. The pipelines that never ran on it are one line under it.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { ipcErrorMessage } from "../../ipc";
  import { repoPipelines, type RepoPipelineRow, type RunRow } from "../../ipc/entity";
  import TfKey from "../TfKey.svelte";
  import { filterKeys } from "../filter-keys";
  import TfNothingHere from "../TfNothingHere.svelte";
  import TfRunGlyph from "../builds/TfRunGlyph.svelte";
  import { pipelineHash, runAt, runGlyph, runHash, runTone, shortTime, testsWords, toneWord } from "../builds/builds";
  import { compareOn, groupBranches, holdOrder, latestOn, railOf, type RailBranch, type Verdict } from "./repo-builds";
  import { repoHash, type BranchOption } from "./repo";

  let {
    repoId,
    branches,
    defaultBranch,
    mine,
    job,
    branch,
    refreshed = 0,
    read = repoPipelines,
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
    /** Bumped by the page's *Refresh* and a return to the tab: reads again, keeping what it drew meanwhile. */
    refreshed?: number;
    /** The read, injectable so a test needs no bridge. */
    read?: ((repoId: string) => Promise<RepoPipelineRow[]>) | undefined;
  } = $props();

  let rows = $state.raw<RepoPipelineRow[] | null>(null);
  let failed = $state<string | null>(null);
  /** Bumped by every read, so an older one answering late is dropped. */
  let reads = 0;
  let readFor: string | null = null;
  $effect(() => {
    const id = repoId;
    void refreshed;
    const at = ++reads;
    // Another repository's rows, error and held places are not this one's; a read again of the same keeps them meanwhile.
    if (readFor !== id) {
      rows = null;
      failed = null;
      readFor = id;
      firstDraw = null;
      railPlaces.clear();
      tablePlaces.clear();
    }
    untrack(() => read(id)).then(
      (answer) => {
        if (at !== reads) return;
        rows = answer;
        failed = null;
      },
      (error: unknown) => {
        if (at === reads) failed = ipcErrorMessage(error);
      },
    );
  });

  /** A branch this session is still making has run nothing yet, and is not listed. */
  const listed = $derived((branches ?? []).filter((b) => b.made === undefined).map((b) => b.name));
  /** The default branch the list names; while it names none, its first, so no branch is made up. */
  const def = $derived(defaultBranch ?? listed[0] ?? "");
  /** The tab draws once the reads that decide its rows have answered: the builds and the branch list. */
  const ready = $derived(rows !== null && branches !== null);
  const rail = $derived(rows === null ? [] : railOf(rows, listed));

  /**
   * What the tab first drew with, held for the visit (rows stay where they drew, 2026-11-11): your branches, and the
   * branch it opened on when the address names none. A checkout read that lands later moves no branch into *Yours*.
   */
  let firstDraw = $state<{ mine: readonly string[]; opened: string } | null>(null);
  $effect(() => {
    if (!ready || firstDraw !== null) return;
    const yours = mine;
    firstDraw = { mine: yours, opened: yours.find((name) => rail.some((b) => b.name === name)) ?? def };
  });
  const heldMine = $derived(firstDraw?.mine ?? mine);
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
    groupBranches(kept, def, heldMine).map((group) => ({ ...group, branches: holdOrder(railPlaces, group.branches, (b) => b.name) })),
  );
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

  /** A branch in words, for its title: how many failed, else its state. */
  function branchWord(b: RailBranch): string {
    if (b.failing > 0) return `${b.failing} failing`;
    return b.tone === "ok" ? "All passed" : toneWord(b.tone);
  }

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
    return repoHash(repoId, "builds", encodeURIComponent(name));
  }
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
        <div class="tf-pl-rows">
          {#each groups as group (group.head)}
            <h3>{group.head}</h3>
            {#each group.branches as b (b.name)}
              {@const slash = b.name.lastIndexOf("/")}
              <a class="tf-tr tf-pi tf-d0 tf-rbr" href={branchHref(b.name)} aria-current={b.name === current} title="{b.name}: {branchWord(b)}">
                <TfRunGlyph tone={b.tone} word={toneWord(b.tone)} />
                <span class="tf-pi-nm" class:tf-bad={b.failing > 0}
                  >{#if slash > 0}<span class="tf-pi-pre">{b.name.slice(0, slash + 1)}</span>{/if}{b.name.slice(slash + 1)}</span
                >
                {#if b.failing > 0}<span class="tf-pi-when tf-err">{b.failing} failing</span>{:else if b.tone === "run"}<span
                    class="tf-pi-when tf-info">running</span
                  >{/if}
              </a>
            {/each}
          {:else}
            <p class="tf-pl-note">No branch matches.</p>
          {/each}
        </div>
      </div>
    </nav>

    <section class="tf-rbp" aria-label="Builds of {current}">
      <div class="tf-rbp-h">
        <h2>{current}</h2>
        {#if job !== null && heldMine.includes(current)}<TfKey chip value={job} class="tf-jobchip-sm" />{/if}
        {#if !isDefault}
          <div class="tf-trail">
            <a class="tf-btn tf-sm tf-ghost" href={repoHash(repoId, "compare", encodeURIComponent(current))}>Compare with {def}</a>
          </div>
        {/if}
      </div>
      <p class="tf-rbp-sub">{isDefault ? "The default branch. Every other branch here is compared with it." : `Compared with ${def}.`}</p>

      <!-- The verdict, the one loud thing on the page: a failure this branch alone has is a problem, as the run page draws one. -->
      {#each only.slice(0, 2) as { row } (row.entity_id)}
        {@const run = latestOn(row, current)!}
        {@const there = latestOn(row, def)}
        <div class="tf-problem">
          <h3>{row.name} {isDefault ? `fails on ${def}` : there === null ? `fails here and has not run on ${def}` : `fails here and passes on ${def}`}</h3>
          <p class="tf-dim">{run.tests !== null ? `${testsWords(run.tests)} in ` : ""}{run.number !== null ? `#${run.number}, ` : ""}{when(run)}.</p>
          <div class="tf-acts">
            <a class="tf-btn tf-sm" href="{runHash(row.entity_id, run.entity_id)}/log">Log</a>
            <a class="tf-btn tf-sm" href={runHash(row.entity_id, run.entity_id)}>Open run</a>
            <a class="tf-btn tf-sm tf-ghost" href={pipelineHash(row.entity_id)}>All its runs</a>
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
      {#if only.length > 2}<p class="tf-rbp-sub">{only.length - 2} more fail only here; they lead the table.</p>{/if}

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
                  ><a href={pipelineHash(row.entity_id)} title={[...row.path, row.name].join(" › ")}
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
          Not run on this branch: {#each page.notRun as row, at (row.entity_id)}{#if at > 0},{" "}{/if}<a href={pipelineHash(row.entity_id)}
              >{row.name}</a
            >{/each}.
        </p>
      {/if}
      <p class="tf-rb-notrun">
        {rows.length}
        {rows.length === 1 ? "pipeline builds" : "pipelines build"} this repository. <a href="#/builds">Every pipeline in Builds</a>
      </p>
    </section>
  </div>
{/if}
