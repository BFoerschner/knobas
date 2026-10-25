<!--
  Code (#837, R-8.1, R-8.2): `#/code` and `#/code/repos` draw *Repositories*,
  `#/code/prs` *Pull requests*. One page, as prototype-6's `renderCode(tab)`:
  the header with *New pull request*, the two tabs with their counts, the
  tab's own filter bar, then its rows.

  - **Repositories**: every mirrored repository, yours first (one of your pull
    requests or one asking your review is in it), then by last push. Each row:
    the name with the job's key when the job reaches it, its open pull
    requests, the local checkout (`entity_checkout`: branch and where it
    stands, or *not on this machine*) and the last push.
  - **Pull requests**: the attention read (`list_entities`, order
    `attention`, #712) groups the open ones as *Asking for your review*,
    *Yours* and *Open, others*; the recency read adds *Merged or closed*.
    Each row: the number, the title over its repository and author, the state
    chip, the job it belongs to (`job/open-jobs.ts`) and when it last moved.
    A merged or closed pull request of yours is under *Yours*.

  A repository's team and language and a pull request's author are
  `code_facts` (#966), read once for every row the two lists hold. The other
  reads are `list_entities`, `entity_checkout`, `list_sources`,
  `list_contexts`, `get_entity` and `job_reach`. Rows open
  `#/repo/<entity id>` and `#/pr/<entity id>`.
-->
<script lang="ts" module>
  import type { Snippet } from "svelte";
  import type { CheckoutView, CodeFacts, EntityFilter, EntityPage } from "../../ipc/entity";
  import { readAllRows } from "../repo/repo";
  import type { SourceSummary } from "../../ipc/sources";
  import type { SourceFacts } from "../job/job";
  import type { OpenJobPorts } from "../job/open-jobs";

  /** The reads this page makes, injectable so a test needs no bridge. */
  export interface CodePorts extends OpenJobPorts {
    listEntities: (filter: EntityFilter, limit: number, offset: number) => Promise<EntityPage>;
    entityCheckout: (entityId: string) => Promise<CheckoutView>;
    codeFacts: (entityIds: string[]) => Promise<CodeFacts[]>;
    /** The sources, for the host the header names. */
    listSources: () => Promise<(SourceFacts & Pick<SourceSummary, "base_url">)[]>;
  }
</script>

<script lang="ts">
  import { clockLabel } from "../../time/format";
  import { ipcErrorMessage } from "../../ipc";
  import { codeFacts, entityCheckout, getEntity, jobReach, listContexts, listEntities, type EntityRow } from "../../ipc/entity";
  import { listSources } from "../../ipc/sources";
  import { CODE_HOST } from "../roles.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig, FilterScope } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import { clones } from "../openin/clones.svelte";
  import TfOpenIn from "../openin/TfOpenIn.svelte";
  import { itemHash, keyOf } from "../job/job";
  import { jobIndex, readOpenJobs, type OpenJob } from "../job/open-jobs";
  import {
    checkoutLine,
    compareHash,
    factsById,
    GROUP_LABEL,
    GROUP_ORDER,
    headerLine,
    openCount,
    openPrsOf,
    PR_FLAGS,
    PR_STATES,
    prLines,
    REPO_FLAGS,
    repoChoices,
    repoTeams,
    visiblePrs,
    visibleRepos,
    type PrGroup,
    type PrLine,
  } from "./code";

  let {
    ready,
    rest,
    job,
    ports = {
      listEntities: (filter, limit, offset) => listEntities(filter, limit, offset),
      entityCheckout: (entityId) => entityCheckout(entityId),
      codeFacts: (entityIds) => codeFacts(entityIds),
      listSources: () => listSources(),
      listContexts: () => listContexts(),
      getEntity: (entityId) => getEntity(entityId),
      jobReach: (entityId) => jobReach(entityId),
    },
    now: fixedNow,
    sync,
    syncRevision = 0,
  }: {
    /** Whether the database answers yet: the reads wait for it. */
    ready: boolean;
    /** What follows `code` in the address: `repos`, `prs`, or nothing. */
    rest: string[];
    /** The job the reader was last in, as a ticket key. */
    job: string | null;
    ports?: CodePorts;
    /** Injectable clock, for the time column and *Idle > 5 days*. */
    now?: Date;
    /** The role frame's *Sync now* (#2106), drawn with the header's actions. */
    sync?: Snippet;
    /** Moves when that *Sync now*'s runs have all ended (#2106). */
    syncRevision?: number;
  } = $props();

  const now = $derived(fixedNow ?? new Date());
  const tab = $derived<"repos" | "prs">(rest[0] === "prs" ? "prs" : "repos");

  /** How many rows one read asks for; a room of more is read on with `offset`. */
  const LIMIT = 1000;
  /** How many `entity_checkout` reads run at once: each may run `git status`. */
  const CHECKOUT_LANES = 4;

  /**
   * Every read Code makes asks for the code host role, so the mirror answers
   * its fillers' rows alone and the page draws them as read; the kinds are
   * the page's pick within the role (#1298, ADR-0029 as amended on 2026-10-19).
   */
  const filterOf = (kinds: string[], order: EntityFilter["order"], reach: string | null = null): EntityFilter => ({
    role: CODE_HOST,
    sources: [],
    kinds,
    updated_within_days: null,
    order,
    context: null,
    project: null,
    include_deleted: false,
    reach,
  });

  /**
   * Every row of a room, read on with `offset` past one read's `LIMIT` until
   * `total` (#1825): the page's counts and its Open PRs column count what the
   * mirror holds, so a tail dropped unsaid would miscount them.
   */
  async function whole(filter: EntityFilter): Promise<EntityPage> {
    const first: { page?: EntityPage } = {};
    const rows = await readAllRows(async (offset) => {
      const page = await ports.listEntities(filter, LIMIT, offset);
      first.page ??= page;
      return page;
    });
    return { ...first.page!, rows };
  }

  let repos = $state<EntityPage | null>(null);
  let attention = $state<EntityPage | null>(null);
  let everyPr = $state<EntityPage | null>(null);
  let branches = $state<EntityRow[]>([]);
  let sources = $state<Pick<SourceSummary, "id" | "base_url">[]>([]);
  let failed = $state<string | null>(null);
  let checkouts = $state<Record<string, CheckoutView>>({});
  let jobs = $state<OpenJob[]>([]);
  let facts = $state<CodeFacts[]>([]);

  $effect(() => {
    if (!ready) return;
    // *Sync now*'s runs ended (#2106): the rows are read again.
    void syncRevision;
    let live = true;
    failed = null;
    checkouts = {};
    const fail = (error: unknown) => {
      if (live) failed = ipcErrorMessage(error);
    };
    whole(filterOf(["repo"], "updated_desc")).then((page) => {
      if (!live) return;
      repos = page;
      void readCheckouts(page.rows.map((row) => row.entity_id), () => live);
    }, fail);
    whole(filterOf(["pr"], "attention")).then((page) => {
      if (live) attention = page;
    }, fail);
    whole(filterOf(["pr"], "updated_desc")).then((page) => {
      if (live) everyPr = page;
    }, fail);
    ports.listSources().then(
      (rows) => {
        if (live) sources = rows;
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });

  $effect(() => {
    if (!ready) return;
    const current = job;
    let live = true;
    void readOpenJobs(ports, current).then(async (open) => {
      if (!live) return;
      jobs = open;
      const own = open[0];
      if (current === null || own === undefined || keyOf(own.id) !== current) {
        branches = [];
        return;
      }
      const page = await ports.listEntities(filterOf(["branch"], "updated_desc", own.id), 50, 0).catch(() => null);
      if (live) branches = page?.rows ?? [];
    });
    return () => {
      live = false;
    };
  });

  // Team, language and author (#966), once both lists have answered. A failed
  // read leaves the cells empty rather than failing the page.
  $effect(() => {
    if (repos === null || everyPr === null) return;
    const ids = [...repos.rows, ...everyPr.rows].map((row) => row.entity_id);
    let live = true;
    ports.codeFacts(ids).then(
      (answer) => {
        if (live) facts = answer;
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });

  // A clone that finished: its row reads its checkout again (R-8.25).
  let seenClones = clones.finished;
  $effect(() => {
    if (clones.finished === seenClones) return;
    seenClones = clones.finished;
    const ids = (repos?.rows ?? []).filter((row) => !checkouts[row.entity_id]?.path).map((row) => row.entity_id);
    void readCheckouts(ids, () => true);
  });

  /** Each repository's checkout, a few at a time; one that fails draws nothing. */
  async function readCheckouts(ids: string[], alive: () => boolean) {
    const queue = [...ids];
    const lane = async () => {
      for (let id = queue.shift(); id !== undefined && alive(); id = queue.shift()) {
        try {
          const view = await ports.entityCheckout(id);
          if (alive()) checkouts[id] = view;
        } catch {
          // An unreadable checkout leaves the cell empty rather than claiming one.
        }
      }
    };
    await Promise.all(Array.from({ length: CHECKOUT_LANES }, lane));
  }

  const index = $derived(jobIndex(jobs));
  const held = $derived((entityId: string) => job !== null && index.holds(job, { entity_id: entityId }));
  const known = $derived(factsById(facts));
  const lines = $derived(attention && everyPr ? prLines(attention, everyPr, now, known) : null);
  const teamsByRepo = $derived(repoTeams(repos?.rows ?? [], known));
  const choices = $derived(repoChoices(repos?.rows ?? [], known));
  const openCounts = $derived(openPrsOf(lines ?? []));
  const compare = $derived(compareHash(branches, repos?.rows ?? []));

  // -- the filter bars -------------------------------------------------------

  const repoFilter = sessionFilter("code-repos");
  const prFilter = sessionFilter("code-prs");
  // Prototype-6 opens both tabs on *All*.
  if (repoFilter.scope === null) repoFilter.scope = "all";
  if (prFilter.scope === null) prFilter.scope = "all";

  $effect(() => {
    if (job !== null) return;
    if (repoFilter.scope === "job") repoFilter.scope = "all";
    if (prFilter.scope === "job") prFilter.scope = "all";
  });

  const scopes = $derived<FilterScope[]>([
    ...(job !== null ? [{ id: "job", label: "This job", tag: job, query: "" }] : []),
    { id: "mine", label: "Mine", query: "" },
    { id: "all", label: "All", query: "" },
  ]);
  const options = (values: readonly string[]) => values.map((value) => ({ value, label: value }));
  const repoNames = $derived([...new Set((repos?.rows ?? []).map((row) => row.title))].sort());

  const repoConfig = $derived<FilterConfig>({
    id: "code-repos",
    placeholder:
      repos === null
        ? "Filter repositories by name, team or language"
        : `Filter ${repos.total} repositories by name, team or language`,
    noun: "repositories",
    one: "repository",
    scopes,
    chips: [{ key: "flag", label: "Flags", options: options(REPO_FLAGS) }],
    dropdowns: [
      { key: "team", label: "Team", options: options(choices.teams) },
      { key: "lang", label: "Language", options: options(choices.languages) },
    ],
  });
  const prConfig = $derived<FilterConfig>({
    id: "code-prs",
    placeholder: "Filter pull requests by title, repo, author or number",
    noun: "pull requests",
    one: "pull request",
    scopes,
    chips: [
      { key: "state", label: "State", options: options(PR_STATES) },
      { key: "flag", label: "Flags", options: options(PR_FLAGS) },
    ],
    dropdowns: [
      { key: "repo", label: "Repo", options: options(repoNames) },
      { key: "team", label: "Team", options: options(choices.teams) },
    ],
  });

  const scopeOf = (value: string | null) => (value === "job" || value === "mine" ? value : "all");

  const shownRepos = $derived(
    repos === null
      ? []
      : visibleRepos(repos.rows, lines ?? [], checkouts, known, {
          scope: scopeOf(repoFilter.scope),
          held,
          flags: repoFilter.chips["flag"] ?? [],
          team: repoFilter.dropdowns["team"] ?? "",
          language: repoFilter.dropdowns["lang"] ?? "",
          text: repoFilter.text,
        }),
  );
  const shownPrs = $derived(
    lines === null
      ? []
      : visiblePrs(
          lines,
          {
            scope: scopeOf(prFilter.scope),
            held,
            states: prFilter.chips["state"] ?? [],
            flags: prFilter.chips["flag"] ?? [],
            repo: prFilter.dropdowns["repo"] ?? "",
            team: prFilter.dropdowns["team"] ?? "",
            text: prFilter.text,
          },
          teamsByRepo,
        ),
  );

  /** The pull requests by group, in draw order; open ones with no group come first, unheaded. */
  const groups = $derived.by(() => {
    const out: { group: PrGroup | null; lines: PrLine[] }[] = [];
    const ungrouped = shownPrs.filter((line) => line.group === null);
    if (ungrouped.length > 0) out.push({ group: null, lines: ungrouped });
    for (const group of GROUP_ORDER) {
      const kept = shownPrs.filter((line) => line.group === group);
      if (kept.length > 0) out.push({ group, lines: kept });
    }
    return out;
  });

  const prTotal = $derived(lines?.length ?? null);
  const filter = $derived(tab === "prs" ? prFilter : repoFilter);
  const searchAll = () => {
    filter.scope = "all";
  };

  /** A row opens its item; a click on the link inside already did, and *Open in…* and its dialogs are not the row. */
  function open(event: MouseEvent, href: string) {
    if ((event.target as Element | null)?.closest("a, .tf-openin, .tf-modal")) return;
    location.hash = href;
  }
</script>

<header class="tf-fhead">
  <div>
    <h1>Code</h1>
    <div class="tf-sub">
      {repos === null ? "Repositories and pull requests, what asks for you first." : headerLine(repos.total, repos.rows, sources)}
    </div>
  </div>
  <div class="tf-acts">
    {@render sync?.()}
    {#if compare !== null}
      <a class="tf-btn" href={compare}>New pull request</a>
    {:else}
      <!-- Focusable, so the reason in its title reaches the keyboard too. -->
      <button
        type="button"
        class="tf-btn"
        aria-disabled="true"
        title={job === null ? "Open a job to propose its branch" : `${job} has no branch yet`}>New pull request</button
      >
    {/if}
  </div>
</header>
<nav class="tf-tabs" aria-label="Code">
  <a href="#/code/repos" aria-current={tab === "repos" ? "page" : undefined}
    >Repositories{#if repos !== null && repos.total > 0}{" "}<span class="tf-count">{repos.total}</span>{/if}</a
  >
  <a href="#/code/prs" aria-current={tab === "prs" ? "page" : undefined}
    >Pull requests{#if prTotal !== null && prTotal > 0}{" "}<span class="tf-count">{prTotal}</span>{/if}</a
  >
</nav>
{#if tab === "repos"}
  <FilterBar
    config={repoConfig}
    filter={repoFilter}
    shown={repos === null ? null : shownRepos.length}
    total={repos?.total ?? null}
  />
{:else}
  <FilterBar config={prConfig} filter={prFilter} shown={lines === null ? null : shownPrs.length} total={prTotal} />
{/if}
<div class="tf-cbody">
  {#if failed !== null}
    <p class="tf-cfail">{failed}</p>
  {/if}
  {#if tab === "repos"}
    {#if shownRepos.length > 0}
      <table class="tf-tbl">
        <thead>
          <tr
            ><th>Repository</th><th>Team</th><th>Language</th><th>Open PRs</th><th>Local checkout</th><th>Last push</th><th
              ><span class="tf-sr">Open or clone</span></th
            ></tr
          >
        </thead>
        <tbody>
          {#each shownRepos as row (row.entity_id)}
            {@const count = openCount(openCounts, row)}
            {@const local = checkoutLine(checkouts[row.entity_id])}
            {@const fact = known.get(row.entity_id)}
            <tr class="tf-row" onclick={(event) => open(event, itemHash(row))}>
              <td
                ><a class="tf-mono" href={itemHash(row)}><Marked text={row.title} typed={repoFilter.text} /></a
                >{#if job !== null && held(row.entity_id)}{" "}<span class="tf-chip tf-chip-job">{job}</span>{/if}</td
              >
              <td><Marked text={fact?.teams.join(", ") ?? ""} typed={repoFilter.text} /></td>
              <td><Marked text={fact?.language ?? ""} typed={repoFilter.text} /></td>
              <td>{#if count > 0}{count}{:else}<span class="tf-dim">0</span>{/if}</td>
              <td class:tf-dim={local?.dim}>{local?.text ?? ""}</td>
              <td class="tf-dim tf-nowrap">{row.updated_at ? clockLabel(row.updated_at, now) : ""}</td>
              <td class="tf-nowrap"><TfOpenIn entityId={row.entity_id} title={row.title} checkout={checkouts[row.entity_id]} variant="ghost" /></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else if repos !== null}
      {@render nothing()}
    {/if}
  {:else}
    {#each groups as { group, lines: kept } (group ?? "open")}
      {#if group !== null}
        <h2 class="tf-cgroup-h">{GROUP_LABEL[group]} <span class="tf-count">{kept.length}</span></h2>
      {/if}
      <table class="tf-tbl tf-cprs">
        <tbody>
          {#each kept as { row, number, idle, author } (row.entity_id)}
            {@const belongs = index.jobOf({ entity_id: row.entity_id, kind: row.kind })}
            <tr class="tf-row" onclick={(event) => open(event, itemHash(row))}>
              <td class="tf-mono"><Marked text={number ?? ""} typed={prFilter.text} /></td>
              <td
                ><a href={itemHash(row)}><Marked text={row.title} typed={prFilter.text} /></a><span class="tf-sub"
                  >{#if row.path}<span class="tf-mono"><Marked text={row.path} typed={prFilter.text} /></span
                    >{/if}{#if author}{row.path ? ", " : ""}<Marked text={author} typed={prFilter.text} />{/if}{#if idle}{row.path ||
                    author
                      ? ", "
                      : ""}idle for more than 5 days{/if}</span
                ></td
              >
              <td>{#if row.state}<span class="tf-chip tf-{row.state.tone}">{row.state.text}</span>{/if}</td>
              <td class="tf-mono tf-dim">{belongs ?? ""}</td>
              <td class="tf-dim tf-nowrap">{row.updated_at ? clockLabel(row.updated_at, now) : ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      {#if lines !== null}
        {@render nothing()}
      {/if}
    {/each}
  {/if}
</div>

{#snippet nothing()}
  <div class="tf-empty">
    Nothing matches.
    {#if filter.scope === "job"}
      <button type="button" class="tf-btn tf-sm" onclick={searchAll}>Search all of Code</button>
    {/if}
  </div>
{/snippet}
