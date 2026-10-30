<!--
  The diff address (#2271, spec #2234 step 6b): `#/diff/<repo entity id>/<base>/<head>`,
  the diff between any two commits of a repository, as its own kept page
  (`page.ts`): linked, gone back to, and still drawn when shown again.

  - **The header**: the repository's name (its mirror row, linking its
    *Commits*), then `base … head` as written in the address, short shas
    shortened, and *Open in…* on the range (#2274); each file's header has
    its own, at its line.
  - **The diff**: the repository's section `compare:<base>...<head>`
    (`address.ts`), drawn by the one diff component. Another address keeps
    the last diff drawn, dimmed, while it is read (#2246). Where the answer
    names its range (#2310), context expands and an image diff renders, each
    side read whole at its sha (`context.ts` `rangeContext`). Where it names
    none, a line above the diff says context does not expand, and why.
  - **Shown again** after keep-alive held it hidden, it reads once more over
    what it drew: between two whole shas the read cache answers that, and a
    branch's end may have moved.

  Reads go through `entity_view` and `get_entity`, injectable as `ports`.
-->
<script module lang="ts">
  import { entityFileBytes, entityView, getEntity, type EntityDetail, type SourceView } from "../../ipc/entity";

  /** The page's two reads. */
  export interface DiffPagePorts {
    entityView: (entityId: string, section: string | null) => Promise<SourceView | null>;
    getEntity: (entityId: string) => Promise<EntityDetail>;
    /** A file's bytes at a commit, for a rendered image diff (#2279). */
    fileBytes?: (entityId: string, path: string, commit: string | null) => Promise<Uint8Array>;
  }

  /** Called through, not bound at load: a test's mock of the bridge may not carry every command. */
  const BRIDGE: DiffPagePorts = {
    entityView: (entityId, section) => entityView(entityId, section),
    getEntity: (entityId) => getEntity(entityId),
    fileBytes: (entityId, path, commit) => entityFileBytes(entityId, path, commit),
  };
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { DiffFile, DiffRange } from "../../ipc/entity";
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
  const context = $derived(range === null ? null : rangeContext(ports.entityView, range, ports.fileBytes));

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
    const asked = section;
    let live = true;
    failed = null;
    read(id, asked, () => live);
    return () => {
      live = false;
    };
  });

  onShownAgain(
    () => visible,
    () => {
      const id = repoId;
      const asked = section;
      read(id, asked, () => repoId === id && section === asked, true);
    },
  );

  /** The diff; `again` keeps what the page drew when the read fails. */
  function read(id: string, asked: string, alive: () => boolean, again = false): Promise<unknown> {
    reading = true;
    return ports.entityView(id, asked).then(
      (view) => {
        if (!alive()) return;
        reading = false;
        const block = view?.sections[0]?.blocks.find((candidate) => candidate.type === "diff");
        files = block?.type === "diff" ? block.files : [];
        cut = block?.type === "diff" && block.truncated;
        range = block?.type === "diff" ? (block.range ?? null) : null;
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
  function shown(end: string): string {
    const found = /^([0-9a-f]{40}|[0-9a-f]{64})(\^?)$/i.exec(end);
    return found ? `${found[1]!.slice(0, 10)}${found[2]}` : end;
  }
</script>

<div class="tf-diffpage">
  <h2 class="tf-cgroup-h">
    <a href={repoHash(repoId, "commits")}>{title ?? repoId}</a>
    <span class="tf-mono" data-testid="diff-range">{shown(base)} … {shown(head)}</span>
    <span class="tf-acts"><TfOpenObject entityId={repoId} object={{ kind: "range", base, head }} /></span>
  </h2>
  <TfWaiting phase={waitPhase({ drawn: files !== null, reading, failed: failed !== null })} error={failed} shape="lines">
    {#if files !== null && files.length === 0}
      <div class="tf-empty">Nothing changes between {shown(base)} and {shown(head)}.</div>
    {:else if files !== null}
      {#if range === null}
        <p class="tf-cnote" data-testid="no-range">Context does not expand here: the source did not settle which two commits this diff is between, as for a range from the repository's first commit or a branch that merged its base back in.</p>
      {/if}
      {#key `${section}\n${range?.base}\n${range?.head}`}
        <TfDiffView {files} {cut} {context} openAt={{ entityId: repoId, at: head }} />
      {/key}
    {/if}
  </TfWaiting>
</div>
