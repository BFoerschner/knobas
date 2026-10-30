<!--
  The repository page (#838, R-8.3 to R-8.5, R-8.7, R-8.10), prototype-6's
  `repoHead` and `renderRepo`: `#/repo/<repo entity id>` and its tabs
  `#/repo/<id>/<tab>`, where `files/<path>` is Files with a file open and
  `commits/<commit entity id>` a commit's diff.

  - **The header**: the repository's `owner/name`, its declared teams and
    language (`code_facts`, #973), its last push and its
    checkout (`entity_checkout`: where it is cloned and how it stands, or *Not
    on this machine yet*); the *Branch* dropdown, which sets the branch every
    tab reads at and is held per repository for the session; *New pull
    request* (the compare page, #840) first as the primary, *New branch…*
    and *Open in…* (#844), or *Clone* for a repository not on this machine
    (#846), in #2238's order.
  - **The tabs**, `REPO_TABS` in `repo.ts`, in two groups split by a rule
    (#2257, the mockup of #2252): the repository at a ref, Files (and its
    edit, #845), Commits, Branches (#839) and Compare (#840); then work in
    flight, Pull requests and Local changes (#847).
  - The checkout is read again when the window gets focus and when a clone
    finishes (ADR-0021); while keep-alive holds the page hidden (#2249) it
    is not, and it is read once when the page is shown again.
  - **Kept tabs** (#2248, `repo-tabs.ts`): a tab visited stays mounted,
    hidden, for the visit to this repository, and shows again as it was left
    (its scroll, its tree, its file), reading again over what it drew. Its
    link in the row goes back to where it was left. Another repository
    starts afresh; another branch keeps only the tab shown.
  - **Refresh** (#2135, `tf/refresh.svelte.ts`): *Refresh* or *Sync now* in
    the header, as the repository's read offers, and the withdrawn and
    turned-off banners under it. A refresh reads the repository's own reads
    again, and `refreshed` tells the open tab to read its sections again, each
    over what it drew; *Refreshing…* holds until the tab's reads have answered
    too (`refreshed.svelte.ts`).
  - **Writes** go through the write queue (`submit_write`): *New branch…*
    (`CreateBranch`), a branch's *Delete…* (`DeleteBranch`), *Create pull
    request* (`CreatePullRequest`) and an edit's commit (`CommitFile`).

  **Without waiting** (#2243): the tabs draw at once on the default branch
  the mirror holds (`mirrorDefault`), or the branch picked this session; the
  *Branch* selector fills in when the live branch list arrives, and the tab
  does not read again for it. *New branch…* and the file's *Commit change…*
  are offered or refused by `refused_writes` (#2241), asked together, never
  by a read of the whole repository view. A branch list the app answered from
  memory and then read again behind (`view:fresh`, #2242) is read again here.

  The repository is read through its source's own view (`entity_view`), so a
  repository nobody cloned reads the same as one on this disk; the checkout
  only adds the `M` marks and the warning on a changed file.
-->
<script lang="ts" module>
  import type { EchoListen } from "../queued.svelte";
  import type {
    CheckoutView,
    ChoiceOption,
    CodeFacts,
    DiffRead,
    EntityDetail,
    EntityFilter,
    EntityPage,
    PipeRun,
    PipeStep,
    RefusedWrite,
    SourceView,
    ViewFresh,
    WriteAsk,
  } from "../../ipc/entity";
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import type { WriteSource } from "../../shell/queued-toast";
  import type { OpenJobPorts } from "../job/open-jobs";
  import type { DetailRefreshPorts } from "../refresh.svelte";

  /** The reads this page makes, injectable so a test needs no bridge. */
  export interface RepoPorts extends OpenJobPorts {
    getEntity: (entityId: string) => Promise<EntityDetail>;
    listEntities: (filter: EntityFilter, limit: number, offset: number) => Promise<EntityPage>;
    entityView: (entityId: string, section: string | null) => Promise<SourceView | null>;
    entityCheckout: (entityId: string) => Promise<CheckoutView>;
    entityCheckoutDiff: (entityId: string) => Promise<DiffRead>;
    setCheckoutOverride: (entityId: string, path: string | null) => Promise<CheckoutView>;
    /** `linkTo` (#984): a ticket the branch or pull request made is linked to once it arrives. */
    submitWrite: (payload: WriteOpPayload, linkTo?: string) => Promise<QueuedWrite>;
    /** The header's team and language, and Commits' and Branches' authors (#966, #973, #986). */
    codeFacts: (entityIds: string[]) => Promise<CodeFacts[]>;
    /** Hears `item:refreshed` and `sync:state`, which read a tab again for its echoes (#2105); the bridge's when left out. */
    listen?: EchoListen;
    /** The header's *Refresh* (#2135); the bridge's when left out. */
    refresh?: DetailRefreshPorts;
    /** *New branch…* and *Commit change…*'s standing (#2241); none refused when left out. */
    refusedWrites?: (asks: WriteAsk[]) => Promise<RefusedWrite[]>;
    /** The byte read (#2256): a Markdown file's relative images, at the file's commit; none load when left out. */
    fileBytes?: (entityId: string, path: string, commit: string | null) => Promise<Uint8Array>;
    /** A file pipeline over a file's bytes (#2264); the viewer offers none when left out. */
    piped?: (entityId: string, path: string, commit: string | null, steps: PipeStep[]) => Promise<PipeRun>;
  }

  /**
   * The branch each repository is read at, by its entity id, as prototype-6's
   * `S.repoBranch`: held for the session, never written anywhere. Its entity
   * id is held too, so the tabs read at it before the branch list answers.
   */
  const chosen: Record<string, { name: string; entityId: string | null }> = $state({});

  /** Forget every repository's branch: a new session, which is what each test is. */
  export function forgetBranches(): void {
    for (const key of Object.keys(chosen)) delete chosen[key];
  }
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { EVENTS, ipcErrorMessage } from "../../ipc";
  import {
    codeFacts,
    entityCheckout,
    entityCheckoutDiff,
    entityFileBytes,
    entityFilePiped,
    entityView,
    getEntity,
    jobReach,
    listContexts,
    listEntities,
    refusedWrites,
    setCheckoutOverride,
    submitWrite,
    type EntityRow,
  } from "../../ipc/entity";
  import { listSources } from "../../ipc/sources";
  import { listen as tauriListen } from "@tauri-apps/api/event";
  import { jobHash } from "../route";
  import {
    branchOptions,
    changedPaths,
    checkoutSentence,
    headLine,
    litTab,
    readAllRows,
    mirrorDefault,
    readRepoRoute,
    repoAsks,
    repoHash,
    repoPrs,
    repoRefusals,
    REPO_TABS,
    type BranchOption,
    type RepoTab,
    type RepoTabId,
  } from "./repo";
  import TfRepoCommit from "./TfRepoCommit.svelte";
  import TfRepoCommits from "./TfRepoCommits.svelte";
  import TfRepoFiles from "./TfRepoFiles.svelte";
  import TfNewBranch from "./TfNewBranch.svelte";
  import TfRepoBranches from "./TfRepoBranches.svelte";
  import TfRepoCompare from "./TfRepoCompare.svelte";
  import TfRepoPrs from "./TfRepoPrs.svelte";
  import TfRepoLocal from "./TfRepoLocal.svelte";
  import TfOpenIn from "../openin/TfOpenIn.svelte";
  import { clones } from "../openin/clones.svelte";
  import TfDetailBanners from "../TfDetailBanners.svelte";
  import TfDetailRefresh from "../TfDetailRefresh.svelte";
  import { createDetailRefresh } from "../refresh.svelte";
  import { provideTabRereads } from "./refreshed.svelte";
  import { NO_TABS, readShown, SLOTS, slotOf, tabHref, visit, type TabSlot } from "./repo-tabs";
  import TfRepoTab from "./TfRepoTab.svelte";
  import { providePageVisible } from "../kept-pages.svelte";
  import { onFocusWhileShown, onShownAgain } from "../while-shown.svelte";

  let {
    ready,
    rest,
    job,
    jobAt = null,
    visible = true,
    ports = {
      getEntity: (entityId) => getEntity(entityId),
      listEntities: (filter, limit, offset) => listEntities(filter, limit, offset),
      entityView: (entityId, section) => entityView(entityId, section),
      entityCheckout: (entityId) => entityCheckout(entityId),
      entityCheckoutDiff: (entityId) => entityCheckoutDiff(entityId),
      setCheckoutOverride: (entityId, path) => setCheckoutOverride(entityId, path),
      listContexts: () => listContexts(),
      listSources: () => listSources(),
      jobReach: (entityId) => jobReach(entityId),
      submitWrite: (payload, linkTo) => submitWrite(payload, linkTo),
      codeFacts: (entityIds) => codeFacts(entityIds),
      refusedWrites: (asks) => refusedWrites(asks),
      fileBytes: (entityId, path, commit) => entityFileBytes(entityId, path, commit),
      piped: (entityId, path, commit, steps) => entityFilePiped(entityId, path, commit, steps),
    },
    now: fixedNow,
  }: {
    /** Whether the database answers yet: the reads wait for it. */
    ready: boolean;
    /** What follows `repo` in the address. */
    rest: string[];
    /** The job the reader was last in, as a ticket key. */
    job: string | null;
    /** The address the job was opened at (#1525), which its chip links; its key when omitted. */
    jobAt?: string | null;
    /** Whether keep-alive shows the page (#2247); hidden, it reads nothing on focus (#2249). */
    visible?: boolean;
    ports?: RepoPorts;
    /** Injectable clock, for the times drawn. */
    now?: Date;
  } = $props();

  const now = $derived(fixedNow ?? new Date());
  const route = $derived(readRepoRoute(rest));
  const repoId = $derived(route.repoId);

  /** How many pull requests the repository's read asks for: every one the mirror holds. */
  const LIMIT = 1000;

  /** The repository's read: its row, its source, and what its *Refresh* offers (#2135). */
  let detail = $state<EntityDetail | null>(null);
  const repo = $derived<EntityRow | null>(detail?.row ?? null);
  /** The repository's source: where its writes go, named in their toasts. */
  const source = $derived<WriteSource | null>(detail?.source ?? null);
  let failed = $state<string | null>(null);
  /** `null` while the *Branches* section has not answered. */
  let branches = $state<BranchOption[] | null>(null);
  /**
   * Why the repository's own toolbar draws *New branch…* disabled (a mirror,
   * a token that cannot write, #2030); `null` while it has not answered, and
   * where it offers the branch.
   */
  let branchRefused = $state<string | null>(null);
  /** Why a commit straight onto the branch the tabs read at is refused (#985, #2241); `null` where it is taken. */
  let commitRefused = $state<string | null>(null);
  let checkout = $state<CheckoutView | undefined>(undefined);
  let local = $state<DiffRead | null>(null);
  let prs = $state<EntityRow[] | null>(null);
  /** The repository's team and language (#973); `undefined` until `code_facts` answers. */
  let facts = $state<CodeFacts | undefined>(undefined);
  /** The header's *Refresh* or *Sync now* (#2135). */
  const refresh = createDetailRefresh(untrack(() => ports.refresh));
  providePageVisible(() => visible);
  /** The open tab's re-reads of a refresh: *Refreshing…* waits for them too, as the job page's does (#2134). */
  const tabRereads = provideTabRereads();

  $effect(() => {
    if (!ready || repoId === null) return;
    const id = repoId;
    let live = true;
    const alive = () => live;
    detail = null;
    failed = null;
    branches = null;
    branchRefused = null;
    checkout = undefined;
    local = null;
    prs = null;
    facts = undefined;
    untrack(() => refresh.reset());
    readRepo(id, alive, false);
    readCheckout(id, alive);
    return () => {
      live = false;
    };
  });

  /** The checkout again on focus, and once when the page is shown again (#2249). */
  function checkoutAgain() {
    const id = repoId;
    if (ready && id !== null) readCheckout(id, () => repoId === id);
  }
  onFocusWhileShown(() => visible, checkoutAgain);
  // The open tab reads again too, over what it drew, as it does on a refresh.
  onShownAgain(() => visible, () => (tabs = readShown(tabs)));

  /**
   * The repository's own reads: its row, its team and language, its pull
   * requests, its branches and its toolbar. `again` is a refresh's (#2135):
   * each read lands over what the page drew, and one that fails keeps it.
   * Settles once every one of them has answered.
   */
  async function readRepo(id: string, alive: () => boolean, again: boolean): Promise<void> {
    const factsRead = ports.codeFacts([id]).then(
      (answer) => {
        if (alive()) facts = answer.find((entry) => entry.entity_id === id);
      },
      () => {},
    );
    // An empty repository has no branches; a source that cannot say reads as
    // one with only its default branch, which the repository's own view is.
    const branchesRead = ports.entityView(id, "branches").then(
      (view) => {
        if (alive()) branches = branchOptions(view?.sections[0] ?? null);
      },
      () => {
        if (alive() && !again) branches = [];
      },
    );
    await Promise.all([factsRead, branchesRead, readRow(id, alive, again)]);
  }

  /** The branch list again, where the app read it again behind its answer and it moved (#2242). */
  $effect(() => {
    if (!ready || repoId === null) return;
    const id = repoId;
    const listen = untrack(() => ports.listen) ?? ((event, handler) => tauriListen(event, handler));
    let live = true;
    let stop: (() => void) | null = null;
    void listen(EVENTS.viewFresh, ({ payload }) => {
      const fresh = payload as ViewFresh;
      if (!live || fresh.entity_id !== id || fresh.section !== "branches") return;
      ports.entityView(id, "branches").then(
        (view) => {
          if (live && repoId === id) branches = branchOptions(view?.sections[0] ?? null);
        },
        () => {},
      );
    }).then(
      (unlisten) => {
        if (live) stop = unlisten;
        else unlisten();
      },
      // No bridge to hear it on (a test without `listen`): nothing is told.
      () => {},
    );
    return () => {
      live = false;
      stop?.();
    };
  });

  // Whether the source offers *New branch…* and a commit onto the branch is
  // its answer to `refused_writes` (ADR-0020 rule 4, #2241), asked together;
  // one that does not answer refuses nothing.
  $effect(() => {
    if (!ready || repoId === null || at === null) return;
    const [id, where] = [repoId, at];
    let live = true;
    const ask = untrack(() => ports.refusedWrites) ?? (async () => []);
    ask(repoAsks(id, where)).then(
      (refused) => {
        if (!live) return;
        const answer = repoRefusals(refused, id, where);
        branchRefused = answer.branch;
        commitRefused = answer.commit;
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });

  /** The repository's row, then its pull requests. */
  async function readRow(id: string, alive: () => boolean, again: boolean): Promise<void> {
    let found: EntityDetail;
    try {
      found = await ports.getEntity(id);
    } catch (error) {
      if (alive() && !again) failed = ipcErrorMessage(error);
      return;
    }
    if (!alive()) return;
    detail = found;
    const filter: EntityFilter = {
      sources: [found.row.source_id],
      kinds: ["pr"],
      updated_within_days: null,
      order: "updated_desc",
      context: null,
      project: null,
      include_deleted: false,
      reach: null,
    };
    await readAllRows((offset) => ports.listEntities(filter, LIMIT, offset)).then(
      (rows) => {
        if (alive()) prs = repoPrs(rows, found.row);
      },
      () => {
        if (alive() && !again) prs = [];
      },
    );
  }

  /** A refresh's re-read (#2135): the repository's reads and the open tab's, over what the page drew; settles once all have answered. */
  async function reread(): Promise<void> {
    const id = repoId;
    if (id === null) return;
    tabs = readShown(tabs);
    await Promise.all([readRepo(id, () => repoId === id, true), tabRereads.settled()]);
  }

  /** The checkout, and its diff when it has changes: for the `M` marks. */
  function readCheckout(id: string, alive: () => boolean) {
    ports.entityCheckout(id).then(
      (view) => {
        if (!alive()) return;
        checkout = view;
        const tree = view.tree;
        if (tree?.status !== "read" || tree.changed + tree.untracked === 0) {
          local = null;
          return;
        }
        ports.entityCheckoutDiff(id).then(
          (read) => {
            if (alive()) local = read;
          },
          () => {},
        );
      },
      () => {},
    );
  }

  // A clone that finished: *Clone* becomes *Open in…*.
  let seenClones = clones.finished;
  $effect(() => {
    if (clones.finished === seenClones) return;
    seenClones = clones.finished;
    const id = repoId;
    if (id !== null && ready) readCheckout(id, () => repoId === id);
  });

  async function setPath(path: string) {
    if (repoId === null) return;
    checkout = await ports.setCheckoutOverride(repoId, path);
  }

  /** The default branch: the live list's, else the mirror's (#2243), `null` while neither has said. */
  const defaultBranch = $derived<BranchOption | null>(
    branches?.find((option) => option.isDefault) ??
      (branches === null && mirrorDefault(detail) !== null
        ? { name: mirrorDefault(detail)!, entityId: null, isDefault: true }
        : null),
  );
  /** The branch picked this session, while the list has not answered or still holds it. */
  const picked = $derived.by((): BranchOption | null => {
    const held = repoId === null ? undefined : chosen[repoId];
    if (held === undefined) return null;
    if (branches === null) return { name: held.name, entityId: held.entityId, isDefault: false };
    return branches.find((option) => option.name === held.name) ?? null;
  });
  const selected = $derived(picked ?? defaultBranch);
  /**
   * The entity every tab reads at: the repository itself on its default
   * branch (or before the branches answer none), the branch on any other.
   */
  const at = $derived(repoId === null ? null : (selected && !selected.isDefault ? selected.entityId : null) ?? repoId);
  const branchName = $derived(selected?.name ?? null);
  const changed = $derived(changedPaths(local));
  const sentence = $derived(checkoutSentence(checkout));
  const tab = $derived<RepoTabId>(litTab(route.tab));

  /** The tab bodies held for the visit (#2248). */
  let tabs = $state(NO_TABS);
  const slot = $derived(slotOf(route));
  $effect.pre(() => {
    const now = rest;
    const reading = at;
    if (branches === null) return;
    tabs = visit(untrack(() => tabs), now, reading);
  });
  /** The tabs held for this repository: none while the page has not noted the one it now names. */
  const held = $derived(tabs.repoId === repoId ? tabs : NO_TABS);
  /** Each slot drawn, in a fixed order so a switch moves no node: the held ones and the one shown. */
  const drawn = $derived(SLOTS.filter((one) => one === slot || held.places[one] !== undefined));
  /** The address a slot is drawn at: the page's for the one shown, the one it was left at for the rest. */
  function placeOf(one: TabSlot) {
    const left = held.places[one];
    return one === slot || left === undefined ? route : readRepoRoute(left);
  }

  const openPrs = $derived(prs?.filter((row) => row.state?.text.toLowerCase() === "open").length ?? 0);
  const localCount = $derived(
    checkout?.tree?.status === "read" ? checkout.tree.changed + checkout.tree.untracked : 0,
  );

  /** A tab's count, drawn only above zero once its read answered. */
  function countOf(id: RepoTabId): { n: number; warn: boolean } | null {
    if (id === "branches" && branches !== null && branches.length > 0) return { n: branches.length, warn: false };
    if (id === "prs" && openPrs > 0) return { n: openPrs, warn: false };
    if (id === "local" && localCount > 0) return { n: localCount, warn: true };
    return null;
  }

  /** What *New pull request* proposes: the branch picked, or the newest other one. */
  const proposed = $derived(
    selected && !selected.isDefault
      ? selected.name
      : (branches?.find((option) => !option.isDefault)?.name ?? branchName),
  );

  function pick(event: Event) {
    if (repoId === null) return;
    const name = (event.currentTarget as HTMLSelectElement).value;
    const option = branches?.find((candidate) => candidate.name === name);
    chosen[repoId] = { name, entityId: option?.entityId ?? null };
  }

  const NOT_BUILT: Partial<Record<RepoTab, string>> = {};

  /** *New branch…*'s dialog, open from the header or from a commit. */
  let newBranch = $state<{ from: ChoiceOption | null } | null>(null);
</script>

<div class="tf-full">
  <nav class="tf-crumbs" aria-label="Breadcrumb">
    {#if job !== null}
      <a class="tf-jobchip tf-card" href={jobHash(jobAt ?? job)}>{job}</a><span class="tf-sep">›</span>
    {/if}
    <a class="tf-crumb" href="#/code">Code</a><span class="tf-sep">›</span>
    <a class="tf-crumb" href="#/code/repos">Repositories</a><span class="tf-sep">›</span>
    <strong>{repo?.title ?? ""}</strong>
    <span class="tf-esc">{job !== null ? `Back to ${job}` : "Back"} <kbd>Esc</kbd></span>
  </nav>

  {#if repoId === null || failed !== null}
    <div class="tf-cbody">
      <p class="tf-cfail">{failed ?? "No repository is named in the address."}</p>
    </div>
  {:else}
    <header class="tf-fhead">
      <div>
        <h1 class="tf-mono">{repo?.title ?? ""}</h1>
        <div class="tf-sub">
          {#if repo !== null}{headLine(facts, repo.updated_at, now)}{/if}
          {#if sentence !== null && sentence.path !== null}
            Cloned at <span class="tf-mono">{sentence.path}</span>{#if sentence.state}, {sentence.state}{/if}.
          {:else if sentence !== null}
            Not on this machine yet.
          {/if}
        </div>
      </div>
      <!-- #2238's order, as the mockup (#2252) draws it: the primary, the others, Open in, Refresh last. -->
      <div class="tf-acts">
        {#if proposed !== null}
          <a class="tf-btn tf-primary" href={repoHash(repoId, "compare", encodeURIComponent(proposed))}>New pull request</a>
        {/if}
        <label class="tf-bsel"
          ><svg viewBox="0 0 16 16" aria-hidden="true"
            ><circle cx="4.5" cy="3.5" r="1.5" /><circle cx="4.5" cy="12.5" r="1.5" /><circle cx="11.5" cy="5" r="1.5" /><path
              d="M4.5 5v6M11.5 6.5c0 3-7 2-7 4.5"
            /></svg
          ><select aria-label="Branch" value={branchName ?? ""} onchange={pick} disabled={branches === null || branches.length === 0}>
            {#each branches ?? [] as option (option.name)}
              <option value={option.name}>{option.name}</option>
            {/each}
          </select></label
        >
        <button
          type="button"
          class="tf-btn"
          disabled={branches === null || branches.length === 0}
          aria-disabled={branchRefused !== null ? "true" : undefined}
          title={branchRefused ?? undefined}
          onclick={() => {
            if (branchRefused === null) newBranch = { from: null };
          }}>New branch…</button>
        <TfOpenIn entityId={repoId} title={repo?.title ?? ""} {checkout} size="md" />
        {#if detail !== null}<TfDetailRefresh {detail} {refresh} {reread} page />{/if}
      </div>
    </header>
    {#if detail !== null}<TfDetailBanners {detail} failed={refresh.failed} />{/if}
    <nav class="tf-tabs" aria-label="Repository">
      {#each REPO_TABS as entry, n (entry.id)}
        {@const count = countOf(entry.id)}
        {#if n > 0 && REPO_TABS[n - 1]!.group !== entry.group}<span class="tf-tabgrp" aria-hidden="true"></span>{/if}
        <a href={tabHref(held, repoId, entry.id)} aria-current={tab === entry.id ? "page" : undefined}
          >{entry.label}{#if count !== null}{" "}<span class="tf-count" class:tf-warn={count.warn}>{count.n}</span
            >{/if}</a
        >
      {/each}
    </nav>
    <div class="tf-cbody">
      {#if NOT_BUILT[route.tab] !== undefined}
        <div class="tf-empty"><p>{NOT_BUILT[route.tab]} is not built in the new shell yet.</p></div>
      {:else}
        {#each drawn as one (one)}
          {@const refreshed = held.reads[one] ?? 0}
          {#if one === "local"}
            <TfRepoTab shown={one === slot}>
              <TfRepoLocal {repoId} title={repo?.title ?? ""} {checkout} readDiff={ports.entityCheckoutDiff} {setPath} />
            </TfRepoTab>
          {:else if at !== null && branchName !== null}
            <!-- The rest wait for the branch every tab reads at: the mirror's default, or the list's (#2243). -->
            <TfRepoTab shown={one === slot}>
              {#if one === "files"}
                <TfRepoFiles
                  {ports}
                  {repoId}
                  {at}
                  branch={branchName}
                  path={placeOf(one).path}
                  line={placeOf(one).line}
                  {changed}
                  editing={placeOf(one).tab === "edit"}
                  {job}
                  checkoutBranch={checkout?.tree?.status === "read" ? checkout.tree.branch : null}
                  {checkout}
                  title={repo?.title ?? ""}
                  {source}
                  {refreshed}
                  refusal={commitRefused}
                />
              {:else if one === "commit" && placeOf(one).commit !== null}
                <TfRepoCommit {ports} {repoId} commitId={placeOf(one).commit ?? ""} {now} {refreshed} onNewBranch={(from) => (newBranch = { from })} />
              {:else if one === "branches"}
                <TfRepoBranches {ports} {repoId} {source} {refreshed} />
              {:else if one === "compare" && branches !== null}
                <TfRepoCompare
                  {ports}
                  {repoId}
                  {source}
                  branches={branches}
                  head={placeOf(one).branch ?? proposed}
                  defaultBranch={defaultBranch?.name ?? null}
                  {refreshed}
                />
              {:else if one === "commits"}
                <TfRepoCommits {ports} {repoId} {at} branch={branchName} defaultBranch={defaultBranch?.name ?? null} {refreshed} />
              {:else if one === "prs"}
                <TfRepoPrs {ports} {repoId} rows={prs} {job} />
              {/if}
            </TfRepoTab>
          {/if}
        {/each}
      {/if}
    </div>
  {/if}
</div>

{#if newBranch !== null && repoId !== null}
  <TfNewBranch
    {repoId}
    repoName={repo?.title ?? repoId}
    {source}
    branches={(branches ?? []).map((option) => option.name)}
    from={newBranch.from}
    {job}
    {ports}
    onclose={() => (newBranch = null)}
  />
{/if}
