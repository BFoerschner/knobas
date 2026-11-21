<!--
  The diff address (#2271, spec #2234 step 6b): `#/diff/<repo entity id>/<base>/<head>`,
  the diff between any two commits of a repository, as its own kept page
  (`page.ts`): linked, gone back to, and still drawn when shown again.

  - **The crumb** names the repository (its mirror row), which is what `Esc`
    goes to where nothing opened the page.
  - **The header**: `base … head` as written in the address, short shas
    shortened, the repository's *Commits* under it, and *Open in…* on the
    range (#2274); each file's header has its own, at its line.
  - **The diff**: `entity_range_diff` (#2272, ADR-0036): local read-only
    git when the repository's checkout holds both commits, else the
    repository's section `compare:<base>...<head>` (`address.ts`), drawn by
    the one diff component.
  - **Ignore whitespace**: in the diff's toolbar, offered when the diff was read
    at the checkout; disabled, with why as its title, when the source served it
    (no Gitea or Forgejo endpoint returns a diff with whitespace ignored, #2253).
    Another address keeps
    the last diff drawn, dimmed, while it is read (#2246). Where the answer
    names its range (#2310), context expands and an image diff renders, each
    side read whole at its sha (`context.ts` `rangeContext`). Where it names
    none, a line above the diff says context does not expand, and why.
  - **Shown again** after keep-alive held it hidden, it reads once more over
    what it drew: between two whole shas the read cache answers that, and a
    branch's end may have moved.

  Reads go through `entity_range_diff`, `entity_view` (context) and
  `get_entity`, injectable as `ports`.
-->
<script module lang="ts">
  import {
    entityFileBytes,
    entityFilePiped,
    entityRangeDiff,
    entityRangeFile,
    entityRangeFilePages,
    entityRangeFiles,
    cancelRead,
    entityView,
    getEntity,
    type EntityDetail,
    type PipeRun,
    type PipeStep,
    type RangeDiff,
    type RangeFiles,
    type SourceView,
  } from "../../ipc/entity";

  /** The page's two reads. */
  export interface DiffPagePorts {
    /** The diff itself, local or the source's (#2272). */
    rangeDiff: (entityId: string, base: string, head: string, ignoreWhitespace: boolean) => Promise<RangeDiff>;
    /** A section, for context read whole at a sha. */
    entityView: (entityId: string, section: string | null) => Promise<SourceView | null>;
    getEntity: (entityId: string) => Promise<EntityDetail>;
    /** A file's bytes at a commit, for a rendered image diff (#2279). */
    fileBytes?: (entityId: string, path: string, commit: string | null) => Promise<Uint8Array>;
    /** A file pipeline over a file at a commit, for pipelines on diffs (#2270). */
    piped?: (entityId: string, path: string, commit: string | null, steps: PipeStep[]) => Promise<PipeRun>;
    /**
     * The range's files without their lines, then each file's a page at a time as it nears the view (the compare pass,
     * 2026-11-13); the whole diff with `rangeDiff` when left out.
     */
    rangeFiles?: RangeFilesRead;
    rangeFile?: RangeFileRead;
    /** The first page of many files in one read (the big-compare pass, 2026-11-16). */
    rangeFilePages?: RangeFilePagesRead;
    /** Calls a named read off at the backend. */
    cancelRead?: (read: string) => Promise<boolean>;
  }

  /** Called through, not bound at load: a test's mock of the bridge may not carry every command. */
  const BRIDGE: DiffPagePorts = {
    rangeDiff: (entityId, base, head, ignoreWhitespace) => entityRangeDiff(entityId, base, head, ignoreWhitespace),
    entityView: (entityId, section) => entityView(entityId, section),
    getEntity: (entityId) => getEntity(entityId),
    fileBytes: (entityId, path, commit) => entityFileBytes(entityId, path, commit),
    piped: (entityId, path, commit, steps) => entityFilePiped(entityId, path, commit, steps),
    rangeFiles: (entityId, base, head, ignoreWhitespace, read) => entityRangeFiles(entityId, base, head, ignoreWhitespace, read),
    rangeFile: (entityId, base, head, path, oldPath, ignoreWhitespace, from, read) => entityRangeFile(entityId, base, head, path, oldPath, ignoreWhitespace, from, read),
    rangeFilePages: (entityId, base, head, files, ignoreWhitespace, read) => entityRangeFilePages(entityId, base, head, files, ignoreWhitespace, read),
    cancelRead: (read) => cancelRead(read),
  };
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { caretOf, isSha, shortEnd } from "./sha";
  import { endWords } from "./rendered";
  import { fillEnds, keptList, rangeFill, seedPages, type RangeFilePagesRead, type RangeFileRead, type RangeFilesRead } from "./fill";
  import type { DiffFile, DiffRange, DiffReadAt } from "../../ipc/entity";
  import type { DiffFill } from "./diff";
  import type { EscTarget } from "../escape";
  import { namePage, parentPage } from "../page-name.svelte";
  import TfCrumbs, { type Crumb } from "../TfCrumbs.svelte";
  import TfWaiting from "../TfWaiting.svelte";
  import { repoHash } from "../repo/repo";
  import { waitPhase } from "../waiting";
  import { onShownAgain } from "../while-shown.svelte";
  import TfDiffView from "./TfDiffView.svelte";
  import TfWhitespaceBox from "./TfWhitespaceBox.svelte";
  import TfOpenObject from "../openin/TfOpenObject.svelte";
  import { compareSection, diffHash, isWorkingTree, WORKING_TREE } from "./address";
  import { onFocusWhileShown } from "../while-shown.svelte";
  import type { WorkingAt } from "../../ipc/entity";
  import { rangeContext } from "./context";

  let {
    repoId,
    base,
    head,
    ready = true,
    visible = true,
    back = null,
    ports = BRIDGE,
  }: {
    repoId: string;
    base: string;
    head: string;
    ready?: boolean;
    visible?: boolean;
    /** Where `Esc` goes, which the breadcrumb's hint names. */
    back?: EscTarget | null;
    ports?: DiffPagePorts;
  } = $props();

  let title = $state<string | null>(null);
  // Raw: thousands of files, replaced whole on each read and never changed in place (the compare pass, 2026-11-13).
  let files = $state.raw<DiffFile[] | null>(null);
  /** The diff hit the parser's ceiling (#2269). */
  let cut = $state(false);
  let reading = $state(false);
  let failed = $state<string | null>(null);
  /** What the drawn diff is between, where the source named it. */
  let range = $state.raw<DiffRange | null>(null);
  /** Where the drawn diff was read; whitespace is ignored only at the checkout. */
  let readAt = $state<DiffReadAt | null>(null);
  let ignoreWhitespace = $state(false);
  /** The box's press redraws the diff around it, so focus is put back. */
  let boxPressed = $state(false);
  const context = $derived(range === null ? null : rangeContext(ports.entityView, range, ports.fileBytes, ports.piped));

  const section = $derived(compareSection(base, head));
  /** The head is the working tree (ux26): read at the checkout, again on focus, never kept between visits. */
  const working = $derived(isWorkingTree(head));
  /** The working tree asked as the base: a diff can end there, not start from it. */
  const backwards = $derived(isWorkingTree(base));
  /** Where the working tree stood when the drawn list was read: the view's epoch. */
  let workingAt = $state.raw<WorkingAt | null>(null);
  const crumbs = $derived<Crumb[]>([
    { name: "Code", href: "#/code" },
    title === null ? { name: "Repository", href: repoHash(repoId) } : { name: title, href: repoHash(repoId), code: true },
    { name: "Diff" },
  ]);
  namePage(() => "Diff");
  // A diff sits under its repository, as its crumb has it.
  parentPage(() => (title === null ? null : { hash: repoHash(repoId), name: title }));

  $effect(() => {
    if (!ready) return;
    const id = repoId;
    let live = true;
    title = null;
    ports.getEntity(id).then(
      (detail) => {
        if (live) title = detail.row.title;
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });

  $effect(() => {
    if (!ready || backwards) return;
    const id = repoId;
    const [from, to] = [base, head];
    const ws = ignoreWhitespace;
    let live = true;
    failed = null;
    const name = newRead();
    read(id, from, to, ws, () => live, false, name);
    return () => {
      live = false;
      stopRead(name);
    };
  });

  // The working tree read again on the window's focus too: a person comes back from their editor (ux26).
  onFocusWhileShown(
    () => visible,
    () => {
      if (!working || files === null) return;
      const id = repoId;
      const [from, to] = [base, head];
      const ws = ignoreWhitespace;
      read(id, from, to, ws, () => repoId === id && base === from && head === to && ignoreWhitespace === ws, true);
    },
  );
  onShownAgain(
    () => visible,
    () => {
      if (backwards) return;
      const id = repoId;
      const [from, to] = [base, head];
      const ws = ignoreWhitespace;
      read(id, from, to, ws, () => repoId === id && base === from && head === to && ignoreWhitespace === ws, true);
    },
  );

  /** Each read named, so another range or a page left calls it off at the backend (the compare pass). */
  const uid = $props.id();
  let reads = 0;
  const newRead = () => `diffpage-${uid}-${++reads}`;
  const stopRead = (name: string) => void ports.cancelRead?.(name).catch(() => {});
  /** The fill the drawn list's files are read by, a page of lines at a time; `null` while the diff drawn carries its lines. */
  let fill = $state.raw<DiffFill | null>(null);

  /**
   * The diff between `from` and `to`; `again` keeps what the page drew when the read fails. Where the ports read a range
   * a file at a time, its list, each file's lines read as it nears the view.
   */
  function read(id: string, from: string, to: string, ws: boolean, alive: () => boolean, again = false, name = newRead()): Promise<unknown> {
    const listing = ports.rangeFiles;
    // *Open as diff* of a compare drawn this session (the big-compare pass, 2026-11-16): the list it drew between these two
    // commits, and its files' lines kept, drawn in this page's first frame, read no second time. Two whole shas name a
    // diff that never changes, so there is nothing to read again behind it, on a return either.
    const drew = listing !== undefined && ports.rangeFile !== undefined && !ws && !isWorkingTree(to) ? keptList(id, from, to) : undefined;
    if (drew !== undefined) {
      reading = false;
      fill = rangeFill(ports.rangeFile!, id, fillEnds(drew, from, to), ws, stopRead, ports.rangeFilePages, drew.read_at === "checkout");
      files = drew.files;
      cut = drew.truncated;
      range = drew.range;
      readAt = drew.read_at;
      return Promise.resolve();
    }
    // Read again behind what is drawn (shown again, the window's focus), the diff stays live and undimmed.
    if (!again) reading = true;
    const answer: Promise<RangeDiff> = listing !== undefined && ports.rangeFile !== undefined ? listing(id, from, to, ws, name) : ports.rangeDiff(id, from, to, ws);
    return answer.then(
      (answered) => {
        if (!alive()) return;
        // The working tree as it stood: nothing is laid down again.
        if (again && answered.working !== undefined && workingAt !== null && answered.working.stamp === workingAt.stamp) return;
        workingAt = answered.working ?? null;
        // The first files' lines the list came with, kept for the fill's first asks (Fable's ruling, the ux18 pass).
        const diff = listing !== undefined && ports.rangeFile !== undefined ? seedPages(id, answered as RangeFiles) : answered;
        reading = false;
        // Nothing read ahead in a working tree: each batch is a start of git, and the tree may move under it.
        const ahead = diff.read_at === "checkout" && !isWorkingTree(to);
        const sameFill = again && fill !== null && isWorkingTree(to);
        fill =
          listing !== undefined && ports.rangeFile !== undefined
            ? sameFill
              ? fill
              : rangeFill(ports.rangeFile, id, fillEnds(diff, from, to), ws, stopRead, ports.rangeFilePages, ahead)
            : null;
        files = diff.files;
        cut = diff.truncated;
        range = diff.range;
        readAt = diff.read_at;
      },
      (error) => {
        if (!alive()) return;
        reading = false;
        if (again) return;
        failed = ipcErrorMessage(error);
        files = null;
      },
    );
  }

  /** A whole sha shortened as the source shortens it; anything else as written. */
  const shown = shortEnd;
  /** Whether an end is a branch name (Spec 5): such a diff is never read at a checkout, whatever it holds. */
  const byName = $derived(!isSha(caretOf(base)[0]) || !isSha(head));
  /** Why whitespace can't be left out here, where the source served the diff. */
  const noWhitespace = $derived(
    readAt === "checkout"
      ? null
      : byName
        ? "A branch name is read at the source, which can't leave whitespace out. Open this diff from its commits to read it at a checkout."
        : "Read at the source, which can't leave whitespace out. A checkout holding both commits can.",
  );
</script>

{#snippet whitespace()}
  <TfWhitespaceBox
    checked={readAt === "checkout" && ignoreWhitespace}
    why={noWhitespace}
    focus={boxPressed}
    onleave={() => (boxPressed = false)}
    onchange={(on) => {
      boxPressed = true;
      ignoreWhitespace = on;
    }}
  />
{/snippet}

<div class="tf-full tf-diffpage">
  <TfCrumbs parts={crumbs} {back} />
  <header class="tf-fhead">
    <div>
      <h1 class="tf-mono" data-testid="diff-range">{shown(base)} … {shown(head)}</h1>
      <div class="tf-sub">In <a class="tf-mono" href={repoHash(repoId, "commits")}>{title ?? repoId}</a></div>
    </div>
    <div class="tf-acts"><TfOpenObject entityId={repoId} object={working || backwards ? { kind: "repository" } : { kind: "range", base, head }} size="md" /></div>
  </header>
  <div class="tf-cbody">
  <!-- With no diff drawn there is no toolbar, and a tick that left nothing to draw must still come off. -->
  {#if readAt !== null && ignoreWhitespace && (files === null || files.length === 0)}
    <div class="tf-diffws">{@render whitespace()}</div>
  {/if}
  {#if backwards}
    <div class="tf-empty">
      <p>A diff can end at the working tree, not start from it.</p>
      <a class="tf-btn tf-sm" href={diffHash(repoId, head, WORKING_TREE)}>Compare {shown(head)} with the working tree</a>
    </div>
  {:else}
  {#if working && failed !== null && /not cloned on this computer/.test(failed)}
    <p class="tf-cnote">This repository is not cloned on this computer, so it has no working tree here. <a href={repoHash(repoId, "local")}>Local changes</a></p>
  {/if}
  <TfWaiting phase={waitPhase({ drawn: files !== null, reading, failed: failed !== null })} error={failed} shape="lines">
    {#if files !== null && files.length === 0}
      <div class="tf-empty">Nothing changes between {shown(base)} and {shown(head)}.</div>
    {:else if files !== null}
      {#if range === null}
        <p class="tf-cnote" data-testid="no-range" title="As for a range from the repository's first commit, or a branch that merged its base back in.">Context does not expand here: the source did not settle which two commits this diff is between.</p>
      {/if}
      {#key `${section}\n${range?.base}\n${range?.head}\n${ignoreWhitespace}`}
        <TfDiffView
          {files}
          {cut}
          {context}
          {fill}
          epoch={workingAt?.stamp ?? null}
          sideNames={range === null ? null : () => ({ old: endWords(range!.base), new: working ? "working tree" : endWords(range!.head) })}
          folders={range !== null && readAt === "checkout" ? { entityId: range.repository, before: range.base, after: working ? WORKING_TREE : range.head } : null}
          openAt={{ entityId: repoId, at: working ? null : head }}
          tools={readAt === null ? null : whitespace}
        />
      {/key}
    {/if}
  </TfWaiting>
  {/if}
  </div>
</div>
