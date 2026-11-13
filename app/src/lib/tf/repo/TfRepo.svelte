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

  **A kept page sees new branches** (the third smoothness pass of PR #2479,
  ADR-0042 as amended): the branch list and the pull requests are read again,
  over what is drawn, when the page is shown again, another tab is shown, the
  window comes back after a while, a branch made here lands or a run of the
  source ends. A branch the source lists and the mirror has not synced is
  offered at the id the source named (`unsynced`); one this session made is
  offered at once (`withMade`), not to read at until a read lists it.

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
    LocalDir,
    LocalPaths,
    LocalTree,
    PipeRun,
    PipeStep,
    RangeDiff,
    RefusedWrite,
    ShownView,
    SourceView,
    ViewBlock,
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
    /** A page's view (ADR-0042): what knobas last read, with when (`held_at`) and the rows the mirror lacks (`unsynced`). */
    entityView: (entityId: string, section: string | null) => Promise<ShownView | SourceView | null>;
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
    /** A diff between two ends, local-first (ADR-0036): Compare's; the source's compare alone when left out. */
    rangeDiff?: (entityId: string, base: string, head: string, ignoreWhitespace: boolean) => Promise<RangeDiff>;
    /** One commit's diff, local-first (#2319): the commit tab's; the commit's own `diff` section when left out. */
    commitDiff?: (entityId: string, commitId: string, sha: string, ignoreWhitespace: boolean) => Promise<RangeDiff>;
    /** The whole tree at the checkout, at the mirror's tip (ADR-0041): what *Files* draws first; the source alone when left out. */
    localTree?: (entityId: string) => Promise<LocalTree | null>;
    /** One directory of the checkout's tree (the big-folder pass): `commit` `null` reads at the mirror's tip. */
    localDir?: (entityId: string, commit: string | null, dir: string) => Promise<LocalDir | null>;
    /** Every file of the checkout's tree at `commit`, for *Go to file*. */
    localPaths?: (entityId: string, commit: string) => Promise<LocalPaths | null>;
    /** A file of that tree, read at the checkout (ADR-0041). */
    localFile?: (entityId: string, commit: string, path: string) => Promise<ViewBlock | null>;
    /** The source's compare now, never local (ADR-0041): what Compare checks a predicted diff against; no check when left out. */
    sourceRangeDiff?: (entityId: string, base: string, head: string) => Promise<RangeDiff>;
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

  /** Forget which repositories' headers have looked for a branch to propose: a new session. */
  export function forgetHeaderSearches(): void {
    searched.clear();
  }

  /**
   * The repositories whose header has looked for a branch to propose this session. On a repository's first look, while
   * Files is shown and has not drawn its tree, the look waits for it, at most {@link FIRST_SEARCH_AFTER_MS}: the branch
   * list now comes from the mirror at once, and the look's branch views (each a compare on the forge) went out beside
   * the tree's own first read and slowed it on a repository with no clone (the inner-pages pass, 2026-11-12). The
   * button's room is held meanwhile, as while it looks.
   */
  const searched = new Set<string>();
  const FIRST_SEARCH_AFTER_MS = 1000;
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { EVENTS, ipcErrorMessage, isIpcError } from "../../ipc";
  import {
    codeFacts,
    entityCheckout,
    entityCheckoutDiff,
    entityFileBytes,
    entityFilePiped,
    entityRangeDiff,
    entityCommitDiff,
    entityLocalFile,
    entityLocalTree,
    entityLocalDir,
    entityLocalPaths,
    entitySourceRangeDiff,
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
  import type { EscTarget } from "../escape";
  import {
    branchOptions,
    changedPaths,
    checkoutSentence,
    headLine,
    branchStanding,
    knowsRepoTab,
    litTab,
    MADE_PR,
    readAllRows,
    mirrorDefault,
    readRepoRoute,
    repoAsks,
    repoHash,
    repoPrs,
    repoRefusals,
    sourceName,
    REPO_TABS,
    MADE,
    REPO_BRANCHES,
    withMade,
    type BranchOption,
    type BranchStanding,
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
  import TfCrumbs, { type Crumb } from "../TfCrumbs.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import TfDetailRefresh from "../TfDetailRefresh.svelte";
  import { createDetailRefresh } from "../refresh.svelte";
  import { provideTabRereads } from "./refreshed.svelte";
  import { NO_TABS, readShown, SLOTS, slotOf, tabHref, visit, type TabSlot } from "./repo-tabs";
  import TfRepoTab from "./TfRepoTab.svelte";
  import { parentPage } from "../page-name.svelte";
  import { itemHash } from "../job/job";
  import { jumpAcross } from "../opener";
  import { echoesOn, settleEchoes, watchEchoes } from "../queued.svelte";
  import { landInPlace } from "../land-in-place";
  import { HELD_AFTER_MS } from "./view-fresh.svelte";
  import type { SourceSyncStatus } from "../../ipc/sources";
  import { providePageVisible } from "../kept-pages.svelte";
  import { onFocusWhileShown, onShownAgain } from "../while-shown.svelte";

  let {
    ready,
    rest,
    job,
    back = null,
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
      rangeDiff: (entityId, base, head, ignoreWhitespace) => entityRangeDiff(entityId, base, head, ignoreWhitespace),
      commitDiff: (entityId, commitId, sha, ignoreWhitespace) => entityCommitDiff(entityId, commitId, sha, ignoreWhitespace),
      localTree: (entityId) => entityLocalTree(entityId),
      localDir: (entityId, commit, dir) => entityLocalDir(entityId, commit, dir),
      localPaths: (entityId, commit) => entityLocalPaths(entityId, commit),
      localFile: (entityId, commit, path) => entityLocalFile(entityId, commit, path),
      sourceRangeDiff: (entityId, base, head) => entitySourceRangeDiff(entityId, base, head),
    },
    now: fixedNow,
  }: {
    /** Whether the database answers yet: the reads wait for it. */
    ready: boolean;
    /** What follows `repo` in the address. */
    rest: string[];
    /** The chosen job (spec #2227), as a ticket key. */
    job: string | null;
    /** Where `Esc` goes, which the breadcrumb's hint names; `null` where it does nothing (#2228). */
    back?: EscTarget | null;
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
  /** The read failed because no such repository is held, not because the read broke. */
  let missing = $state(false);
  /** Bumped by *Try again*. */
  let attempt = $state(0);
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
  /**
   * Bumped by a refresh and by the page being shown again, so the toolbar's
   * writes are asked about again: an ask the app could not read is answered
   * disabled and kept nowhere (#2306), and only a new ask clears it.
   */
  let writesAsked = $state(0);
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

  /** Whether Files has drawn its tree on this repository yet, which the header's first look waits for. */
  let treeDrew = drewSignal();
  function drewSignal() {
    let resolve!: () => void;
    const promise = new Promise<void>((r) => (resolve = r));
    const signal = { done: false, promise, tell: () => {} };
    signal.tell = () => {
      signal.done = true;
      resolve();
    };
    return signal;
  }

  $effect(() => {
    if (!ready || repoId === null) return;
    void attempt;
    const id = repoId;
    let live = true;
    const alive = () => live;
    treeDrew = drewSignal();
    detail = null;
    failed = null;
    missing = false;
    branches = null;
    branchesFailed = false;
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
  // The window back after a while: the lists read again too. A moment away reads nothing more.
  $effect(() => {
    if (!visible) return;
    const back = () => {
      if (Date.now() - listsAskedAt >= HELD_AFTER_MS) untrack(rereadLists);
    };
    window.addEventListener("focus", back);
    return () => window.removeEventListener("focus", back);
  });
  // The open tab reads again too, over what it drew, as it does on a refresh; and the page's own lists, its branches
  // and pull requests, so a branch made while the page was away is known to Compare and *New pull request*.
  onShownAgain(
    () => visible,
    () => {
      tabs = readShown(tabs);
      writesAsked += 1;
      rereadLists();
    },
  );

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
    await Promise.all([factsRead, readBranches(id, alive, again), readRow(id, alive, again)]);
  }

  /** Each branch list read, so only the newest answer lands and `branchesReading` holds until it has. */
  let branchRead = 0;
  /** A read of the branch list is under way: Compare does not say a branch is missing while it is. */
  let branchesReading = $state(false);
  /** The first read of the branch list failed: the tabs read at the mirror's default branch, as before it answered. */
  let branchesFailed = $state(false);
  /** When the lists were last asked for, so a window coming back into focus reads them again only after a while. */
  let listsAskedAt = 0;

  /**
   * The branch list (the third smoothness pass of PR #2479): read at once
   * from what knobas last read, which the app then reads again behind it
   * (ADR-0042), and read again whenever the page is shown again, a tab is
   * switched, the window comes back after a while, a branch knobas made
   * lands or a run of the repository's source ends. A branch the source
   * lists and the mirror has not synced yet is offered with the id the
   * source named (`unsynced`). Each branch this session made that the list
   * now holds settles its echo. An empty repository has no branches; a
   * source that cannot say reads as one with only its default branch, which
   * the repository's own view is. Read again, it lands without moving what
   * the reader is on (`landInPlace`).
   */
  function readBranches(id: string, alive: () => boolean, again: boolean): Promise<void> {
    const mine = ++branchRead;
    branchesReading = true;
    listsAskedAt = Date.now();
    const newest = () => alive() && mine === branchRead;
    return ports.entityView(id, "branches").then(
      (view) => {
        if (!newest()) return;
        const next = branchOptions(view?.sections[0] ?? null, (view as ShownView | null)?.unsynced ?? []);
        settleEchoes(id, (echo) => echo.kind === MADE && next.some((option) => option.name === echo.text));
        if (again) void landInPlace(page, () => (branches = next));
        else branches = next;
        branchesFailed = false;
        branchesReading = false;
      },
      () => {
        if (!newest()) return;
        if (!again) {
          branches = [];
          branchesFailed = true;
        }
        branchesReading = false;
      },
    );
  }

  /** The repository's lists again over what the page drew: its branches, and its pull requests with its row. */
  function rereadLists(): void {
    const id = repoId;
    if (!ready || id === null) return;
    const alive = () => repoId === id;
    void readBranches(id, alive, true);
    void readRow(id, alive, true);
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
      if (!live || fresh.entity_id !== id || fresh.section !== "branches" || fresh.failed) return;
      void readBranches(id, () => live && repoId === id, true);
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

  // A branch made here (*New branch…*, a commit to a new branch) lands, or its source's run ends: the list reads again.
  $effect(() => {
    if (!ready || repoId === null) return;
    const id = repoId;
    return untrack(() => watchEchoes(REPO_BRANCHES, () => readBranches(id, () => repoId === id, true), { listen: ports.listen }));
  });

  // A run of the repository's own source ended (a scheduled sync, *Start work*'s branch landing, a push the run brought in):
  // the branches and the pull requests read again, over what the page drew.
  const sourceId = $derived(detail?.row.source_id ?? null);
  $effect(() => {
    const from = sourceId;
    if (!ready || from === null || !visible) return;
    const listen = untrack(() => ports.listen) ?? ((event, handler) => tauriListen(event, handler));
    let live = true;
    let stop: (() => void) | null = null;
    void listen(EVENTS.syncState, ({ payload }) => {
      const row = payload as SourceSyncStatus;
      if (live && !row.running && row.source_id === from) untrack(rereadLists);
    }).then(
      (unlisten) => {
        if (live) stop = unlisten;
        else unlisten();
      },
      () => {},
    );
    return () => {
      live = false;
      stop?.();
    };
  });

  // Another tab of the page shown: the lists read again, so Compare and *New pull request* know a branch made since.
  let slotWas: string | null = null;
  $effect(() => {
    const now = slot;
    const was = slotWas;
    slotWas = now;
    if (was !== null && was !== now) untrack(rereadLists);
  });

  // Whether the source offers *New branch…* and a commit onto the branch is
  // its answer to `refused_writes` (ADR-0020 rule 4, #2241), asked together;
  // one that does not answer refuses nothing. Asked again on a refresh and
  // on being shown again (`writesAsked`).
  $effect(() => {
    if (!ready || repoId === null || at === null) return;
    void writesAsked;
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
      if (alive() && !again) {
        failed = ipcErrorMessage(error);
        missing = isIpcError(error) && error.code === "not_found";
      }
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
      // Its own pull requests, not its source's every one (the query audit, 2026-11-12: 1,710 rows in four reads,
      // 700 ms, for the repository's few dozen); `repoPrs` still keeps the rows whose path line is this repository.
      parent: found.row.title,
    };
    await readAllRows((offset) => ports.listEntities(filter, LIMIT, offset)).then(
      (rows) => {
        if (!alive()) return;
        const next = repoPrs(rows, found.row);
        if (again) void landInPlace(page, () => (prs = next));
        else prs = next;
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
    writesAsked += 1;
    await Promise.all([readRepo(id, () => repoId === id, true), tabRereads.settled()]);
  }

  /** The page's element: where a list read again keeps the reader's place (`landInPlace`). */
  let page = $state<HTMLElement | undefined>(undefined);

  /** The pull requests again, for one this session queued. */
  function rereadPrs(): void {
    const id = repoId;
    if (id !== null) void readRow(id, () => repoId === id, true);
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

  /**
   * The branches the page offers: the list's, then each one this session made that no read holds yet (`withMade`), so a
   * branch knobas made is in the header, Compare and *New pull request* at once. `null` while the list has not answered.
   */
  const listed = $derived(branches === null || repoId === null ? branches : withMade(branches, echoesOn(repoId, MADE)));
  /**
   * With the mirror's default first where the list names none: a source cuts a listing of thousands of branches at a
   * page of the newest (Gitea's 1,000), and a default branch older than all of them was in no list, so no tab drew at
   * all (the extremes pass, 2026-11-13: 5,000 branches, `main` at the root commit).
   */
  const known = $derived(
    listed === null || listed.some((option) => option.isDefault) || mirrorDefault(detail) === null
      ? listed
      : [{ name: mirrorDefault(detail)!, entityId: null, isDefault: true }, ...listed.filter((option) => option.name !== mirrorDefault(detail))],
  );
  /**
   * The default branch: the live list's, else the mirror's (#2243), `null` while neither has said. A list that failed, or
   * one that names no default, falls back to the mirror's too: with the source down the page drew no tab at all (the
   * inner-pages pass, 2026-11-12).
   */
  const defaultBranch = $derived<BranchOption | null>(
    known?.find((option) => option.isDefault) ??
      ((branches === null || branchesFailed) && mirrorDefault(detail) !== null
        ? { name: mirrorDefault(detail)!, entityId: null, isDefault: true }
        : null),
  );
  /** The branch picked this session, while the list has not answered or still holds it. */
  const picked = $derived.by((): BranchOption | null => {
    const held = repoId === null ? undefined : chosen[repoId];
    if (held === undefined) return null;
    if (known === null) return { name: held.name, entityId: held.entityId, isDefault: false };
    return known.find((option) => option.name === held.name && (option.entityId !== null || option.isDefault)) ?? null;
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
  /** The line under the title in words, for its tooltip: one line on the page, a long clone path cut. */
  const subTitle = $derived(
    [
      repo === null ? "" : headLine(facts, repo.updated_at, now),
      sentence === null ? "" : sentence.path !== null ? `Cloned at ${sentence.path}${sentence.state ? `, ${sentence.state}` : ""}.` : "Not on this machine yet.",
    ]
      .filter((part) => part !== "")
      .join(" "),
  );
  const tab = $derived<RepoTabId>(litTab(route.tab));
  /** The address names a tab the page has; one it does not have is said under the tabs, not drawn as Files. */
  const tabKnown = $derived(knowsRepoTab(rest));

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
    if (id === "branches" && known !== null && known.length > 0) return { n: known.length, warn: false };
    // One on its way counts as its row does, so the tab agrees with the list under it (F-242).
    const onTheWay = id === "prs" && repoId !== null ? echoesOn(repoId, MADE_PR).length : 0;
    if (id === "prs" && openPrs + onTheWay > 0) return { n: openPrs + onTheWay, warn: false };
    if (id === "local" && localCount > 0) return { n: localCount, warn: true };
    return null;
  }

  /** The branches a pull request was sent from this session, which *New pull request* doesn't start on again (F-257). */
  const sentFrom = $derived(new Set(repoId === null ? [] : echoesOn(repoId, MADE_PR).map((echo) => echo.base)));
  /** What *New pull request* proposes: the branch picked, or the newest other one, past those a pull request was just sent from. */
  const proposed = $derived(
    selected && !selected.isDefault
      ? selected.name
      : (known?.find((option) => !option.isDefault && !sentFrom.has(option.name))?.name ?? known?.find((option) => !option.isDefault)?.name ?? branchName),
  );

  /**
   * The branch Compare at an address naming none draws (`compare` alone): what the page proposed when it first drew there,
   * held for that address and branch read at, so a list read again that brings a newer branch does not change *From* under
   * the reader (the third smoothness pass of PR #2479). Proposed again once the branch held is gone from the list.
   */
  let compareHeld: { key: string; head: string | null } | null = null;
  const compareProposed = $derived.by(() => {
    const key = `${(held.places.compare ?? []).join("\u0000")}\u0001${at ?? ""}`;
    const kept = compareHeld;
    if (kept !== null && kept.key === key && kept.head !== null && (known ?? []).some((option) => option.name === kept.head)) return kept.head;
    compareHeld = { key, head: proposed };
    return proposed;
  });

  /** How many branches the header asks before it gives up on finding one with nothing open: a source's burst stays small. */
  const ASKED = 8;
  /** How many of them are read at once, as the Branches tab reads them: one at a time took 2.5 s on a live forge. */
  const ASKED_AT_ONCE = 4;
  /**
   * The header is still asking which branch *New pull request* starts on. Its room stays, unseen, until the answer: on a live
   * forge the button read *New pull request* for 2.5 s and then became *Open PR #144* under the pointer (the live run,
   * 2026-11-09), so a press in that time started a pull request the page then said it would not.
   */
  let seeking = $state(false);
  /** The branch *New pull request* starts on, once the header has found one with nothing open from it (F-243). */
  let toPropose = $state<string | null>(null);
  /** Where every branch it could start on has a pull request open: the loudest button goes there, as a form saying so offers nothing to create. */
  let openedFrom = $state<BranchStanding["pr"]>(null);
  $effect(() => {
    const choice = selected !== null && !selected.isDefault ? selected : null;
    const asking = (choice === null ? (known ?? []).filter((option) => !option.isDefault && !sentFrom.has(option.name)) : [choice]).flatMap((option) => (option.entityId === null ? [] : [{ name: option.name, id: option.entityId }]));
    void writesAsked;
    let live = true;
    const asked = ready ? asking.slice(0, ASKED) : [];
    // Before the branches are read the page knows no branch to start on: *main* drew for half a second, then went.
    untrack(() => (seeking = asked.length > 0 || (ready && choice === null && branches === null)));
    void (async () => {
      const id = untrack(() => repoId);
      if (asked.length > 0 && id !== null && !searched.has(id)) {
        searched.add(id);
        if (untrack(() => slot) === "files" && !treeDrew.done) await Promise.race([treeDrew.promise, new Promise((resolve) => setTimeout(resolve, FIRST_SEARCH_AFTER_MS))]);
        if (!live) {
          searched.delete(id);
          return;
        }
      }
      let first: BranchStanding["pr"] = null;
      // In order, a batch at a time: the first branch in order that stands free wins, as when they were read one by one.
      for (let at = 0; at < asked.length; at += ASKED_AT_ONCE) {
        const batch = asked.slice(at, at + ASKED_AT_ONCE);
        const standings = await Promise.all(batch.map(({ id }) => ports.entityView(id, null).then(branchStanding, () => null)));
        if (!live) return;
        for (const [i, standing] of standings.entries()) {
          // A branch that can't be read says nothing about what stands: *proposed* stays, as it was before anything was asked.
          if (standing === null) {
            toPropose = null;
            openedFrom = first;
            seeking = false;
            return;
          }
          if (standing.pr === null && (choice !== null || standing.canOpenPr)) {
            toPropose = batch[i]!.name;
            openedFrom = null;
            seeking = false;
            return;
          }
          first ??= standing.pr;
        }
      }
      toPropose = null;
      openedFrom = first;
      seeking = false;
    })();
    return () => {
      live = false;
    };
  });

  /** *New pull request*'s branch: the header's search's while it stands, else the plain proposal. */
  const starting = $derived(toPropose !== null && !sentFrom.has(toPropose) ? toPropose : proposed);
  /**
   * A pull request sent from here that no read holds yet, with nothing else to start: the primary stays out, as it would otherwise name
   * another pull request right after this one was made, or start this one again (F-263). A branch picked by hand still says what stands on it.
   */
  const sentWaits = $derived(sentFrom.size > 0 && (selected === null || selected.isDefault) && (toPropose === null || sentFrom.has(toPropose)));
  /** What Compare says above its form about the branch this press chose (F-063): nothing, where the reader picked it. */
  let guessed = $state<{ branch: string; why: string } | null>(null);
  /**
   * The header's press goes to another of the page's tabs: an opening, so `Esc` comes back to the tab it was pressed on (F-259, F-065).
   * The page's name is the repository, which Compare is on too, so the bar names the tab.
   */
  function pressNewPr(event: MouseEvent, branch: string) {
    guessed = selected !== null && !selected.isDefault ? null : { branch, why: toPropose === branch ? "the newest branch with nothing open" : "the newest branch" };
    // A commit is a page of its own, which already names itself.
    jumpAcross(event, route.commit !== null ? undefined : REPO_TABS.find((entry) => entry.id === tab)?.label);
  }

  function pick(event: Event) {
    if (repoId === null) return;
    const name = (event.currentTarget as HTMLSelectElement).value;
    const option = known?.find((candidate) => candidate.name === name);
    chosen[repoId] = { name, entityId: option?.entityId ?? null };
  }

  /** A repository holding one branch has nothing to propose from. */
  const oneBranch = $derived(known !== null && known.length === 1);

  const NOT_BUILT: Partial<Record<RepoTab, string>> = {};

  // Its last part is what *Back to <name>* calls this page once it is left (spec #2227).
  const crumbs = $derived.by((): Crumb[] => {
    const code = { name: "Code", href: "#/code" };
    if (route.commit !== null && repoId !== null) {
      return [code, repo === null ? { name: "Repository", href: repoHash(repoId) } : { name: repo.title, href: repoHash(repoId), code: true }, { name: "Commit" }];
    }
    return [code, repo === null ? { name: "Repository" } : { name: repo.title, code: true }];
  });
  // A commit sits under its repository, by the title its crumb gives it.
  parentPage(() => (route.commit !== null && repoId !== null && repo !== null ? { hash: repoHash(repoId), name: repo.title } : null));

  /** *New branch…*'s dialog, open from the header or from a commit. */
  let newBranch = $state<{ from: ChoiceOption | null } | null>(null);

  /** Whether the header's actions sit under its title. A width query can't tell, because the branch's name changes how wide they are. */
  let stacked = $state(false);
  function watchStack(header: HTMLElement) {
    const title = header.firstElementChild;
    const acts = header.querySelector(":scope > .tf-acts");
    if (title === null || acts === null) return;
    const read = () => {
      const box = title.getBoundingClientRect();
      stacked = box.height > 0 && acts.getBoundingClientRect().top >= box.bottom;
    };
    read();
    if (typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(read);
    watch.observe(header);
    watch.observe(acts);
    return () => watch.disconnect();
  }
</script>

<div class="tf-full" bind:this={page}>
  <TfCrumbs parts={crumbs} {back} />

  {#if repoId === null || failed !== null}
    {#if failed !== null && !missing}
      <TfNothingHere message="This repository could not be read: {failed}" onretry={() => attempt++} />
    {:else}
      <TfNothingHere message="knobas does not hold this repository. It may not have synced yet." href="#/code" action="All repositories" />
    {/if}
  {:else}
    <header class="tf-fhead tf-repohead" {@attach watchStack}>
      <div>
        <h1 class="tf-mono">{repo?.title ?? ""}</h1>
        <div class="tf-sub" title={subTitle || undefined}>
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
        <!-- Compare is this form, with a primary and *Into* and *From* of its own, so these would say it twice. They keep their room,
             hidden and out of the Tab order, because a shorter row lets the tabs under it jump between tabs (F-249). Where the row
             stacks under the title it starts at the page's edge, so the hidden room goes after what shows (F-256). -->
        <span class="tf-repohold" class:tf-held={tab === "compare"} class:tf-stacked={stacked} inert={tab === "compare"}>
          {#if oneBranch}
            <button type="button" class="tf-btn tf-primary" aria-disabled="true" title="This repository has one branch: make another first">New pull request</button>
          {:else if sentWaits || seeking}
            <!-- Its room, unseen, so the tabs under the header hold still until the read brings it back (F-249), or until the
                 header knows which branch it starts on (`seeking`). -->
            <span class="tf-btn tf-primary tf-repoheld" aria-hidden="true">New pull request</span>
          {:else if openedFrom?.entityId}
            <a class="tf-btn tf-primary" href={itemHash({ entity_id: openedFrom.entityId, kind: "pr" })}>Open PR {openedFrom.number ?? ""}</a>
          {:else if starting !== null}
            {@const href = repoHash(repoId, "compare", encodeURIComponent(starting))}
            <a class="tf-btn tf-primary" {href} onclick={(event) => pressNewPr(event, starting)}>New pull request</a>
          {/if}
          <label class="tf-bsel"
            ><svg viewBox="0 0 16 16" aria-hidden="true"
              ><circle cx="4.5" cy="3.5" r="1.5" /><circle cx="4.5" cy="12.5" r="1.5" /><circle cx="11.5" cy="5" r="1.5" /><path
                d="M4.5 5v6M11.5 6.5c0 3-7 2-7 4.5"
              /></svg
            ><select aria-label="Branch" value={branchName ?? ""} onchange={pick} disabled={known === null || known.length === 0}>
              {#each known ?? [] as option (option.name)}
                <!-- A branch knobas is making has nothing to read at until a read lists it: offered, but not to read at yet. -->
                <option value={option.name} disabled={option.made !== undefined} title={option.made !== undefined ? `Being made on ${sourceName(source)}: it can be read once it is there` : undefined}
                  >{option.name}{option.made !== undefined ? ` (${option.made})` : ""}</option
                >
              {/each}
            </select></label
          >
        </span>
        <button
          type="button"
          class="tf-btn"
          disabled={known === null || known.length === 0}
          aria-disabled={branchRefused !== null ? "true" : undefined}
          title={branchRefused ?? undefined}
          onclick={() => {
            if (branchRefused === null) newBranch = { from: null };
          }}>New branch…</button>
        <TfOpenIn entityId={repoId} title={repo?.title ?? ""} {checkout} size="md" variant="ghost" />
        {#if detail !== null}<TfDetailRefresh {detail} {refresh} {reread} page />{/if}
      </div>
    </header>
    {#if detail !== null}<TfDetailBanners {detail} {refresh} {reread} />{/if}
    <nav class="tf-tabs" aria-label="Repository">
      {#each REPO_TABS as entry, n (entry.id)}
        {@const count = countOf(entry.id)}
        {#if n > 0 && REPO_TABS[n - 1]!.group !== entry.group}<span class="tf-tabgrp" aria-hidden="true"></span>{/if}
        <a href={tabHref(held, repoId, entry.id)} aria-current={tabKnown && tab === entry.id ? "page" : undefined}
          >{entry.label}{#if count !== null}{" "}<span class="tf-count" class:tf-warn={count.warn}>{count.n}</span
            >{/if}</a
        >
      {/each}
    </nav>
    {#if !tabKnown}
      <TfNothingHere message="This repository has no such tab." href={repoHash(repoId)} action="Files" />
    {:else}
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
                  treeSections={detail?.tree ?? null}
                  ondrew={() => treeDrew.tell()}
                />
              {:else if one === "commit" && placeOf(one).commit !== null}
                <TfRepoCommit {ports} {repoId} commitId={placeOf(one).commit ?? ""} {now} {refreshed} onNewBranch={(from) => (newBranch = { from })} />
              {:else if one === "branches"}
                <TfRepoBranches {ports} {repoId} {source} {refreshed} />
              {:else if one === "compare" && known !== null}
                <TfRepoCompare
                  {ports}
                  {repoId}
                  {source}
                  branches={known}
                  {branchesReading}
                  head={placeOf(one).branch ?? compareProposed}
                  base={placeOf(one).base}
                  defaultBranch={defaultBranch?.name ?? null}
                  {refreshed}
                  {branchRefused}
                  shown={one === slot}
                  {guessed}
                  onNewBranch={() => (newBranch = { from: null })}
                />
              {:else if one === "commits"}
                <TfRepoCommits {ports} {repoId} {at} {source} branch={branchName} defaultBranch={defaultBranch?.name ?? null} {refreshed} />
              {:else if one === "prs"}
                <TfRepoPrs {ports} {repoId} rows={prs} {job} {now} newPr={proposed === null || oneBranch ? null : repoHash(repoId, "compare", encodeURIComponent(proposed))} reread={rereadPrs} />
              {/if}
            </TfRepoTab>
          {/if}
        {/each}
      {/if}
    </div>
    {/if}
  {/if}
</div>

{#if newBranch !== null && repoId !== null}
  <TfNewBranch
    {repoId}
    repoName={repo?.title ?? repoId}
    {source}
    branches={(known ?? []).filter((option) => option.made === undefined).map((option) => option.name)}
    from={newBranch.from}
    {job}
    {ports}
    onclose={() => (newBranch = null)}
  />
{/if}
