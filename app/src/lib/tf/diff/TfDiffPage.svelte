<!--
  The diff address (#2271, spec #2234 step 6b): `#/diff/<repo entity id>/<base>/<head>`,
  the diff between any two commits of a repository, as its own kept page
  (`page.ts`): linked, gone back to, and still drawn when shown again.

  - **The header**: the repository's name (its mirror row, linking its
    *Commits*), then `base … head` as written in the address, short shas
    shortened, and *Open in…* on the range (#2274); each file's header has
    its own, at its line.
  - **The diff**: `entity_range_diff` (#2272, ADR-0036): local read-only
    git when the repository's checkout holds both commits, else the
    repository's section `compare:<base>...<head>` (`address.ts`), drawn by
    the one diff component.
  - **Ignore whitespace**: offered when the diff was read at the checkout;
    shown disabled, saying why, when the source served it (no Gitea or
    Forgejo endpoint returns a diff with whitespace ignored, #2253). Another address keeps
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
  }

  /** Called through, not bound at load: a test's mock of the bridge may not carry every command. */
  const BRIDGE: DiffPagePorts = {
    rangeDiff: (entityId, base, head, ignoreWhitespace) => entityRangeDiff(entityId, base, head, ignoreWhitespace),
    entityView: (entityId, section) => entityView(entityId, section),
    getEntity: (entityId) => getEntity(entityId),
    fileBytes: (entityId, path, commit) => entityFileBytes(entityId, path, commit),
    piped: (entityId, path, commit, steps) => entityFilePiped(entityId, path, commit, steps),
  };
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { caretOf, isSha, shortEnd } from "./sha";
  import type { DiffFile, DiffRange, DiffReadAt } from "../../ipc/entity";
  import TfWaiting from "../TfWaiting.svelte";
  import { repoHash } from "../repo/repo";
  import { waitPhase } from "../waiting";
  import { onShownAgain } from "../while-shown.svelte";
  import TfDiffView from "./TfDiffView.svelte";
  import TfOpenObject from "../openin/TfOpenObject.svelte";
  import { compareSection } from "./address";
  import { rangeContext } from "./context";

  let {
    repoId,
    base,
    head,
    ready = true,
    visible = true,
    ports = BRIDGE,
  }: {
    repoId: string;
    base: string;
    head: string;
    ready?: boolean;
    visible?: boolean;
    ports?: DiffPagePorts;
  } = $props();

  let title = $state<string | null>(null);
  let files = $state<DiffFile[] | null>(null);
  /** The diff hit the parser's ceiling (#2269). */
  let cut = $state(false);
  let reading = $state(false);
  let failed = $state<string | null>(null);
  /** What the drawn diff is between, where the source named it. */
  let range = $state<DiffRange | null>(null);
  /** Where the drawn diff was read; whitespace is ignored only at the checkout. */
  let readAt = $state<DiffReadAt | null>(null);
  let ignoreWhitespace = $state(false);
  const uid = $props.id();
  const why = `${uid}-ws-why`;
  const context = $derived(range === null ? null : rangeContext(ports.entityView, range, ports.fileBytes, ports.piped));

  const section = $derived(compareSection(base, head));

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
    read(id, from, to, ws, () => live);
    return () => {
      live = false;
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

  /** The diff between `from` and `to`; `again` keeps what the page drew when the read fails. */
  function read(id: string, from: string, to: string, ws: boolean, alive: () => boolean, again = false): Promise<unknown> {
    reading = true;
    return ports.rangeDiff(id, from, to, ws).then(
      (diff) => {
        if (!alive()) return;
        reading = false;
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
</script>

<div class="tf-diffpage">
  <h2 class="tf-cgroup-h">
    <a href={repoHash(repoId, "commits")}>{title ?? repoId}</a>
    <span class="tf-mono" data-testid="diff-range">{shown(base)} … {shown(head)}</span>
    <span class="tf-acts"><TfOpenObject entityId={repoId} object={{ kind: "range", base, head }} /></span>
  </h2>
  {#if readAt !== null}
    <div class="tf-diffws">
      <label class="tf-ck">
        <input
          type="checkbox"
          checked={readAt === "checkout" && ignoreWhitespace}
          onchange={(event) => (ignoreWhitespace = event.currentTarget.checked)}
          disabled={readAt !== "checkout"}
          aria-describedby={readAt === "checkout" ? undefined : why} />
        Ignore whitespace
      </label>
      {#if readAt !== "checkout"}
        <span class="tf-cnote" id={why} data-testid="ws-why"
          >{byName
            ? "Read from the source, whose API cannot leave whitespace out. A branch name is read where it lives, never at a checkout; open the diff from its commits to read it locally."
            : "Read from the source, whose API cannot leave whitespace out. A checkout holding both commits can."}</span
        >
      {/if}
    </div>
  {/if}
  <TfWaiting phase={waitPhase({ drawn: files !== null, reading, failed: failed !== null })} error={failed} shape="lines">
    {#if files !== null && files.length === 0}
      <div class="tf-empty">Nothing changes between {shown(base)} and {shown(head)}.</div>
    {:else if files !== null}
      {#if range === null}
        <p class="tf-cnote" data-testid="no-range">Context does not expand here: which two commits this diff is between is not settled, as for a range from the repository's first commit or a branch that merged its base back in.</p>
      {/if}
      {#key `${section}\n${range?.base}\n${range?.head}\n${ignoreWhitespace}`}
        <TfDiffView {files} {cut} {context} openAt={{ entityId: repoId, at: head }} />
      {/key}
    {/if}
  </TfWaiting>
</div>
