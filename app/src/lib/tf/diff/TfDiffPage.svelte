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
    entityRangeFiles,
    cancelRead,
    entityView,
    getEntity,
    type EntityDetail,
    type PipeRun,
    type PipeStep,
    type RangeDiff,
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
    cancelRead: (read) => cancelRead(read),
  };
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { caretOf, isSha, shortEnd } from "./sha";
  import { fillEnds, rangeFill, type RangeFileRead, type RangeFilesRead } from "./fill";
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
  import { compareSection } from "./address";
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
    if (!ready) return;
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

  onShownAgain(
    () => visible,
    () => {
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
    reading = true;
    const listing = ports.rangeFiles;
    const answer: Promise<RangeDiff> = listing !== undefined && ports.rangeFile !== undefined ? listing(id, from, to, ws, name) : ports.rangeDiff(id, from, to, ws);
    return answer.then(
      (diff) => {
        if (!alive()) return;
        reading = false;
        fill = listing !== undefined && ports.rangeFile !== undefined ? rangeFill(ports.rangeFile, id, fillEnds(diff, from, to), ws, stopRead) : null;
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
    <div class="tf-acts"><TfOpenObject entityId={repoId} object={{ kind: "range", base, head }} size="md" /></div>
  </header>
  <div class="tf-cbody">
  <!-- With no diff drawn there is no toolbar, and a tick that left nothing to draw must still come off. -->
  {#if readAt !== null && ignoreWhitespace && (files === null || files.length === 0)}
    <div class="tf-diffws">{@render whitespace()}</div>
  {/if}
  <TfWaiting phase={waitPhase({ drawn: files !== null, reading, failed: failed !== null })} error={failed} shape="lines">
    {#if files !== null && files.length === 0}
      <div class="tf-empty">Nothing changes between {shown(base)} and {shown(head)}.</div>
    {:else if files !== null}
      {#if range === null}
        <p class="tf-cnote" data-testid="no-range" title="As for a range from the repository's first commit, or a branch that merged its base back in.">Context does not expand here: the source did not settle which two commits this diff is between.</p>
      {/if}
      {#key `${section}\n${range?.base}\n${range?.head}\n${ignoreWhitespace}`}
        <TfDiffView {files} {cut} {context} {fill} openAt={{ entityId: repoId, at: head }} tools={readAt === null ? null : whitespace} />
      {/key}
    {/if}
  </TfWaiting>
  </div>
</div>
