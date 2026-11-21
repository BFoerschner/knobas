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
  import type { RangeFilePagesRead } from "../diff/fill";
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
    RangeFilePage,
    RangeFiles,
    RangeNames,
    RefusedWrite,
    RepoBuilds,
    BranchPull,
    RunRow,
    ShownView,
    SourceView,
    ViewBlock,
    ViewFresh,
    WriteAsk,
  } from "../../ipc/entity";
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import type { WriteSource } from "../../shell/queued-toast";
  import type { OpenJobPorts } from "../job/open-jobs";
  import type { ArchivePorts } from "../../render/jar-ports";
  import type { DetailRefreshPorts } from "../refresh.svelte";

  /** The reads this page makes, injectable so a test needs no bridge. */
  export interface RepoPorts extends OpenJobPorts {
    getEntity: (entityId: string) => Promise<EntityDetail>;
    listEntities: (filter: EntityFilter, limit: number, offset: number) => Promise<EntityPage>;
    /** A page's view (ADR-0042): what knobas last read, with when (`held_at`) and the rows the mirror lacks (`unsynced`). */
    entityView: (entityId: string, section: string | null) => Promise<ShownView | SourceView | null>;
    /**
     * A view drawn beside the read the page waits on (`entityViewBehind`, the ux16 overs pass): Compare's branch standing,
     * whose requests to the source wait for the compare's own. `entityView` where absent.
     */
    behindView?: (entityId: string, section: string | null) => Promise<ShownView | SourceView | null>;
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
    /** The JAR view's reads (ux19): an archive listed, its entries, a class outlined or decompiled; no JAR view when left out. */
    archive?: ArchivePorts;
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
    /** The Builds tab's read (the repo-builds pass): the pipelines that built the repository's commits; the bridge's when left out. */
    repoPipelines?: (repoId: string) => Promise<RepoBuilds>;
    /** The branches as the mirror holds them, with the pull request open from each (the forge-rounds pass): the header's look; the bridge's when left out. */
    repoBranchPulls?: (repoId: string) => Promise<BranchPull[]>;
    /** A pipeline's runs, for the Builds tab's pipeline; the bridge's when left out. */
    pipelineRuns?: (pipelineId: string) => Promise<RunRow[]>;
    /**
     * A range's files without their lines, read where `rangeDiff` reads (the compare pass, 2026-11-13), under a name
     * `cancelRead` calls it off by; with `rangeFile`, Compare lists a range of thousands of files at once and reads each
     * file's lines as it reaches it. Compare reads the whole diff with `rangeDiff` when left out.
     */
    rangeFiles?: (entityId: string, base: string, head: string, ignoreWhitespace: boolean, read: string) => Promise<RangeFiles>;
    /** One page of one file's lines (the compare pass). */
    rangeFile?: (entityId: string, base: string, head: string, path: string, oldPath: string | null, ignoreWhitespace: boolean, from: number, read: string) => Promise<RangeFilePage>;
    /** The first page of many files in one read (the big-compare pass, 2026-11-16): one start of git for the files near the view. */
    rangeFilePages?: RangeFilePagesRead;
    /** The source's compare now as a list of files (the compare pass): what Compare checks a predicted list against. */
    sourceRangeFiles?: (entityId: string, base: string, head: string, read: string) => Promise<RangeFiles>;
    /** Calls off a named read at the backend (the compare pass). */
    cancelRead?: (read: string) => Promise<boolean>;
    /** A compare's file names at the clone, no counts, `null` where it cannot say (the stored-compare pass, 2026-11-14): drawn while `rangeFiles` reads. */
    rangeNames?: (entityId: string, base: string, head: string) => Promise<RangeNames | null>;
    /**
     * What Compare's build ahead of its press waits for (the compare-first-open pass): the reader still and the window
     * idle (`quietAndIdle`). Under jsdom nothing is built ahead unless a test gives its own.
     */
    aheadWait?: () => Promise<void>;
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

  /** Forget the branches' standings the header's looks read: a new session. */
  export function forgetHeaderLooks(): void {
    standings.clear();
  }

  /** Forget the branches' views the header's looks are reading now: a new session. */
  export function forgetHeaderReads(): void {
    looking.clear();
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

  /**
   * Each branch's standing as the header's look last read it, by the branch's id: a look is up to eight branches' whole
   * views, each some ten reads on the forge, and the page looked again on every visit, every tab shown and every return
   * from Compare (asking then for the branch Compare had picked): 2,179 requests to the forge in one run of the inner
   * pages, most of a forge's quota of ten a second, with a person's own reads waiting behind them, and the button held
   * for seconds on each return (the inner-6x pass, 2026-11-15). A standing is kept {@link STANDING_KEPT_MS} at most, and
   * a refresh drops the repository's; a look whose branches are all kept answers at once. Not dropped when a run of the
   * source ends: looked for again then, the reads went out with the run's own and Commits waited a second for them; a
   * pull request someone opened from the branch since is what Compare says when the button is pressed, and one sent from
   * here takes its branch out of the look (`sentFrom`).
   */
  const standings = new Map<string, { at: number; standing: BranchStanding }>();
  const STANDING_KEPT_MS = 5 * 60_000;
  /**
   * The branches' views a look is reading now, by branch: a look started again before they answer (the branch list and
   * the pull requests landing a moment apart after a run, each starting one) waits for the same read rather than send a
   * second. Before, a run's end read each branch's whole view twice, a branch list and its checks each time (the ux13
   * forge-load pass, 2026-11-15).
   */
  const looking = new Map<string, Promise<BranchStanding | null>>();
</script>

<script lang="ts">
  import { bridgeArchive } from "../../render/jar-ports";
  import { untrack } from "svelte";
  import { EVENTS, ipcErrorMessage, isIpcError } from "../../ipc";
  import {
    codeFacts,
    entityCheckout,
    entityCheckoutDiff,
    entityFileBytes,
    entityFilePiped,
    entityRangeDiff,
    entityRangeFile,
    entityRangeFilePages,
    entityRangeFiles,
    entitySourceRangeFiles,
    entityRangeNames,
    cancelRead,
    entityCommitDiff,
    entityLocalFile,
    entityLocalTree,
    entityLocalDir,
    entityLocalPaths,
    entitySourceRangeDiff,
    entityView,
    entityViewBehind,
    getEntity,
    jobReach,
    listContexts,
    listEntities,
    refusedWrites,
    repoPipelines,
    repoBranchPulls,
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
    workingChanges,
    workingWhy,
    compareHash,
    checkoutSentence,
    headLine,
    branchStanding,
    knowsRepoTab,
    litTab,
    MADE_PR,
    readAllRows,
    mirrorDefault,
    mirrorStanding,
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
    type LookStanding,
    type RepoTab,
    type RepoTabId,
  } from "./repo";
  import TfRepoCommit from "./TfRepoCommit.svelte";
  import TfRepoCommits from "./TfRepoCommits.svelte";
  import TfRepoFiles from "./TfRepoFiles.svelte";
  import TfNewBranch from "./TfNewBranch.svelte";
  import TfRepoBranches from "./TfRepoBranches.svelte";
  import TfRepoCompare from "./TfRepoCompare.svelte";
  import TfRepoCompareWorking from "./TfRepoCompareWorking.svelte";
  import { heardCheckout } from "./cloned-here.svelte";
  import { isWorkingTree, WORKING_TREE } from "../diff/address";
  import { push } from "../../shell/toasts.svelte";
  import { treeStanding } from "../code/code";
  import TfRepoPrs from "./TfRepoPrs.svelte";
  import TfRepoBuilds from "./TfRepoBuilds.svelte";
  import { latestOn, yoursOf } from "./repo-builds";
  import { runTone } from "../builds/builds";
  import TfRepoLocal from "./TfRepoLocal.svelte";
  import TfCloneSign from "./TfCloneSign.svelte";
  import TfOpenIn from "../openin/TfOpenIn.svelte";
  import { clones } from "../openin/clones.svelte";
  import TfDetailBanners from "../TfDetailBanners.svelte";
  import TfCrumbs, { type Crumb } from "../TfCrumbs.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import TfDetailRefresh from "../TfDetailRefresh.svelte";
  import { createDetailRefresh } from "../refresh.svelte";
  import { heldHead } from "../held-head.svelte";
  import { provideTabRereads } from "./refreshed.svelte";
  import { holdSlot, NO_TABS, readShown, SLOTS, slotOf, tabHref, visit, type TabSlot } from "./repo-tabs";
  import TfRepoTab from "./TfRepoTab.svelte";
  import { parentPage } from "../page-name.svelte";
  import { itemHash } from "../job/job";
  import { jumpAcross } from "../opener";
  import { echoesOn, settleEchoes, watchEchoes } from "../queued.svelte";
  import { landInPlace } from "../land-in-place";
  import { HELD_AFTER_MS } from "./view-fresh.svelte";
  import { askCompareEarly, earlyAsked, whenIdle } from "./compare-early";
  import { quietAndIdle } from "../ahead";
  import { holdRepoPage } from "./hover-reads";
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
      behindView: (entityId, section) => entityViewBehind(entityId, section),
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
      archive: bridgeArchive(),
      piped: (entityId, path, commit, steps) => entityFilePiped(entityId, path, commit, steps),
      rangeDiff: (entityId, base, head, ignoreWhitespace) => entityRangeDiff(entityId, base, head, ignoreWhitespace),
      commitDiff: (entityId, commitId, sha, ignoreWhitespace) => entityCommitDiff(entityId, commitId, sha, ignoreWhitespace),
      localTree: (entityId) => entityLocalTree(entityId),
      localDir: (entityId, commit, dir) => entityLocalDir(entityId, commit, dir),
      localPaths: (entityId, commit) => entityLocalPaths(entityId, commit),
      localFile: (entityId, commit, path) => entityLocalFile(entityId, commit, path),
      sourceRangeDiff: (entityId, base, head) => entitySourceRangeDiff(entityId, base, head),
      rangeFiles: (entityId, base, head, ignoreWhitespace, read) => entityRangeFiles(entityId, base, head, ignoreWhitespace, read),
      rangeFile: (entityId, base, head, path, oldPath, ignoreWhitespace, from, read) => entityRangeFile(entityId, base, head, path, oldPath, ignoreWhitespace, from, read),
      rangeFilePages: (entityId, base, head, files, ignoreWhitespace, read) => entityRangeFilePages(entityId, base, head, files, ignoreWhitespace, read),
      sourceRangeFiles: (entityId, base, head, read) => entitySourceRangeFiles(entityId, base, head, read),
      cancelRead: (read) => cancelRead(read),
      rangeNames: (entityId, base, head) => entityRangeNames(entityId, base, head),
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

  /** Whether a read again answered what the page holds: then it writes nothing, and nothing is laid out anew. */
  const sameAnswer = (held: unknown, next: unknown): boolean => held !== null && JSON.stringify(held) === JSON.stringify(next);

  // This page reads its repository's landing for itself: the shell's address change asks it only for a repository no
  // page holds (`readLandingAtAddress`).
  $effect(() => {
    const id = repoId;
    if (id !== null) return holdRepoPage(id);
  });

  /** Two frames on (what was just shown has drawn), then the window idle: when a read behind a press goes out. */
  function afterDrawn(): Promise<void> {
    const frame = () =>
      new Promise<void>((resolve) => {
        if (typeof requestAnimationFrame === "function") requestAnimationFrame(() => resolve());
        else setTimeout(resolve, 16);
      });
    return frame().then(frame).then(whenIdle);
  }

  /** The repository's read: its row, its source, and what its *Refresh* offers (#2135). */
  let detail = $state<EntityDetail | null>(null);
  const repo = $derived<EntityRow | null>(detail?.row ?? null);
  /** What ⌘R says it reads (ux25): the repository's name without its owner, *payout-service*. */
  const shortName = $derived(repo === null ? undefined : (repo.title.slice(repo.title.lastIndexOf("/") + 1) || repo.title));
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
  /** Whether `code_facts` and the checkout's read have answered, held or not: the header's subline and *Open in…* wait on them. */
  let factsAnswered = $state(false);
  let checkoutAnswered = $state(false);
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
    checkoutAnswered = false;
    local = null;
    prs = null;
    facts = undefined;
    factsAnswered = false;
    branchPulls = null;
    untrack(() => refresh.reset());
    readRepo(id, alive, false);
    readCheckout(id, alive);
    return () => {
      live = false;
    };
  });

  /**
   * The header draws once, whole (`held-head.svelte.ts`): its title and *Sync now* are the item's, its subline's facts
   * `code_facts`'s and *Open in…* or *Clone* the checkout's; drawn as each landed, the title came a frame after the header,
   * *Clone* and *Sync now* pushed the branch picker 161 px left, and the facts pushed the clone's sign right. The tabs under
   * it are held with it until their counts have answered: a count landing alone widened its tab and pushed the rest right.
   */
  const drawnHead = heldHead(
    () => repoId,
    () =>
      (detail !== null && factsAnswered && checkoutAnswered && (branches !== null || branchesFailed) && prs !== null && (builds !== null || buildsFailed !== null)) ||
      failed !== null,
  );

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
        if (!alive()) return;
        facts = answer.find((entry) => entry.entity_id === id);
        factsAnswered = true;
      },
      () => {
        if (alive()) factsAnswered = true;
      },
    );
    await Promise.all([factsRead, readBranches(id, alive, again), readRow(id, alive, again), readBranchPulls(id, alive)]);
  }

  /**
   * The branches as the mirror holds them, each with the pull request open from it (`repo_branch_pulls`, the forge-rounds
   * pass, 2026-11-15): what the header's *New pull request* is decided on, so its look reads no branch's view on the forge
   * for a branch the mirror holds. `null` while it has not answered; `"failed"` where it could not, and the look reads
   * each branch's view as before.
   */
  let branchPulls = $state.raw<BranchPull[] | "failed" | null>(null);
  function readBranchPulls(id: string, alive: () => boolean): Promise<void> {
    return (ports.repoBranchPulls ?? repoBranchPulls)(id).then(
      (answer) => {
        if (!alive()) return;
        const next = Array.isArray(answer) ? answer : "failed";
        // The same answer again (a tab switched, the window back): nothing to lay out anew.
        if (!sameAnswer(branchPulls, next)) branchPulls = next;
      },
      () => {
        if (alive() && branchPulls === null) branchPulls = "failed";
      },
    );
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
        // The same list again writes nothing: a tab's switch read it in that tab's first open, and `landInPlace` laid
        // the half-built tab out to land it (the compare-first-open pass, 2026-11-20).
        if (again && sameAnswer(branches, next)) {
          // Nothing moved.
        } else if (again) void landInPlace(page, () => (branches = next));
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
    void readBranchPulls(id, alive);
    readBuilds(id);
  }

  /**
   * The Builds tab's answer (the repo-builds pass): read once for the page, so the tab's count, the tab and the
   * Branches tab's *Builds* column share it. One statement over the mirror, kept by the answer cache; read again with
   * the page's lists, and on the tab's own refresh, over what is drawn.
   */
  let builds = $state.raw<RepoBuilds | null>(null);
  let buildsFailed = $state<string | null>(null);
  let buildsFor: string | null = null;
  let buildsReads = 0;
  /** The repository a read is on its way for: a second ask while it is (a return reads the lists and the tab) is that read. */
  let buildsAsking: string | null = null;
  function readBuilds(id: string): void {
    if (buildsAsking === id) return;
    buildsAsking = id;
    const at = ++buildsReads;
    (ports.repoPipelines ?? repoPipelines)(id)
      .then(
        (answer) => {
          if (at !== buildsReads || repoId !== id) return;
          builds = answer;
          buildsFailed = null;
        },
        (error: unknown) => {
          if (at === buildsReads && repoId === id) buildsFailed = ipcErrorMessage(error);
        },
      )
      .finally(() => {
        if (buildsAsking === id) buildsAsking = null;
      });
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

  // A run of the repository's own source ended with news (a scheduled sync, *Start work*'s branch landing, a push the run
  // brought in): the branches and the pull requests read again, over what the page drew, and the open tab, as a refresh
  // does: Commits left open through a run that mirrored a pushed commit listed what it had drawn 15 s after the run ended
  // (the ux13 Windows pass, 2026-11-15). A run that changed nothing reads nothing: the page left open sent 17 requests to
  // the forge a minute, each run's end reading the branch list and its checks whether the run had found anything or not
  // (the same pass). While the window is hidden (minimised, another space) a run with news is noted and read once on
  // its return.
  const sourceId = $derived(detail?.row.source_id ?? null);
  $effect(() => {
    const from = sourceId;
    if (!ready || from === null || !visible) return;
    const listen = untrack(() => ports.listen) ?? ((event, handler) => tauriListen(event, handler));
    let live = true;
    let stop: (() => void) | null = null;
    let missed = false;
    const news = () => {
      untrack(rereadLists);
      tabs = readShown(untrack(() => tabs));
    };
    void listen(EVENTS.syncState, ({ payload }) => {
      const row = payload as SourceSyncStatus;
      if (!live || row.running || row.source_id !== from || row.last_changed === false) return;
      if (document.visibilityState === "hidden") missed = true;
      else news();
    }).then(
      (unlisten) => {
        if (live) stop = unlisten;
        else unlisten();
      },
      () => {},
    );
    const back = () => {
      if (!live || !missed || document.visibilityState === "hidden") return;
      missed = false;
      news();
    };
    document.addEventListener("visibilitychange", back);
    return () => {
      live = false;
      stop?.();
      document.removeEventListener("visibilitychange", back);
    };
  });

  // Another tab of the page shown: the lists read again, so Compare and *New pull request* know a branch made since. Once
  // the tab has drawn and the window is idle (Fable's ruling, the compare-first-open pass, 2026-11-20): read in the press,
  // the four answers landed inside the tab's first open, each laying out the half-built tab, 31 ms of Compare's 200 at 6x.
  let slotWas: string | null = null;
  $effect(() => {
    const now = slot;
    const was = slotWas;
    slotWas = now;
    if (was === null || was === now) return;
    let live = true;
    void afterDrawn().then(() => {
      if (live) untrack(rereadLists);
    });
    return () => {
      live = false;
    };
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
    if (!(again && sameAnswer(detail, found))) detail = found;
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
        if (again && sameAnswer(prs, next)) return;
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
    for (const branch of standings.keys()) if (branch.startsWith(`${id}@`)) standings.delete(branch);
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
        checkoutAnswered = true;
        heardCheckout(id, view.path);
        const tree = view.tree;
        if (tree?.status !== "read" || tree.changed + tree.untracked === 0) {
          local = null;
          return;
        }
        ports.entityCheckoutDiff(id).then(
          (read) => {
            if (alive()) local = read;
          },
          // A list that could not be read is answered too: the working tree's Files draws `HEAD` with no marks rather
          // than wait for ever (2026-11-21).
          (error: unknown) => {
            if (alive()) local = { status: "failed", reason: ipcErrorMessage(error) };
          },
        );
      },
      () => {
        if (alive()) checkoutAnswered = true;
      },
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
    if (held === undefined || held.name === WORKING_TREE) return null;
    if (known === null) return { name: held.name, entityId: held.entityId, isDefault: false };
    return known.find((option) => option.name === held.name && (option.entityId !== null || option.isDefault)) ?? null;
  });
  // ---- The working tree in the branch box (2026-11-21, `docs/decisions/2026-11-21-the-working-tree-is-a-choice-in-the-branch-box.md`;
  // Björn: "I would like to be able to select the current work tree in the branch selection, not just the branches") ----
  /** Why the working tree can't be chosen now, `null` where it can. */
  const diskWhy = $derived(workingWhy(checkout));
  /** The working tree is this session's choice for the repository. */
  const wantsDisk = $derived(repoId !== null && chosen[repoId]?.name === WORKING_TREE);
  /** The working tree is chosen and can be read. */
  const onDisk = $derived(wantsDisk && diskWhy === null);
  const diskTree = $derived(onDisk && checkout?.tree?.status === "read" ? checkout.tree : null);
  /** The checkout's branch among the branches: what Commits, *History* and *New pull request* use on the working tree. */
  const diskBranch = $derived(diskTree?.branch == null ? null : (known?.find((option) => option.name === diskTree.branch) ?? null));
  /**
   * What Files lays over the checkout's `HEAD`: git's uncommitted list (ADR-0026), held until it has answered, so the tree
   * draws once. A clean tree has no list to wait for (`readCheckout` reads none).
   */
  const disk = $derived(
    diskTree === null || diskTree.head === null || (local === null && diskTree.changed + diskTree.untracked > 0)
      ? null
      : {
          head: diskTree.head,
          marks: workingChanges(local),
          stamp: local?.status === "read" ? (local.stamp ?? `${diskTree.changed}:${diskTree.untracked}`) : "",
          branch: diskTree.branch,
          standing: treeStanding(diskTree) || null,
        },
  );
  // A held choice whose tree stops reading goes back to the default branch, as a deleted branch does, and says so once.
  $effect(() => {
    if (!wantsDisk || checkout === undefined || diskWhy === null || repoId === null) return;
    const id = repoId;
    const why = diskWhy;
    untrack(() => {
      delete chosen[id];
      push({ text: `The working tree can't be read any more: ${why}. Showing ${defaultBranch?.name ?? "the default branch"}.` });
    });
  });
  const selected = $derived(onDisk ? (diskBranch ?? defaultBranch) : (picked ?? defaultBranch));
  /**
   * The entity every tab reads at: the repository itself on its default
   * branch (or before the branches answer none), the branch on any other.
   */
  const at = $derived(repoId === null ? null : (selected && !selected.isDefault ? selected.entityId : null) ?? repoId);
  const branchName = $derived(selected?.name ?? null);
  /**
   * Whether `one` is Files on a repository no branch was picked on this session: it reads at the repository itself,
   * which is its default branch whatever the list or the mirror will name, so it starts its reads with the page's own
   * and names the branch when one of them answers. Waiting for the name put its tree, and the README read after it,
   * a round of reads behind the row and the branch list (the ux12 Windows pass, 2026-11-15: three rounds, 219 ms at
   * 4x and 391 at 6x on Björn's PC).
   */
  const filesAtOnce = (one: string) => one === "files" && picked === null;
  const changed = $derived(changedPaths(local));
  const sentence = $derived(checkoutSentence(checkout));
  /** The line under the title, for its tooltip where it is cut; the clone's sign beside it holds its path in its bubble. */
  const subTitle = $derived(repo === null ? "" : headLine(facts, repo.updated_at, now));
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
  /** The tab's own reads again (its refresh, a return to it): a number, so moving between tabs, which replaces the held tabs, asks nothing. */
  const buildsAsked = $derived(held.reads["builds"] ?? 0);
  $effect(() => {
    const id = repoId;
    if (!ready || id === null) return;
    void buildsAsked;
    if (buildsFor !== id) {
      buildsFor = id;
      buildsAsking = null;
      builds = null;
      buildsFailed = null;
    }
    untrack(() => readBuilds(id));
  });
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

  /** Your branches (`yoursOf`): the Builds tab's *Yours*, and what its count watches beside the default branch. */
  const buildsMine = $derived(
    yoursOf(
      (known ?? []).map((option) => option.name),
      job,
      checkout?.tree?.status === "read" ? checkout.tree.branch : null,
      defaultBranch?.name ?? null,
    ),
  );
  /** The pipelines whose newest run failed on the default branch or one of yours: the Builds tab's count, in err. */
  const buildsFailing = $derived(
    builds === null
      ? 0
      : builds.pipelines.filter((row) =>
          [defaultBranch?.name ?? null, ...buildsMine].some((name) => {
            const run = name === null ? null : latestOn(row, name);
            return run !== null && runTone(run) === "err";
          }),
        ).length,
  );

  /** A tab's count, drawn only above zero once its read answered. */
  /** The tabs that can carry a count: each keeps the count's room with none (the nothing-moves pass). */
  const COUNTED: ReadonlySet<RepoTabId> = new Set<RepoTabId>(["builds", "branches", "prs"]);
  function countOf(id: RepoTabId): { n: number; warn: boolean; err?: boolean } | null {
    if (id === "builds" && builds !== null && builds.pipelines.length > 0)
      return buildsFailing > 0 ? { n: buildsFailing, warn: false, err: true } : { n: builds.pipelines.length, warn: false };
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

  /** Bumped where Compare built ahead is shown at another pair than it was built for: it is built afresh ({@link compareAhead}). */
  let compareGen = $state(0);
  /** The pair Compare was built ahead at, until it is first shown. */
  let compareAhead: { base: string; head: string } | null = null;

  // An address on Compare that no press reached for (a launcher jump, a link set by a page): the pair it names is asked
  // before Compare is built, as a press on its link asks it (Fable's ruling, the compare-first-open pass, 2026-11-20). At
  // 6x the build took 100 ms and more before Compare asked for itself.
  $effect.pre(() => {
    const id = repoId;
    if (slot !== "compare" || id === null) return;
    const place = route;
    untrack(() => {
      const base = place.base ?? defaultBranch?.name ?? null;
      const head = place.branch ?? compareProposed;
      // Built ahead for another pair than this address names: built afresh for it, so it never shows the other pair's
      // diff dimmed while it reads its own, as a pick would.
      if (compareAhead !== null && (compareAhead.base !== base || compareAhead.head !== head)) compareGen += 1;
      compareAhead = null;
      if (base !== null && head !== null) askCompareEarly(ports.rangeFiles, id, base, head, false, ports.rangeNames);
    });
  });

  /**
   * **Compare built ahead of its press** (Fable's ruling, the compare-first-open pass, 2026-11-20). Its first open drew in
   * 206 and 317 ms at 4x and 6x full screen on Björn's PC, four tasks around reads that went out late, each laying the
   * page out again; built while the reader looks at the landing, the press is a return, one style pass and one layout, and
   * its first frame holds the whole page. Once the landing's tree has drawn (or a moment after the page opened on another
   * tab), the reader has been still a second and the window is idle, the Compare tab is mounted hidden at the bare
   * `compare` address, on the branch *New pull request* proposes. Only on a page shown, with more than one branch; without
   * a clone, once the early compare (asked here in the background lane if the landing has not) has answered, so the forge
   * is asked nothing new but the branch's standing, which goes in that lane too. Nothing is staged and nothing moves: the
   * tab is hidden until its press shows it whole.
   */
  $effect(() => {
    const id = repoId;
    const wait = ports.aheadWait ?? (/jsdom/i.test(navigator.userAgent) ? null : quietAndIdle);
    if (wait === null || !ready || !visible || id === null || known === null || oneBranch || slot === "compare" || held.places.compare !== undefined) return;
    if (checkout === undefined) return;
    const noClone = sentence !== null && sentence.path === null;
    const drew = treeDrew;
    let live = true;
    void Promise.race([drew.promise, new Promise((resolve) => setTimeout(resolve, COMPARE_EARLY_AFTER_MS))])
      .then(() => (live ? wait() : undefined))
      .then(() => {
        if (!live || !noClone) return;
        const [base, head] = untrack(() => [defaultBranch?.name ?? null, compareProposed]);
        if (base === null || head === null || base === head) return;
        askCompareEarly(untrack(() => ports.rangeFiles), id, base, head, true, untrack(() => ports.rangeNames));
        return earlyAsked(id, base, head)?.then(() => (live ? wait() : undefined));
      })
      .then(() => {
        if (!live) return;
        untrack(() => {
          const [base, head] = [defaultBranch?.name ?? null, compareProposed];
          if (base === null || head === null || base === head || tabs.repoId !== id) return;
          compareAhead = { base, head };
          tabs = holdSlot(tabs, [id, "compare"]);
        });
      });
    return () => {
      live = false;
    };
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
    const pulls = branchPulls;
    let live = true;
    const asked = ready ? asking.slice(0, ASKED) : [];
    const id = untrack(() => repoId);
    /** A branch's standing as a look of the last few minutes read it, else as the mirror holds it (`mirrorStanding`). */
    const keptOf = (branch: { name: string; id: string }): LookStanding | undefined => {
      const kept = standings.get(branch.id);
      if (kept !== undefined && Date.now() - kept.at < STANDING_KEPT_MS) return kept.standing;
      return Array.isArray(pulls) ? mirrorStanding(pulls, branch.name) : undefined;
    };
    /**
     * The look over the branches in order: the first that stands free wins, a branch that can't be read (`null`) says nothing
     * about what stands, and *proposed* stays; `undefined` while a branch it needs is not read yet.
     */
    const walk = (of: (i: number) => LookStanding | null | undefined): { propose: string | null; opened: BranchStanding["pr"] } | undefined => {
      let first: BranchStanding["pr"] = null;
      for (let i = 0; i < asked.length; i += 1) {
        const standing = of(i);
        if (standing === undefined) return undefined;
        if (standing === null) return { propose: null, opened: first };
        if (standing.pr === null && (choice !== null || standing.canOpenPr)) return { propose: asked[i]!.name, opened: null };
        first ??= standing.pr;
      }
      return { propose: null, opened: first };
    };
    const got: (LookStanding | null | undefined)[] = asked.map(keptOf);
    const found = (answer: { propose: string | null; opened: BranchStanding["pr"] }) => {
      toPropose = answer.propose;
      openedFrom = answer.opened;
      seeking = false;
    };
    // Every branch it needs is kept: drawn now, nothing held and nothing asked.
    const answered = asked.length > 0 ? walk((i) => got[i]) : undefined;
    if (answered !== undefined) {
      untrack(() => found(answered));
      return;
    }
    // Before the branches are read the page knows no branch to start on: *main* drew for half a second, then went.
    untrack(() => (seeking = asked.length > 0 || (ready && choice === null && branches === null)));
    // The mirror's answer comes in a frame or two, and reads no forge: the look waits for it rather than read views.
    if (pulls === null && asked.length > 0) return;
    void (async () => {
      if (asked.length > 0 && id !== null && !searched.has(id)) {
        searched.add(id);
        if (untrack(() => slot) === "files" && !treeDrew.done) await Promise.race([treeDrew.promise, new Promise((resolve) => setTimeout(resolve, FIRST_SEARCH_AFTER_MS))]);
        if (!live) {
          searched.delete(id);
          return;
        }
      }
      // In order, a batch at a time: the first branch in order that stands free wins, as when they were read one by one.
      for (let at = 0; at < asked.length; at += ASKED_AT_ONCE) {
        const batch = asked.slice(at, at + ASKED_AT_ONCE);
        const read = await Promise.all(
          batch.map(({ id: branch }, i) =>
            got[at + i] !== undefined
              ? Promise.resolve(got[at + i]!)
              : (looking.get(branch) ??
                (() => {
                  const reading = ports.entityView(branch, null).then(
                    (view) => {
                      const standing = branchStanding(view);
                      standings.set(branch, { at: Date.now(), standing });
                      return standing;
                    },
                    () => null,
                  );
                  looking.set(branch, reading);
                  void reading.finally(() => looking.delete(branch));
                  return reading;
                })()),
          ),
        );
        if (!live) return;
        read.forEach((standing, i) => (got[at + i] = standing));
        const answer = walk((i) => (i < at + batch.length ? got[i] : undefined));
        if (answer !== undefined) return found(answer);
      }
      found(walk((i) => got[i]) ?? { propose: null, opened: null });
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
    // The working tree is no branch of the list: it is held by its name, `~working-tree`, with no entity.
    const name = (event.currentTarget as HTMLSelectElement).value;
    const option = known?.find((candidate) => candidate.name === name);
    chosen[repoId] = { name, entityId: option?.entityId ?? null };
  }

  /** A repository holding one branch has nothing to propose from. */
  const oneBranch = $derived(known !== null && known.length === 1);

  // ---- A compare asked early (the forge-waits pass, 2026-11-13, `compare-early.ts`) ----

  /**
   * *New pull request*'s compare, asked once the landing has drawn (or a moment after the page opened on another tab) and
   * the page is idle, where the repository has no clone: Gitea's compare took 0.65 to 2.7 s, which a clone answers in a
   * tenth of that. Once a session; Compare takes the answer. Asked ahead (`askCompareEarly`'s `ahead`): the backend sends
   * it behind the landing's own reads of the forge, which it went out beside before (the landing-rounds pass,
   * 2026-11-16).
   */
  const COMPARE_EARLY_AFTER_MS = 1500;
  $effect(() => {
    const id = repoId;
    const head = seeking ? null : starting;
    const base = defaultBranch?.name ?? null;
    const noClone = sentence !== null && sentence.path === null;
    if (!ready || !visible || id === null || head === null || base === null || head === base || !noClone || oneBranch || tab === "compare") return;
    let live = true;
    const drew = treeDrew;
    void Promise.race([drew.promise, new Promise((resolve) => setTimeout(resolve, COMPARE_EARLY_AFTER_MS))])
      .then(() => (live ? whenIdle() : undefined))
      .then(() => {
        if (live) askCompareEarly(untrack(() => ports.rangeFiles), id, base, head, true, untrack(() => ports.rangeNames));
      });
    return () => {
      live = false;
    };
  });

  /**
   * The pointer resting on a link to a compare (*New pull request*, the Compare tab, a branch's row) asks its list, so the
   * source's round trip runs while the hand is on its way to the press, as a file's in the tree does (`REST_MS`); the
   * press itself (a pointer's, or Enter on the link) asks it at once, where the pointer came quicker than its rest
   * (Fable's ruling, the ux18 pass, 2026-11-17): the forge's round starts at the press, not once Compare has built.
   */
  const COMPARE_REST_MS = 80;
  let comparing: ReturnType<typeof setTimeout> | null = null;
  /** The compare a link under `target` opens: its repository and two ends; `null` for any other link. */
  function compareAt(target: EventTarget | null): { id: string; from: string; to: string } | null {
    const id = repoId;
    const base = defaultBranch?.name ?? null;
    const link = target instanceof Element ? target.closest("a[href]") : null;
    if (link === null || id === null || base === null) return null;
    const href = link.getAttribute("href") ?? "";
    const stem = repoHash(id, "compare");
    if (href !== stem && !href.startsWith(`${stem}/`)) return null;
    let tail: string;
    try {
      tail = href === stem ? (compareProposed ?? "") : decodeURIComponent(href.slice(stem.length + 1));
    } catch {
      return null;
    }
    const cut = tail.indexOf("...");
    const [from, to] = cut < 0 ? [base, tail] : [tail.slice(0, cut), tail.slice(cut + 3)];
    return { id, from, to };
  }
  function restOnCompare(event: PointerEvent) {
    if (comparing !== null) clearTimeout(comparing);
    comparing = null;
    const at = compareAt(event.target);
    if (at === null) return;
    comparing = setTimeout(() => {
      comparing = null;
      askCompareEarly(ports.rangeFiles, at.id, at.from, at.to, false, ports.rangeNames);
    }, COMPARE_REST_MS);
  }
  function pressOnCompare(event: PointerEvent | KeyboardEvent) {
    if (event instanceof KeyboardEvent ? event.key !== "Enter" : event.button !== 0) return;
    const at = compareAt(event.target);
    if (at === null) return;
    if (comparing !== null) clearTimeout(comparing);
    comparing = null;
    askCompareEarly(ports.rangeFiles, at.id, at.from, at.to, false, ports.rangeNames);
  }
  /** Heard on the page, not on each link: the links are drawn by the header, the tabs and a tab's rows. */
  function watchCompareLinks(element: HTMLElement) {
    element.addEventListener("pointerover", restOnCompare, { passive: true });
    element.addEventListener("pointerdown", pressOnCompare, { passive: true });
    element.addEventListener("keydown", pressOnCompare, { passive: true });
    return () => {
      element.removeEventListener("pointerover", restOnCompare);
      element.removeEventListener("pointerdown", pressOnCompare);
      element.removeEventListener("keydown", pressOnCompare);
      if (comparing !== null) clearTimeout(comparing);
    };
  }

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
    // The observer's first call reads it after the page's layout and before its first paint: read here, it laid out the
    // half-built page inside the build of a repository's first open (7 ms of it at CPU 6x, the ux14 pass, 2026-11-16).
    if (typeof ResizeObserver === "undefined") return read();
    const watch = new ResizeObserver(read);
    watch.observe(header);
    watch.observe(acts);
    return () => watch.disconnect();
  }
</script>

<div class="tf-full" bind:this={page} {@attach watchCompareLinks}>
  <TfCrumbs parts={crumbs} {back} />

  {#if repoId === null || failed !== null}
    {#if failed !== null && !missing}
      <TfNothingHere message="This repository could not be read: {failed}" onretry={() => attempt++} />
    {:else}
      <TfNothingHere message="knobas does not hold this repository. It may not have synced yet." href="#/code" action="All repositories" />
    {/if}
  {:else}
    <header class="tf-fhead tf-repohead" class:tf-unread={!drawnHead.shown} {@attach watchStack}>
      <div>
        <h1 class="tf-mono">{repo?.title ?? ""}</h1>
        <div class="tf-sub">
          <span class="tf-reposub" title={subTitle || undefined}>{subTitle}</span>
          <TfCloneSign entityId={repoId} {checkout} source={sourceName(source)} />
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
            >{#if wantsDisk}<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.75 3.75h4.5l1.5 1.5h6.5v7h-12.5z" /></svg
              >{:else}<svg viewBox="0 0 16 16" aria-hidden="true"
              ><circle cx="4.5" cy="3.5" r="1.5" /><circle cx="4.5" cy="12.5" r="1.5" /><circle cx="11.5" cy="5" r="1.5" /><path
                d="M4.5 5v6M11.5 6.5c0 3-7 2-7 4.5"
              /></svg
            >{/if}<select
              aria-label="Branch"
              title={wantsDisk ? `Working tree${checkout?.path ? ` at ${checkout.path}` : ""}${diskTree?.branch ? `, on ${diskTree.branch}` : ""}` : (branchName ?? undefined)}
              value={wantsDisk ? WORKING_TREE : (branchName ?? "")}
              onchange={pick}
              disabled={(known === null || known.length === 0) && !wantsDisk}
            >
              <!-- The working tree first, under a rule, where the repository is cloned here (2026-11-21): its words never
                   change, so no re-read resizes the box; a checkout that can't be read keeps it, disabled, saying why. -->
              {#if checkout?.path != null || wantsDisk}
                <option value={WORKING_TREE} disabled={!wantsDisk && diskWhy !== null} title={diskWhy ?? undefined}
                  >{wantsDisk || diskWhy === null ? "Working tree" : `Working tree: ${diskWhy}`}</option
                ><hr />
              {/if}
              <!-- Before the list is read, the shown name is its one option, so the box is as wide as that name from its first draw. -->
              {#if known === null && branchName && !wantsDisk}<option value={branchName}>{branchName}</option>{/if}
              {#each known ?? [] as option (option.name)}
                <!-- A branch knobas is making has nothing to read at until a read lists it: offered, but not to read at yet. -->
                <option value={option.name} disabled={option.made !== undefined} title={option.made !== undefined ? `Being made on ${sourceName(source)}: it can be read once it is there` : undefined}
                  >{option.name}{option.made !== undefined ? ` (${option.made})` : ""}</option
                >
              {/each}
            </select><svg class="tf-caret" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
              ><path d="M4 6l4 4 4-4" /></svg
            ></label
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
        {#if detail !== null}<TfDetailRefresh {detail} {refresh} {reread} page name={shortName} />{/if}
      </div>
    </header>
    {#if detail !== null}<TfDetailBanners {detail} {refresh} {reread} />{/if}
    <!-- A tab that can carry a count keeps a two-figure count's room with none, so one a later read or a sync brings widens
         nothing and pushes no tab (the nothing-moves pass). -->
    <nav class="tf-tabs tf-countroom" class:tf-unread={!drawnHead.shown} aria-label="Repository">
      {#each REPO_TABS as entry, n (entry.id)}
        {@const count = countOf(entry.id)}
        {#if n > 0 && REPO_TABS[n - 1]!.group !== entry.group}<span class="tf-tabgrp" aria-hidden="true"></span>{/if}
        <!-- On the working tree, Compare opens with it as *From* (ADR-0049's page), or where the reader left it on it. -->
        <a href={entry.id === "compare" && onDisk && !isWorkingTree(placeOf("compare").branch) ? compareHash(repoId, WORKING_TREE) : tabHref(held, repoId, entry.id)} aria-current={tabKnown && tab === entry.id ? "page" : undefined}
          >{entry.label}{#if count !== null}{" "}<span class="tf-count" class:tf-warn={count.warn} class:tf-err={count.err === true} title={entry.id === "builds" && count.err === true ? `${count.n} failing on ${defaultBranch?.name ?? "the default branch"} or your branch` : undefined}>{count.n}</span
            >{:else if COUNTED.has(entry.id)}{" "}<span class="tf-count tf-hidden" aria-hidden="true"></span>{/if}</a
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
        <!-- Another repository mounts its tabs afresh (#2248): nothing the reader did on one carries into the next. -->
        {#key repoId}
        {#each drawn as one (one)}
          {@const refreshed = held.reads[one] ?? 0}
          {#if one === "local"}
            <TfRepoTab shown={one === slot}>
              <TfRepoLocal {repoId} title={repo?.title ?? ""} {checkout} readDiff={ports.entityCheckoutDiff} {setPath} fileBytes={ports.fileBytes ?? null} piped={ports.piped ?? null} />
            </TfRepoTab>
          {:else if one === "files" && wantsDisk && disk === null && (checkout === undefined || onDisk)}
            <!-- The working tree waits for the checkout and git's list, so its tree draws once (2026-11-21). -->
            <TfRepoTab shown={one === slot}><div class="tf-files"></div></TfRepoTab>
          {:else if at !== null && (branchName !== null || filesAtOnce(one))}
            <!-- The rest wait for the branch every tab reads at: the mirror's default, or the list's (#2243). Files
                 does not on a repository no branch was picked on, where it reads at the repository (`filesAtOnce`). -->
            <TfRepoTab shown={one === slot}>
              {#if one === "files"}
                <TfRepoFiles
                  {ports}
                  {repoId}
                  {at}
                  branch={branchName}
                  path={placeOf(one).path}
                  line={placeOf(one).line}
                  place={placeOf(one).place ?? null}
                  folderRender={placeOf(one).render ?? null}
                  rendered={placeOf(one).rendered ?? null}
                  {changed}
                  editing={placeOf(one).tab === "edit"}
                  {job}
                  checkoutBranch={checkout?.tree?.status === "read" ? checkout.tree.branch : null}
                  {checkout}
                  checkoutRead={checkoutAnswered}
                  title={repo?.title ?? ""}
                  {source}
                  {refreshed}
                  refusal={commitRefused}
                  treeSections={detail?.tree ?? null}
                  ondrew={() => treeDrew.tell()}
                  {disk}
                />
              {:else if one === "commit" && placeOf(one).commit !== null}
                <TfRepoCommit {ports} {repoId} commitId={placeOf(one).commit ?? ""} {now} {refreshed} onNewBranch={(from) => (newBranch = { from })} />
              {:else if one === "branches"}
                <TfRepoBranches {ports} {repoId} {source} {refreshed} {builds} />
              {:else if one === "compare" && known !== null && isWorkingTree(placeOf(one).branch)}
                <!-- *From* the working tree (ux26): its own page, so no branch read starts for a name that is no branch. -->
                <TfRepoCompareWorking
                  {ports}
                  {repoId}
                  branches={known}
                  base={placeOf(one).base}
                  defaultBranch={defaultBranch?.name ?? null}
                  {checkout}
                  {refreshed}
                />
              {:else if one === "compare" && known !== null}
                {#key compareGen}
                <TfRepoCompare
                  {checkout}
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
                  pulls={branchPulls}
                  onNewBranch={() => (newBranch = { from: null })}
                />
                {/key}
              {:else if one === "commits"}
                <TfRepoCommits {ports} {repoId} {at} {source} branch={branchName} defaultBranch={defaultBranch?.name ?? null} {refreshed} />
              {:else if one === "builds"}
                <TfRepoBuilds
                  {repoId}
                  branches={known}
                  defaultBranch={defaultBranch?.name ?? null}
                  mine={buildsMine}
                  {job}
                  branch={placeOf(one).branch}
                  pipeline={placeOf(one).pipeline}
                  {builds}
                  failed={buildsFailed}
                  runs={ports.pipelineRuns}
                  {refreshed}
                />
              {:else if one === "prs"}
                <TfRepoPrs {ports} {repoId} rows={prs} {job} {now} newPr={proposed === null || oneBranch ? null : repoHash(repoId, "compare", encodeURIComponent(proposed))} reread={rereadPrs} />
              {/if}
            </TfRepoTab>
          {/if}
        {/each}
        {/key}
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
