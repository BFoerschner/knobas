<!--
  Code (#837, R-8.1, R-8.2): `#/code` and `#/code/repos` draw *Repositories*,
  `#/code/prs` *Pull requests*. One page, as prototype-6's `renderCode(tab)`:
  the header with *New pull request*, the two tabs with their counts, the
  tab's own filter bar, then its rows.

  - **Repositories**: every mirrored repository, yours first (an open pull
    request of yours or one asking your review is in it), then by last push. Each row:
    the name with the job's key when the job reaches it, its open pull
    requests, the local checkout (`entity_checkout`: branch and where it
    stands, or *not on this machine*) and the last push.
  - **Pull requests**: the attention read (`list_entities`, order
    `attention`, #712) groups the open ones as *Asking for your review*,
    *Yours* and *Open, others*; the recency read adds *Merged or closed*.
    Each row: the number, the title over its repository and author, the state
    chip, the job it belongs to (`job/open-jobs.ts`) and when it last moved.
    A merged or closed pull request is under *Merged or closed*, yours too.

  **Rows stay where they first drew** (the still-rows pass, 2026-11-11): each
  list waits for the reads that decide its order (the repositories and the
  open pull requests), draws, and then holds its rows' places
  (`held-order.svelte.ts`). The team, language, author, checkout and later
  pages fill rows in or join the end; a row a later read would put above a
  drawn one waits behind *Show N new* in the filter bar, and a changed filter
  sorts again.

  The recency read asks for the newest 100 pull requests first and the rest
  once the page has drawn and the window is idle, or sooner when they are
  wanted: the list's end scrolled into view, the filter bar set, or a job's
  count on the tab (the query audit, 2026-11-12). A repository's
  team and language and a pull request's author are `code_facts` (#966), asked
  once for each row read, as its read lands. The other
  reads are `list_entities`, `entity_checkout`, `list_sources`,
  `list_contexts`, `get_entity` and `job_reach`. *Clone all…* in the header
  clones every repository not on this machine (`TfCloneAll`, PR #2479). Rows open
  `#/repo/<entity id>` and `#/pr/<entity id>`. A long list draws only the
  rows near the viewport (`shell/windowed.svelte.ts`, #2210), each group of
  pull requests on its own.
-->
<script lang="ts" module>
  import type { RoleFrameProps } from "../page-entry";
  import type { CheckoutOf, CheckoutView, CodeFacts, EntityFilter, EntityPage } from "../../ipc/entity";
  import { readAllRows } from "../repo/repo";
  import type { SourceSummary } from "../../ipc/sources";
  import type { SourceFacts } from "../job/job";
  import type { OpenJobPorts } from "../job/open-jobs";

  /** The reads this page makes, injectable so a test needs no bridge. */
  export interface CodePorts extends OpenJobPorts {
    listEntities: (filter: EntityFilter, limit: number, offset: number) => Promise<EntityPage>;
    /** Many checkouts in one call, in the order asked; an id refused is left out. */
    entityCheckouts: (entityIds: string[]) => Promise<CheckoutOf[]>;
    codeFacts: (entityIds: string[]) => Promise<CodeFacts[]>;
    /** The sources, for the host the header names. */
    listSources: () => Promise<(SourceFacts & Pick<SourceSummary, "base_url">)[]>;
  }
</script>

<script lang="ts">
  import TfKey from "../TfKey.svelte";
  import TfStateChip from "../TfStateChip.svelte";
  import { clockLabel } from "../../time/format";
  import { ipcErrorMessage } from "../../ipc";
  import { codeFacts, entityBriefs, entityCheckouts, getEntity, jobReach, listContexts, listEntities, type EntityRow } from "../../ipc/entity";
  import { listSources } from "../../ipc/sources";
  import { CODE_HOST } from "../roles.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import { isFiltering, type FilterConfig, type FilterScope } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import TfNoMatch from "../filter/TfNoMatch.svelte";
  import { clones } from "../openin/clones.svelte";
  import TfCloneAll from "../openin/TfCloneAll.svelte";
  import { Windowed } from "../../shell/windowed.svelte";
  import { HeldOrder, type Placed } from "../held-order.svelte";
  import { untrack } from "svelte";
  import { pageVisible } from "../kept-pages.svelte";
  import { onShownAgain } from "../while-shown.svelte";
  import TfWaiting from "../TfWaiting.svelte";
  import { waitPhase } from "../waiting";
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
    twinRows,
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
      entityCheckouts: (entityIds) => entityCheckouts(entityIds),
      codeFacts: (entityIds) => codeFacts(entityIds),
      listSources: () => listSources(),
      listContexts: () => listContexts(),
      getEntity: (entityId) => getEntity(entityId),
      entityBriefs: (entityIds) => entityBriefs(entityIds),
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
    /** The chosen job (spec #2227), as a ticket key. */
    job: string | null;
    ports?: CodePorts;
    /** Injectable clock, for the time column and *Idle > 5 days*. */
    now?: Date;
  } & RoleFrameProps = $props();

  const now = $derived(fixedNow ?? new Date());
  const tab = $derived<"repos" | "prs">(rest[0] === "prs" ? "prs" : "repos");

  /** How many rows one read asks for; a room of more is read on with `offset`. */
  const LIMIT = 1000;
  /**
   * How many pull requests the recency read asks for first, newest first: more than a screen of *Merged or closed*
   * (the query audit, 2026-11-12: every one of 1,710 was read in four reads in series, 711 ms and 552 KB, before the
   * tab could count them). The rest are read once the page has drawn and the window is idle, or sooner when they are
   * wanted: the reader scrolls to the list's end, sets the filter, or the job's count on the tab needs them
   * (`restWanted`). The open ones are the attention read's, whole.
   */
  const FIRST_PRS = 100;
  /**
   * How many repositories one `entity_checkouts` call asks about. One call is out at a time, and the backend runs at most
   * eight `git status` at once and none where nothing in the checkout changed (the scale pass, 2026-11-10: 65 calls on
   * every return to Code).
   */
  const CHECKOUT_BATCH = 200;

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
   *
   * `onPage`, where given, is handed the list so far after each page (#2245):
   * the first page draws at once and the rest append behind it, its `total`
   * the first page's, so the counts are the mirror's from the start.
   */
  async function whole(filter: EntityFilter, onPage?: (page: EntityPage) => void): Promise<EntityPage> {
    const first: { page?: EntityPage } = {};
    const rows = await readAllRows(
      async (offset) => {
        const page = await ports.listEntities(filter, LIMIT, offset);
        first.page ??= page;
        return page;
      },
      undefined,
      onPage && ((rows) => onPage({ ...first.page!, rows: rows as EntityRow[] })),
    );
    return { ...first.page!, rows };
  }

  // Replaced whole on each read, never changed in place: raw, so 10,000 rows are not proxied row by row (#2210).
  let repos = $state.raw<EntityPage | null>(null);
  let attention = $state.raw<EntityPage | null>(null);
  /** The open pull requests are read whole: what makes a repository yours, so its place, is known. */
  let attentionRead = $state(false);
  /** The sources are read: a twin row's *on <source>* and the header's host are their names, not ids that grow into them. */
  let sourcesRead = $state(false);
  let everyPr = $state.raw<EntityPage | null>(null);
  let branches = $state<EntityRow[]>([]);
  let sources = $state<Pick<SourceSummary, "id" | "base_url" | "display_name">[]>([]);
  let failed = $state<string | null>(null);
  /** A re-read under way (#2246): each tab keeps what it drew, dimmed, until its lists have answered. */
  let repoReading = $state(false);
  let prReading = $state(false);
  let checkouts = $state<Record<string, CheckoutView>>({});
  let jobs = $state<OpenJob[]>([]);
  let facts = $state.raw<CodeFacts[]>([]);

  $effect(() => {
    if (!ready) return;
    // *Sync now*'s runs ended (#2106): the rows are read again.
    void syncRevision;
    let live = true;
    failed = null;
    // A first read draws each list from its first page and appends the rest
    // (#2245); a re-read keeps the whole list drawn, dimmed, until the whole
    // new one is in, so a long list never shrinks back to one page under the
    // reader.
    const again = untrack(() => repos !== null || attention !== null || everyPr !== null);
    repoReading = again;
    prReading = again;
    const fail = (error: unknown) => {
      if (live) failed = ipcErrorMessage(error);
    };
    const readCheckouts = checkoutReader(() => live);
    let asked = 0;
    const repoPage = (page: EntityPage) => {
      if (!live) return;
      repos = page;
      readCheckouts(page.rows.slice(asked).map((row) => row.entity_id));
      asked = page.rows.length;
    };
    const whileLive = (set: (page: EntityPage) => unknown) => (page: EntityPage) => {
      if (live) set(page);
    };
    // A first read is handed every page as it lands, the last one too, so
    // only a re-read sets the list from the whole answer.
    const read = (filter: EntityFilter, set: (page: EntityPage) => void) =>
      again ? whole(filter).then(set, fail) : whole(filter, set).then(undefined, fail);
    const repoRead = read(filterOf(["repo"], "updated_desc"), repoPage);
    const attentionWhole = read(filterOf(["pr"], "attention"), whileLive((page) => (attention = page)));
    void attentionWhole.finally(() => {
      if (live) attentionRead = true;
    });
    // Every pull request when the rest was wanted before this read, so a re-read never shrinks the list; else the
    // first screen's worth, and the rest behind it once wanted (`readRest`).
    const wholePrs = untrack(() => restWanted) || restRead;
    readsAt += 1;
    restRead = wholePrs;
    const newest = filterOf(["pr"], "updated_desc");
    const prReads = [
      attentionWhole,
      wholePrs
        ? read(newest, whileLive((page) => (everyPr = page)))
        : ports.listEntities(newest, FIRST_PRS, 0).then(whileLive((page) => (everyPr = page)), fail),
    ];
    void repoRead.finally(() => {
      if (live) repoReading = false;
    });
    void Promise.allSettled(prReads).then(() => {
      if (live) prReading = false;
    });
    ports.listSources().then(
      (rows) => {
        if (live) sources = rows;
      },
      () => {},
    ).finally(() => {
      if (live) sourcesRead = true;
    });
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

  /** Bumped by each read of the lists: a later page read for an earlier one lands nowhere. */
  let readsAt = 0;
  /** Whether the rest of the pull requests is read, or being read, for the lists' current read. */
  let restRead = false;
  /** The end of the *Pull requests* list scrolled into view: the reader wants what is past the first screen. */
  let atPrEnd = $state(false);
  let prEnd: HTMLElement | null | undefined = $state();
  $effect(() => {
    const at = prEnd;
    if (at == null || typeof IntersectionObserver === "undefined") return;
    const watcher = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) atPrEnd = true;
    });
    watcher.observe(at);
    return () => watcher.disconnect();
  });

  /**
   * The page has drawn and the window has gone idle: the rest is read now, so it is there before anyone types into the
   * filter. Read on the first key instead, its landing cost that key 265 ms at 4x on the fixture at scale (the frame
   * budget's `code-filter`, a 100 ms line), where every key over rows already read is under 50.
   */
  let idleAfterDraw = $state(false);
  $effect(() => {
    if (idleAfterDraw || !(reposOrdered || prsOrdered)) return;
    let gone = false;
    const go = () => {
      if (!gone) idleAfterDraw = true;
    };
    if (typeof requestIdleCallback === "function") {
      const handle = requestIdleCallback(go, { timeout: 2000 });
      return () => ((gone = true), cancelIdleCallback(handle));
    }
    const handle = setTimeout(go, 300);
    return () => ((gone = true), clearTimeout(handle));
  });

  // The pull requests past the first screen, once wanted: appended behind it, older than every row drawn, so they
  // join the end of *Merged or closed* and nothing drawn moves. They land in one piece, not a page at a time: each
  // landing sorts and filters the whole list again.
  $effect(() => {
    const first = everyPr;
    if (!restWanted || first === null || restRead || first.rows.length >= first.total) return;
    restRead = true;
    const at = readsAt;
    const alive = () => at === readsAt;
    const filter = filterOf(["pr"], "updated_desc");
    void (async () => {
      const rows = [...first.rows];
      const seen = new Set(rows.map((row) => row.entity_id));
      let offset = rows.length;
      while (alive() && offset < first.total) {
        const page = await ports.listEntities(filter, LIMIT, offset);
        if (!alive() || page.rows.length === 0) return;
        offset += page.rows.length;
        for (const row of page.rows) if (!seen.has(row.entity_id)) seen.add(row.entity_id), rows.push(row);
      }
      if (alive() && rows.length > first.rows.length) everyPr = { ...first, rows };
    })().catch(() => {});
  });

  /** The ids `code_facts` was asked for in this round of reads, and the round: a run's re-read asks them again. */
  let factsAsked = new Set<string>();
  let factsRound = -1;
  // Team, language and author (#966) for the rows read, each asked once a round: the repositories, the open pull
  // requests and the recency read's so far, so a later page asks for its own rows alone (the query audit: all 1,775
  // ids twice on every open, 411 KB). A failed read leaves the cells empty rather than failing the page.
  $effect(() => {
    if (repos === null || everyPr === null) return;
    const round = syncRevision;
    const ids = [...repos.rows, ...(attention?.rows ?? []), ...everyPr.rows].map((row) => row.entity_id);
    if (round !== factsRound) {
      factsRound = round;
      factsAsked = new Set();
    }
    const ask = [...new Set(ids)].filter((id) => !factsAsked.has(id));
    if (ask.length === 0) return;
    for (const id of ask) factsAsked.add(id);
    ports.codeFacts(ask).then(
      (answer) => {
        if (round !== factsRound) return;
        const byId = new Map(untrack(() => facts).map((fact) => [fact.entity_id, fact]));
        for (const fact of answer) byId.set(fact.entity_id, fact);
        facts = [...byId.values()];
      },
      () => {
        if (round === factsRound) for (const id of ask) factsAsked.delete(id);
      },
    );
  });

  /**
   * Shown again with no run of a forge ending meanwhile, Code reads none of its lists (Björn, 2026-11-10: every return
   * read all 4,400 pull requests). A checkout is the one thing a sync never brings, since a commit or a clone made in a
   * terminal changes it, so each row reads its checkout again; a return the lists read again for has its checkouts read
   * with them, and this reads nothing more.
   */
  onShownAgain(pageVisible(), () =>
    queueMicrotask(() => {
      if (repos === null || repoReading) return;
      checkoutReader(() => true)(repos.rows.map((row) => row.entity_id));
    }),
  );

  // A clone that finished: its row reads its checkout again (R-8.25).
  let seenClones = clones.finished;
  $effect(() => {
    if (clones.finished === seenClones) return;
    seenClones = clones.finished;
    const ids = (repos?.rows ?? []).filter((row) => !checkouts[row.entity_id]?.path).map((row) => row.entity_id);
    checkoutReader(() => true)(ids);
  });

  /**
   * Each repository's checkout, in calls of up to {@link CHECKOUT_BATCH}, one out at a time; one that fails draws
   * nothing. The reader it answers takes more ids as each page of repositories lands, into the next call.
   */
  function checkoutReader(alive: () => boolean): (ids: string[]) => void {
    const queue: string[] = [];
    let running = false;
    const run = async () => {
      running = true;
      try {
        while (queue.length > 0 && alive()) {
          const ids = queue.splice(0, CHECKOUT_BATCH);
          try {
            const answers = await ports.entityCheckouts(ids);
            if (alive()) for (const answer of answers) checkouts[answer.entity_id] = answer.checkout;
          } catch {
            // An unreadable checkout leaves the cell empty rather than claiming one.
          }
        }
      } finally {
        running = false;
      }
    };
    return (ids) => {
      queue.push(...ids);
      if (!running) void run();
    };
  }

  const index = $derived(jobIndex(jobs));
  const held = $derived((entityId: string) => job !== null && index.holds(job, { entity_id: entityId }));
  const known = $derived(factsById(facts));
  const lines = $derived(attention && everyPr ? prLines(attention, everyPr, now, known) : null);
  /** The open pull requests alone: what orders the repositories and counts their open ones, before the recency read lands. */
  const openLines = $derived(attention ? prLines(attention, { ...attention, rows: [] }, now, known) : null);
  const teamsByRepo = $derived(repoTeams(repos?.rows ?? [], known));
  const choices = $derived(repoChoices(repos?.rows ?? [], known));
  const openCounts = $derived(openPrsOf(openLines ?? []));
  const compare = $derived(compareHash(branches, repos?.rows ?? []));
  /** The job's open pull request: the loudest button goes there, as *New pull request* would only land on saying one is open (F-243). */
  const jobPr = $derived(job === null ? null : ((lines ?? []).find((line) => line.row.state?.text.toLowerCase() === "open" && index.jobOf({ entity_id: line.row.entity_id, kind: line.row.kind }) === job) ?? null));

  // -- the filter bars -------------------------------------------------------

  const repoFilter = sessionFilter("code-repos");
  const prFilter = sessionFilter("code-prs");
  /**
   * Whether every pull request is wanted, not the first screen's: the list's end is in view, the filter bar keeps
   * something (it filters what is read, so it must hold them all), or a job's count on the tab counts them.
   */
  const prFilterSet = $derived(isFiltering(prFilter) || prFilter.scope !== "all");
  const restWanted = $derived(idleAfterDraw || atPrEnd || prFilterSet || (tab === "prs" && job !== null));
  // Prototype-6 opens both tabs on *All*.
  if (repoFilter.scope === null) repoFilter.scope = "all";
  if (prFilter.scope === null) prFilter.scope = "all";

  $effect(() => {
    if (job !== null) return;
    if (repoFilter.scope === "job") repoFilter.scope = "all";
    if (prFilter.scope === "job") prFilter.scope = "all";
  });

  const scopes = $derived<FilterScope[]>([
    ...(job !== null
      ? [{ id: "job", label: "This job", count: (tab === "prs" ? (everyPr?.rows ?? []) : (repos?.rows ?? [])).filter((row) => held(row.entity_id)).length, title: `What ${job} reaches`, query: "" }]
      : []),
    { id: "mine", label: "Mine", query: "" },
    { id: "all", label: "All", query: "" },
  ]);
  const options = (values: readonly string[]) => values.map((value) => ({ value, label: value }));
  const repoNames = $derived([...new Set((repos?.rows ?? []).map((row) => row.title))].sort());

  const repoConfig = $derived<FilterConfig>({
    id: "code-repos",
    placeholder: "Filter repositories",
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
    placeholder: "Filter PRs",
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

  /** Rows another source holds under the same name: each says its source (`twinRows`). */
  const twinRepos = $derived(twinRows(repos?.rows ?? [], (row) => row.title));
  const twinPrs = $derived(twinRows((lines ?? []).map(({ row, number }) => ({ ...row, number })), (row) => `${row.path ?? ""}${row.number ?? ""}`));
  /** The source a twin row names, by its name; its id until the sources are read. */
  const sourceName = (sourceId: string) => sources.find((source) => source.id === sourceId)?.display_name || sourceId;

  const shownRepos = $derived(
    repos === null
      ? []
      : visibleRepos(repos.rows, openLines ?? [], checkouts, known, {
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

  // -- rows stay where they first drew ---------------------------------------

  /** Each list draws once its order is decided: the repositories once the open pull requests are read whole too. */
  const reposOrdered = $derived(repos !== null && attentionRead && sourcesRead);
  const prsOrdered = $derived(lines !== null && attentionRead && sourcesRead);
  /** What the person chose: a change sorts the list again (`HeldOrder`). */
  const basisOf = (chosen: typeof repoFilter) => JSON.stringify([chosen.scope, chosen.text, chosen.chips, chosen.dropdowns, job]);
  const repoHold = new HeldOrder<EntityRow>((row) => row.entity_id);
  const prHold = new HeldOrder<PrLine>((line) => line.row.entity_id, (line) => line.group ?? "open");
  const none = <T,>(): Placed<T> => ({ rows: [], waiting: 0, stale: false });
  const placedRepos = $derived(reposOrdered ? repoHold.place(shownRepos, basisOf(repoFilter)) : none<EntityRow>());
  /** The groups' rows as one list in draw order, so a later read's row joins the end of the last group, never above one. */
  const flatPrs = $derived(
    [null, ...GROUP_ORDER].flatMap((group) => shownPrs.filter((line) => line.group === group)),
  );
  const placedPrs = $derived(prsOrdered ? prHold.place(flatPrs, basisOf(prFilter)) : none<PrLine>());

  /** The pull requests by group, in draw order, each in the group it drew in; open ones with no group come first, unheaded. */
  const groups = $derived.by(() => {
    const out: { group: PrGroup | null; lines: PrLine[] }[] = [];
    const tagged = placedPrs.rows.map((line) => ({ line, tag: prHold.tagOf(line) }));
    for (const group of [null, ...GROUP_ORDER] as (PrGroup | null)[]) {
      const kept = tagged.filter(({ tag }) => tag === (group ?? "open")).map(({ line }) => line);
      if (kept.length > 0) out.push({ group, lines: kept });
    }
    return out;
  });
  const heldRepos = $derived({ waiting: placedRepos.waiting, stale: placedRepos.stale, show: () => repoHold.release() });
  const heldPrs = $derived({ waiting: placedPrs.waiting, stale: placedPrs.stale, show: () => prHold.release() });

  /** Thousands of rows draw only those near the viewport (#2210); a repository row is 42 px, a pull request's 51. */
  const repoWin = new Windowed({ count: () => placedRepos.rows.length, key: (i) => placedRepos.rows[i]!.entity_id, estimate: 42 });
  const linesOf = (name: PrGroup | "open") => groups.find((kept) => (kept.group ?? "open") === name)?.lines ?? [];
  const prWins = new Map(
    (["open", ...GROUP_ORDER] as const).map((name) => [
      name,
      // From 100 rows: three groups of under 300 each drew whole, about 550 rows and a 140 ms frame as the tab opened
      // on the live scale estate (2026-11-10).
      new Windowed({ count: () => linesOf(name).length, key: (i) => linesOf(name)[i]!.row.entity_id, estimate: 51, from: 100 }),
    ]),
  );

  /** The mirror's count from the first page on, not the rows in so far (#2245). */
  const prTotal = $derived(lines === null || everyPr === null ? null : Math.max(lines.length, everyPr.total));
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
      {repos === null || !reposOrdered ? "Repositories and pull requests, yours first." : headerLine(repos.total, repos.rows, sources)}
    </div>
  </div>
  <div class="tf-acts">
    {#if jobPr !== null}
      <a class="tf-btn tf-primary" href={itemHash(jobPr.row)}>Open PR {jobPr.number ?? ""}</a>
    {:else if compare !== null}
      <a class="tf-btn tf-primary" href={compare}>New pull request</a>
    {/if}
    <!-- With no job's branch to propose there is no button: a repository's own page offers it, on the branch it finds (F-252). -->
    <!-- *Clone all…* (PR #2479): every repository not on this machine, in the background. -->
    <TfCloneAll />
    <!-- *Sync now* last in the header (#2238 header order). -->
    {@render sync?.()}
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
    shown={reposOrdered ? placedRepos.rows.length : null}
    total={repos?.total ?? null}
    held={heldRepos}
  />
{:else}
  <!-- Unfiltered, the list holds every pull request: those past the first screen are read as the reader scrolls to them. -->
  <FilterBar
    config={prConfig}
    filter={prFilter}
    shown={prsOrdered ? (prFilterSet ? placedPrs.rows.length : prTotal) : null}
    total={prTotal}
    held={heldPrs}
  />
{/if}
<div class="tf-cbody">
  {#if tab === "repos"}
    <TfWaiting phase={waitPhase({ drawn: reposOrdered, reading: repoReading, failed: failed !== null })} error={failed} columns={7}>
    {#if placedRepos.rows.length > 0}
      <table class="tf-tbl">
        <thead>
          <tr
            ><th>Repository</th><th>Team</th><th>Language</th><th>Open PRs</th><th>Local checkout</th><th>Last push</th><th
              ><span class="tf-sr">Open or clone</span></th
            ></tr
          >
        </thead>
        <tbody {@attach repoWin.list}>
          {#if repoWin.before > 0}<tr class="tf-wgap" aria-hidden="true"><td colspan="7" {@attach repoWin.room("before")}></td></tr>{/if}
          {#each repoWin.slice(placedRepos.rows) as row (row.entity_id)}
            {@const count = openCount(openCounts, row)}
            {@const local = checkoutLine(checkouts[row.entity_id])}
            {@const fact = known.get(row.entity_id)}
            <tr class="tf-row" onclick={(event) => open(event, itemHash(row))} {@attach repoWin.row(row.entity_id)}>
              <td
                ><a class="tf-mono" href={itemHash(row)}
                  ><span class="tf-dim"><Marked text={row.title.slice(0, row.title.lastIndexOf("/") + 1)} typed={repoFilter.text} /></span
                  ><Marked text={row.title.slice(row.title.lastIndexOf("/") + 1)} typed={repoFilter.text} /></a
                >{#if twinRepos.has(row.entity_id)}{" "}<span class="tf-dim">on {sourceName(row.source_id)}</span>{/if}{#if job !== null && held(row.entity_id)}{" "}<TfKey chip value={job} class="tf-chip-job" />{/if}</td
              >
              <td><Marked text={fact?.teams.join(", ") ?? ""} typed={repoFilter.text} /></td>
              <td><Marked text={fact?.language ?? ""} typed={repoFilter.text} /></td>
              <td>{#if count > 0}{count}{:else}<span class="tf-dim">0</span>{/if}</td>
              <td class:tf-dim={local?.dim}>{local?.text ?? ""}</td>
              <td class="tf-dim tf-nowrap"><span class="tf-rowval">{row.updated_at ? clockLabel(row.updated_at, now) : ""}</span></td>
              <td class="tf-nowrap tf-right"><span class="tf-rowend"><span class="tf-rowacts"><TfOpenIn entityId={row.entity_id} title={row.title} checkout={checkouts[row.entity_id]} variant="ghost" /></span></span></td>
            </tr>
          {/each}
          {#if repoWin.after > 0}<tr class="tf-wgap" aria-hidden="true"><td colspan="7" {@attach repoWin.room("after")}></td></tr>{/if}
        </tbody>
      </table>
    {:else}
      {@render nothing()}
    {/if}
    </TfWaiting>
  {:else}
    <TfWaiting phase={waitPhase({ drawn: prsOrdered, reading: prReading, failed: failed !== null })} error={failed} columns={5}>
    {#each groups as { group, lines: kept } (group ?? "open")}
      {#if group !== null}
        <h2 class="tf-cgroup-h">{GROUP_LABEL[group]} <span class="tf-count">{kept.length}</span></h2>
      {/if}
      {@const win = prWins.get(group ?? "open")!}
      <table class="tf-tbl tf-cprs">
        <!-- Its own widths, every group alike: a fixed table takes them from its first row, which a windowed group's spacer would be (2026-11-10). -->
        <colgroup><col class="tf-cp-no" /><col /><col class="tf-cp-state" /><col class="tf-cp-job" /><col class="tf-cp-at" /></colgroup>
        <tbody {@attach win.list}>
          {#if win.before > 0}<tr class="tf-wgap" aria-hidden="true"><td colspan="5" {@attach win.room("before")}></td></tr>{/if}
          {#each win.slice(kept) as { row, number, idle, author, mine } (row.entity_id)}
            {@const belongs = index.jobOf({ entity_id: row.entity_id, kind: row.kind })}
            <tr class="tf-row" onclick={(event) => open(event, itemHash(row))} {@attach win.row(row.entity_id)}>
              <td class="tf-prno"><Marked text={number ?? ""} typed={prFilter.text} /></td>
              <td
                ><a class="tf-rowt" href={itemHash(row)}><Marked text={row.title} typed={prFilter.text} /></a><span class="tf-sub"
                  >{#if row.path}<span class="tf-mono"><Marked text={row.path} typed={prFilter.text} /></span
                    >{/if}{#if twinPrs.has(row.entity_id)}{row.path ? " " : ""}on {sourceName(row.source_id)}{/if}{#if author}{row.path ? ", " : ""}{#if mine}you{:else}<Marked text={author} typed={prFilter.text} />{/if}{/if}{#if idle}{row.path ||
                    author
                      ? ", "
                      : ""}idle for more than 5 days{/if}</span
                ></td
              >
              <td>{#if row.state}<TfStateChip text={row.state.text} tone={row.state.tone} />{/if}</td>
              <td class="tf-mono tf-dim">{#if belongs !== null && belongs === job}<TfKey chip value={belongs} class="tf-chip-job" />{:else}{belongs ?? ""}{/if}</td>
              <td class="tf-dim tf-nowrap">{row.updated_at ? clockLabel(row.updated_at, now) : ""}</td>
            </tr>
          {/each}
          {#if win.after > 0}<tr class="tf-wgap" aria-hidden="true"><td colspan="5" {@attach win.room("after")}></td></tr>{/if}
        </tbody>
      </table>
    {:else}
      {@render nothing()}
    {/each}
    {#if prsOrdered && everyPr !== null && everyPr.rows.length < everyPr.total}
      <!-- The list's end: in view, the pull requests past the first screen are read and join the end. -->
      <div class="tf-cprs-end" aria-hidden="true" bind:this={prEnd}></div>
    {/if}
    </TfWaiting>
  {/if}
</div>

{#snippet nothing()}
  <TfNoMatch noun={tab === "prs" ? "pull requests" : "repositories"} {filter}>
    {#if filter.scope === "job"}
      <button type="button" class="tf-btn tf-sm" onclick={searchAll}>Search all of Code</button>
    {/if}
  </TfNoMatch>
{/snippet}
