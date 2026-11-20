<!--
  Compare with *From* the working tree (ux26, ADR-0048,
  `docs/decisions/2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-the-working-tree.md` ruling 5):
  `#/repo/<id>/compare/<into>...~working-tree`, the checkout's files as they stand on disk against *Into*, untracked
  files git does not ignore included.

  - **The form's short shape**, the one Compare has when a pull request is open, known from the address so it draws at
    its final height in the first frame: *Into* and *From* (*Working tree* first), and one line where the pull
    request's would stand, *Your checkout at ⟨path⟩, on ⟨branch⟩ at ⟨sha⟩. Commit and push it to propose a pull
    request.* No title, description or *Create pull request*.
  - **Commits**: *Uncommitted changes* first, then the commits `HEAD` has that *Into* lacks (`range.commits`).
  - **What it changes**: the names first (`entity_range_names`), the counts behind (`entity_range_files`), each file's
    lines a page at a time between the merge base and the working tree; nothing read ahead (each batch is a start of
    git, and the tree may move). Read again when the page is shown, on the window's focus and on a refresh: the
    backend's stamp makes an untouched tree free, and a new stamp (`working.stamp`) is the view's epoch, so what changed
    is laid down in place.

  Its own component, not a mode of `TfRepoCompare`: none of a branch's reads (its standing, the proposal, the early
  read, the prediction and the check behind it) can start for a name that is no branch.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { sideWidth } from "../side-width.svelte";
  import { ipcErrorMessage } from "../../ipc";
  import type { CheckoutView, DiffFile, DiffRange, RangeFiles, WorkingAt } from "../../ipc/entity";
  import type { DiffFill, DrawnFile } from "../diff/diff";
  import { namesList, rangeFill } from "../diff/fill";
  import TfDiffView from "../diff/TfDiffView.svelte";
  import TfOpenObject from "../openin/TfOpenObject.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import TfPath from "../TfPath.svelte";
  import TfWaiting from "../TfWaiting.svelte";
  import { diffHash, WORKING_TREE } from "../diff/address";
  import { endWords } from "../diff/rendered";
  import { shortEnd } from "../diff/sha";
  import { rangeContext } from "../diff/context";
  import { pageVisible } from "../kept-pages.svelte";
  import { onFocusWhileShown } from "../while-shown.svelte";
  import { waitPhase } from "../waiting";
  import { thousands } from "../counted";
  import { Windowed } from "../../shell/windowed.svelte";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { compareHash, type BranchOption } from "./repo";

  let {
    ports,
    repoId,
    branches,
    base: named = null,
    defaultBranch,
    checkout,
    refreshed = 0,
  }: {
    ports: Pick<RepoPorts, "entityView" | "fileBytes" | "piped" | "rangeFiles" | "rangeFile" | "rangeFilePages" | "cancelRead" | "rangeNames">;
    repoId: string;
    branches: readonly BranchOption[];
    /** *Into*, where the address names it. */
    base?: string | null;
    defaultBranch: string | null;
    /** The repository's checkout; `undefined` while `entity_checkout` has not answered. */
    checkout: CheckoutView | undefined;
    /** The page's refreshes: each reads the working tree again. */
    refreshed?: number;
  } = $props();

  const into = $derived(named !== null && branches.some((option) => option.name === named) ? named : (defaultBranch ?? branches[0]?.name ?? ""));
  /** No checkout here: the option is drawn, disabled, with why. */
  const noCheckout = $derived(checkout !== undefined && checkout.path === null);

  let files = $state.raw<DrawnFile[] | null>(null);
  let counted = $state(false);
  let range = $state.raw<DiffRange | null>(null);
  let working = $state.raw<WorkingAt | null>(null);
  let failed = $state<string | null>(null);
  let reading = $state(false);
  /** The *Into* the drawn list was read against: another one dims it while its own is read. */
  let drawnFor = $state<string | null>(null);

  const uid = $props.id();
  let reads = 0;
  let gen = 0;
  /** The counted read still out, called off by its name when another starts or the page goes. */
  let out: string | null = null;
  const stopRead = (name: string) => void ports.cancelRead?.(name);

  /** Each untracked file says so beside its name, as a local change says *staged*. */
  function labelled(list: readonly DiffFile[], at: WorkingAt | null | undefined): DrawnFile[] {
    const untracked = new Set(at?.untracked ?? []);
    return untracked.size === 0 ? [...list] : list.map((file) => (untracked.has(file.path) ? { ...file, label: `${file.path} (untracked)` } : file));
  }

  function lay(list: RangeFiles, isCounted: boolean, at: WorkingAt | null | undefined, asked: string) {
    files = labelled(list.files, at);
    counted = isCounted;
    range = list.range;
    working = at ?? null;
    failed = null;
    reading = false;
    drawnFor = asked;
  }

  /** Reads the working tree against *Into*; `again`: behind what is drawn, which stays until a new state lands. */
  function read(again: boolean) {
    const listing = ports.rangeFiles;
    if (listing === undefined || into === "") return;
    const asked = into;
    const id = ++gen;
    if (out !== null) stopRead(out);
    const name = `cmp-working-${uid}-${++reads}`;
    out = name;
    if (!again) reading = true;
    let landed = false;
    void ports.rangeNames?.(repoId, asked, WORKING_TREE).then(
      (names) => {
        if (id !== gen || landed || names === null) return;
        // The same state drawn already: nothing is laid down again.
        if (drawnFor === asked && working !== null && names.working?.stamp === working.stamp) return;
        lay(namesList(names), false, names.working, asked);
      },
      () => {},
    );
    listing(repoId, asked, WORKING_TREE, false, name).then(
      (list) => {
        landed = true;
        if (out === name) out = null;
        if (id !== gen) return;
        if (drawnFor === asked && counted && working !== null && list.working?.stamp === working.stamp) return;
        lay(list, true, list.working, asked);
      },
      (cause) => {
        if (out === name) out = null;
        if (id !== gen) return;
        reading = false;
        // A re-read that fails keeps what is drawn.
        if (!again || files === null) failed = ipcErrorMessage(cause);
      },
    );
  }

  $effect(() => {
    void into;
    void refreshed;
    if (noCheckout) return;
    // A refresh reads behind what is drawn; another *Into* dims it while its own list is read.
    untrack(() => read(drawnFor === into));
  });
  $effect(() => () => {
    if (out !== null) stopRead(out);
  });
  // Read again on focus and when the page is shown again: a person comes back from their editor (ruling 6).
  onFocusWhileShown(pageVisible(), () => {
    if (!noCheckout && drawnFor !== null) read(true);
  });

  const fill = $derived.by((): DiffFill | null => {
    const page = ports.rangeFile;
    if (page === undefined || range === null) return null;
    // Nothing read ahead: each batch starts git, and the tree may move under it.
    return rangeFill(page, repoId, { base: range.base, head: WORKING_TREE }, false, stopRead, ports.rangeFilePages, false);
  });
  const context = $derived(range === null ? null : rangeContext(ports.entityView, range, ports.fileBytes, ports.piped));
  const sideNames = $derived(range === null ? null : () => ({ old: `${drawnFor ?? into} at ${endWords(range!.base)}`, new: "working tree" }));
  const commits = $derived(range?.commits ?? []);
  const dirty = $derived(files !== null && files.length > 0);
  const commitWin = new Windowed({ count: () => commits.length, key: (i) => i, estimate: 25, scrolls: true });

  const sha7 = (sha: string | null | undefined) => (sha ? sha.slice(0, 7) : null);
  const path = $derived(checkout?.path ?? null);
  const branch = $derived(working?.branch ?? (checkout?.tree?.status === "read" ? checkout.tree.branch : null) ?? null);
  const head = $derived(working?.head ?? (checkout?.tree?.status === "read" ? checkout.tree.head : null) ?? null);

  function pickInto(value: string) {
    location.hash = compareHash(repoId, WORKING_TREE, value === defaultBranch ? null : value);
  }
  function pickFrom(value: string) {
    if (value === WORKING_TREE) return;
    location.hash = compareHash(repoId, value, into === defaultBranch ? null : into);
  }
</script>

<!-- Compare's own width for *Commits*, as the person dragged it there: its edge is Compare's (one page draws a sidebar's edge). -->
<div class="tf-compare tf-cworking" {@attach sideWidth("compare")}>
  <div>
    <h2 class="tf-cgroup-h">Compare</h2>
    <form class="tf-panel tf-cform" onsubmit={(event) => event.preventDefault()}>
      <div class="tf-formgrid">
        <label for="tf-cmp-into">Into</label>
        <select id="tf-cmp-into" class="tf-cbranch" value={into} onchange={(event) => pickInto((event.currentTarget as HTMLSelectElement).value)}>
          {#each branches as option (option.name)}<option value={option.name}>{option.name}</option>{/each}
        </select>
        <label for="tf-cmp-from">From</label>
        <select id="tf-cmp-from" class="tf-cbranch" value={WORKING_TREE} onchange={(event) => pickFrom((event.currentTarget as HTMLSelectElement).value)}>
          <option value={WORKING_TREE} disabled={noCheckout}>{noCheckout ? "Working tree: not cloned on this computer" : "Working tree"}</option>
          {#each branches as option (option.name)}<option value={option.name}>{option.name}</option>{/each}
        </select>
      </div>
      <div class="tf-cgo">
        {#if noCheckout}
          <span class="tf-cexists">This repository is not cloned on this computer, so it has no working tree here.</span>
        {:else}
          <span class="tf-cexists tf-cworkline"
            >Your checkout at {#if path !== null}<TfPath {path} />{:else}…{/if}, {#if branch !== null}on <span class="tf-mono">{branch}</span> at <span
                class="tf-mono">{sha7(head) ?? "no commit yet"}</span
              >{:else}at a detached head, <span class="tf-mono">{sha7(head) ?? "no commit yet"}</span>{/if}. Commit and push it to propose a pull request.</span
          >
        {/if}
        {#if files !== null && counted}
          <span class="tf-dim tf-small tf-cfiles">{thousands(files.length)} {files.length === 1 ? "file" : "files"} changed against {drawnFor ?? into}</span>
        {/if}
      </div>
    </form>
  </div>
  <aside class="tf-side">
    <div class="tf-panel tf-headed tf-ccommits">
      <header>
        <h3>Commits</h3>
        {#if commits.length > 0 || dirty}<span class="tf-ccount">{commits.length > 0 ? `${thousands(commits.length)}${dirty ? ", and what is not committed" : ""}` : "what is not committed"}</span>{/if}
      </header>
      {#if dirty}
        <div class="tf-rows tf-crows tf-cuncommitted"><div class="tf-r"><span class="tf-mono tf-dim"></span><span class="tf-t">Uncommitted changes</span></div></div>
      {/if}
      {#if commits.length > 0}
        <div class="tf-rows tf-crows" aria-label="Commits" {@attach commitWin.list}>
          {#if commitWin.before > 0}<div class="tf-wgap" aria-hidden="true" {@attach commitWin.room("before")}></div>{/if}
          {#each commitWin.slice(commits) as commit, n (commitWin.start + n)}
            <div class="tf-r" title={commit.title} {@attach commitWin.row(commitWin.start + n)}><span class="tf-mono tf-dim">{shortEnd(commit.sha)}</span><span class="tf-t"><TfKeyed text={commit.title} /></span></div>
          {/each}
          {#if commitWin.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach commitWin.room("after")}></div>{/if}
        </div>
      {:else if files !== null && !dirty}
        <p class="tf-prnote tf-cnone">Nothing that {drawnFor ?? into} lacks.</p>
      {/if}
    </div>
  </aside>
  <section class="tf-cdiff">
    <h2 class="tf-cgroup-h">
      What it changes
      {#if range !== null}
        <span class="tf-trail"
          ><a class="tf-btn tf-sm tf-ghost" href={diffHash(repoId, range.base, WORKING_TREE)}>Open as diff</a><TfOpenObject
            entityId={repoId}
            object={{ kind: "repository" }}
          /></span
        >
      {/if}
    </h2>
    {#if noCheckout}
      <div class="tf-empty">This repository is not cloned on this computer, so it has no working tree here.</div>
    {:else}
      <p class="tf-cnote">Read from your working tree. It reads again when you come back to the window.</p>
      <TfWaiting phase={waitPhase({ drawn: files !== null, reading, failed: failed !== null && files === null })} error={failed} shape="lines">
        {#if files !== null && files.length === 0}
          <div class="tf-empty">Your working tree changes nothing against {drawnFor ?? into}.</div>
        {:else if files !== null}
          <TfDiffView
            {files}
            {context}
            {fill}
            {counted}
            calm={!reading}
            {sideNames}
            epoch={working?.stamp ?? null}
            folders={range === null ? null : { entityId: repoId, before: range.base, after: WORKING_TREE }}
            openAt={{ entityId: repoId, at: null }} />
        {/if}
      </TfWaiting>
    {/if}
  </section>
</div>
