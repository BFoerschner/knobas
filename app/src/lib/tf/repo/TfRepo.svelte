<!--
  The repository page (#838, R-8.3 to R-8.5, R-8.7, R-8.10), prototype-6's
  `repoHead` and `renderRepo`: `#/repo/<repo entity id>` and its tabs
  `#/repo/<id>/<tab>`, where `files/<path>` is Files with a file open and
  `commits/<commit entity id>` a commit's diff.

  - **The header**: the repository's `owner/name`, its declared teams and
    language (`code_facts`, #973), its last push and its
    checkout (`entity_checkout`: where it is cloned and how it stands, or *Not
    on this machine yet*); the *Branch* dropdown, which sets the branch every
    tab reads at and is held per repository for the session; *New branch…*,
    *New pull request* (the compare page, #840) and *Open in…* (#844), or
    *Clone* for a repository not on this machine (#846).
  - **The tabs**, `REPO_TABS` in `repo.ts`: Files (and its edit, #845),
    Commits, Branches (#839), Pull requests (and the compare page, #840) and
    Local changes (#847).
  - The checkout is read again when the window gets focus and when a clone
    finishes (ADR-0021).
  - **Writes** go through the write queue (`submit_write`): *New branch…*
    (`CreateBranch`), a branch's *Delete…* (`DeleteBranch`), *Create pull
    request* (`CreatePullRequest`) and an edit's commit (`CommitFile`).

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
    SourceView,
  } from "../../ipc/entity";
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import type { WriteSource } from "../../shell/queued-toast";
  import type { OpenJobPorts } from "../job/open-jobs";

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
  }

  /**
   * The branch each repository is read at, by its entity id, as prototype-6's
   * `S.repoBranch`: held for the session, never written anywhere.
   */
  const chosen: Record<string, string> = $state({});

  /** Forget every repository's branch: a new session, which is what each test is. */
  export function forgetBranches(): void {
    for (const key of Object.keys(chosen)) delete chosen[key];
  }
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import {
    codeFacts,
    entityCheckout,
    entityCheckoutDiff,
    entityView,
    getEntity,
    jobReach,
    listContexts,
    listEntities,
    setCheckoutOverride,
    submitWrite,
    type EntityRow,
  } from "../../ipc/entity";
  import { listSources } from "../../ipc/sources";
  import { jobHash } from "../route";
  import {
    branchOptions,
    changedPaths,
    checkoutSentence,
    headLine,
    litTab,
    readAllRows,
    readRepoRoute,
    repoHash,
    repoPrs,
    REPO_TABS,
    toolbarRefusal,
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

  let {
    ready,
    rest,
    job,
    jobAt = null,
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
    ports?: RepoPorts;
    /** Injectable clock, for the times drawn. */
    now?: Date;
  } = $props();

  const now = $derived(fixedNow ?? new Date());
  const route = $derived(readRepoRoute(rest));
  const repoId = $derived(route.repoId);

  /** How many pull requests the repository's read asks for: every one the mirror holds. */
  const LIMIT = 1000;

  let repo = $state<EntityRow | null>(null);
  /** The repository's source: where its writes go, named in their toasts. */
  let source = $state<WriteSource | null>(null);
  let failed = $state<string | null>(null);
  /** `null` while the *Branches* section has not answered. */
  let branches = $state<BranchOption[] | null>(null);
  /**
   * Why the repository's own toolbar draws *New branch…* disabled (a mirror,
   * a token that cannot write, #2030); `null` while it has not answered, and
   * where it offers the branch.
   */
  let branchRefused = $state<string | null>(null);
  let checkout = $state<CheckoutView | undefined>(undefined);
  let local = $state<DiffRead | null>(null);
  let prs = $state<EntityRow[] | null>(null);
  /** The repository's team and language (#973); `undefined` until `code_facts` answers. */
  let facts = $state<CodeFacts | undefined>(undefined);

  $effect(() => {
    if (!ready || repoId === null) return;
    const id = repoId;
    let live = true;
    repo = null;
    source = null;
    failed = null;
    branches = null;
    branchRefused = null;
    checkout = undefined;
    local = null;
    prs = null;
    facts = undefined;
    ports.codeFacts([id]).then(
      (answer) => {
        if (live) facts = answer.find((entry) => entry.entity_id === id);
      },
      () => {},
    );
    ports.getEntity(id).then(
      (detail) => {
        if (!live) return;
        repo = detail.row;
        source = detail.source;
        const filter: EntityFilter = {
          sources: [detail.row.source_id],
          kinds: ["pr"],
          updated_within_days: null,
          order: "updated_desc",
          context: null,
          project: null,
          include_deleted: false,
          reach: null,
        };
        readAllRows((offset) => ports.listEntities(filter, LIMIT, offset)).then(
          (rows) => {
            if (live) prs = repoPrs(rows, detail.row);
          },
          () => {
            if (live) prs = [];
          },
        );
      },
      (error) => {
        if (live) failed = ipcErrorMessage(error);
      },
    );
    // An empty repository has no branches; a source that cannot say reads as
    // one with only its default branch, which the repository's own view is.
    ports.entityView(id, "branches").then(
      (view) => {
        if (live) branches = branchOptions(view?.sections[0] ?? null);
      },
      () => {
        if (live) branches = [];
      },
    );
    // Whether the source offers a branch here is its toolbar's answer
    // (ADR-0020 rule 4); a view that does not answer leaves the button be.
    ports.entityView(id, null).then(
      (view) => {
        if (live) branchRefused = toolbarRefusal(view, "create_branch");
      },
      () => {},
    );
    readCheckout(id, () => live);
    const again = () => readCheckout(id, () => live);
    window.addEventListener("focus", again);
    return () => {
      live = false;
      window.removeEventListener("focus", again);
    };
  });

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

  const defaultBranch = $derived(branches?.find((option) => option.isDefault) ?? null);
  const selected = $derived(
    (repoId !== null && branches?.find((option) => option.name === chosen[repoId])) || defaultBranch,
  );
  /**
   * The entity every tab reads at: the repository itself on its default
   * branch (or before the branches answer none), the branch on any other.
   */
  const at = $derived(repoId === null ? null : (selected && !selected.isDefault ? selected.entityId : null) ?? repoId);
  const branchName = $derived(selected?.name ?? null);
  const changed = $derived(changedPaths(local));
  const sentence = $derived(checkoutSentence(checkout));
  const tab = $derived<RepoTabId>(litTab(route.tab));

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
    chosen[repoId] = (event.currentTarget as HTMLSelectElement).value;
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
      <div class="tf-acts">
        <label class="tf-fsel"
          >Branch<select aria-label="Branch" value={branchName ?? ""} onchange={pick} disabled={branches === null || branches.length === 0}>
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
        {#if proposed !== null}
          <a class="tf-btn" href={repoHash(repoId, "compare", encodeURIComponent(proposed))}>New pull request</a>
        {/if}
        <TfOpenIn entityId={repoId} title={repo?.title ?? ""} {checkout} size="md" />
      </div>
    </header>
    <nav class="tf-tabs" aria-label="Repository">
      {#each REPO_TABS as entry (entry.id)}
        {@const count = countOf(entry.id)}
        <a href={repoHash(repoId, entry.id)} aria-current={tab === entry.id ? "page" : undefined}
          >{entry.label}{#if count !== null}{" "}<span class="tf-count" class:tf-warn={count.warn}>{count.n}</span
            >{/if}</a
        >
      {/each}
    </nav>
    <div class="tf-cbody">
      {#if NOT_BUILT[route.tab] !== undefined}
        <div class="tf-empty"><p>{NOT_BUILT[route.tab]} is not built in the new shell yet.</p></div>
      {:else if route.tab === "local"}
        <TfRepoLocal {repoId} title={repo?.title ?? ""} {checkout} readDiff={ports.entityCheckoutDiff} {setPath} />
      {:else if branches === null || at === null}
        <!-- The branch every tab reads at is not known yet. -->
      {:else if route.tab === "files" || route.tab === "edit"}
        <TfRepoFiles
          {ports}
          {repoId}
          {at}
          branch={branchName}
          path={route.path}
          {changed}
          editing={route.tab === "edit"}
          {job}
          checkoutBranch={checkout?.tree?.status === "read" ? checkout.tree.branch : null}
          {checkout}
          title={repo?.title ?? ""}
          {source}
        />
      {:else if route.tab === "commits" && route.commit !== null}
        <TfRepoCommit {ports} {repoId} commitId={route.commit} {now} onNewBranch={(from) => (newBranch = { from })} />
      {:else if route.tab === "branches"}
        <TfRepoBranches {ports} {repoId} {source} />
      {:else if route.tab === "compare"}
        <TfRepoCompare
          {ports}
          {repoId}
          {source}
          branches={branches}
          head={route.branch ?? proposed}
          defaultBranch={defaultBranch?.name ?? null}
        />
      {:else if route.tab === "commits"}
        <TfRepoCommits {ports} {repoId} {at} branch={branchName} defaultBranch={defaultBranch?.name ?? null} />
      {:else if route.tab === "prs"}
        <TfRepoPrs {ports} {repoId} rows={prs} {job} />
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
