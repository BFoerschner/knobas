<!--
  One commit under the repository's *Commits* tab (#838, R-8.7), as
  prototype-6's `renderCommitView`: its short sha, message, author and time,
  *New branch from here…* (#839: the page's *New branch…* dialog, starting
  where the commit view's own *New branch…* starts, its whole sha) and *All
  commits*, then its diff file by file.

  The header is the mirror's row (`get_entity`: title, author, `updated_at`);
  the toolbar and the parent are the source's whole view (`entity_view`), read
  live. The diff is `entity_commit_diff` (#2319, story 78, ADR-0036): local
  read-only git when the repository's checkout holds the commit, with
  *Ignore whitespace* on offer; otherwise the commit's own `diff` section, the
  one source read it always was (the whole view defers it).
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { clockLabel } from "../../time/format";
  import { ipcErrorMessage } from "../../ipc";
  import type { ChoiceOption, DiffFile, DiffReadAt, EntityDetail, RangeDiff, SourceView } from "../../ipc/entity";
  import { commitContext, parentOf } from "../diff/context";
  import TfDiffView from "../diff/TfDiffView.svelte";
  import { shaOfCommit } from "../diff/address";
  import TfOpenObject from "../openin/TfOpenObject.svelte";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { onRefreshed } from "./refreshed.svelte";
  import { branchFrom, repoHash, shaOf } from "./repo";

  let {
    ports,
    repoId,
    commitId,
    now,
    onNewBranch,
    refreshed = 0,
  }: {
    ports: Pick<RepoPorts, "entityView" | "getEntity" | "fileBytes" | "piped" | "commitDiff">;
    repoId: string;
    commitId: string;
    now: Date;
    /** Open *New branch…* starting at this commit: its whole sha the value, its short one the label. */
    onNewBranch: (from: ChoiceOption) => void;
    /** The page's refreshes (#2135): each reads the commit again over what the tab drew. */
    refreshed?: number;
  } = $props();

  let detail = $state<EntityDetail | null>(null);
  let files = $state<DiffFile[] | null>(null);
  let start = $state<ReturnType<typeof branchFrom>>(null);
  /** The diff hit the parser's ceiling. */
  let cut = $state(false);
  /** The first parent's address, for the old side of expanded context. */
  let parent = $state<string | null>(null);
  let failed = $state<string | null>(null);
  /** Where the drawn diff was read; whitespace is ignored only at the checkout. */
  let readAt = $state<DiffReadAt | null>(null);
  let ignoreWhitespace = $state(false);
  const uid = $props.id();
  const why = `${uid}-ws-why`;

  $effect(() => {
    const id = commitId;
    let live = true;
    detail = null;
    files = null;
    start = null;
    failed = null;
    readAt = null;
    ignoreWhitespace = false;
    // Only a new commit reads it all again; Ignore whitespace re-reads the diff alone.
    untrack(() => readCommit(id, () => live));
    return () => {
      live = false;
    };
  });

  // A refresh of the page reads the commit again, over what the tab drew.
  onRefreshed(
    () => refreshed,
    () => {
      const id = commitId;
      return readCommit(id, () => commitId === id, true);
    },
  );

  /** The commit's diff alone, from a harness with no `commitDiff` port: its own section, at the source. */
  function viaSource(entityView: (entityId: string, section: string | null) => Promise<SourceView | null>, id: string): Promise<RangeDiff> {
    return entityView(id, "diff").then((view) => {
      const block = view?.sections[0]?.blocks.find((candidate) => candidate.type === "diff");
      return block?.type === "diff"
        ? { files: block.files, truncated: block.truncated, range: block.range ?? null, read_at: "source" }
        : { files: [], truncated: false, range: null, read_at: "source" };
    });
  }

  /** The diff, local-first; `again` keeps what the tab drew when the read fails. */
  function readDiff(id: string, ws: boolean, alive: () => boolean, again = false): Promise<unknown> {
    const sha = shaOfCommit(id);
    const read = ports.commitDiff !== undefined && sha !== null ? ports.commitDiff(repoId, id, sha, ws) : viaSource(ports.entityView, id);
    return read.then(
      (diff) => {
        if (!alive()) return;
        files = diff.files;
        cut = diff.truncated;
        readAt = diff.read_at;
      },
      (error) => {
        if (alive() && !again) failed = ipcErrorMessage(error);
      },
    );
  }

  /** Ignore whitespace, toggled: the diff alone is read again. */
  function toggleWhitespace(on: boolean) {
    ignoreWhitespace = on;
    const id = commitId;
    readDiff(id, on, () => commitId === id && ignoreWhitespace === on);
  }

  /** The commit's row, its view and its diff; `again` keeps what the tab drew when a read fails. */
  function readCommit(id: string, alive: () => boolean, again = false): Promise<unknown> {
    const mine = () => alive() && commitId === id;
    const fail = (error: unknown) => {
      if (mine() && !again) failed = ipcErrorMessage(error);
    };
    const rowRead = ports.getEntity(id).then((answer) => {
      if (mine()) detail = answer;
    }, fail);
    const viewRead = ports.entityView(id, null).then((view) => {
      if (!mine()) return;
      parent = parentOf(view);
      start = branchFrom(view);
    }, fail);
    const ws = ignoreWhitespace;
    const diffRead = readDiff(id, ws, () => mine() && ignoreWhitespace === ws, again);
    return Promise.all([rowRead, viewRead, diffRead]);
  }

  const sha = $derived(shaOf(commitId));
  /** The whole sha, for *Open in…*: a merge commit opens as any commit does. */
  const whole = $derived(shaOfCommit(commitId));

  function branchHere() {
    if (start !== null && start.disabled === null) onNewBranch(start.from);
  }
</script>

<h2 class="tf-cgroup-h">
  {#if sha !== null}<span class="tf-mono">{sha}</span>{/if}
  {detail?.row.title ?? ""}
  {#if detail !== null}
    <span class="tf-dim tf-rc-who"
      >{[detail.author, detail.row.updated_at ? clockLabel(detail.row.updated_at, now) : null].filter(Boolean).join(", ")}</span
    >
  {/if}
  <span class="tf-acts">
    {#if start !== null}
      <button
        type="button"
        class="tf-btn tf-sm"
        aria-disabled={start.disabled !== null ? "true" : undefined}
        title={start.disabled ?? undefined}
        onclick={branchHere}>New branch from here…</button>
    {/if}
    <a class="tf-btn tf-sm tf-ghost" href={repoHash(repoId, "commits")}>All commits</a>
    {#if whole !== null}<TfOpenObject entityId={repoId} object={{ kind: "commit", sha: whole }} />{/if}
  </span>
</h2>
{#if readAt !== null && failed === null}
  <div class="tf-diffws">
    <label class="tf-ck">
      <input
        type="checkbox"
        checked={readAt === "checkout" && ignoreWhitespace}
        onchange={(event) => toggleWhitespace(event.currentTarget.checked)}
        disabled={readAt !== "checkout"}
        aria-describedby={readAt === "checkout" ? undefined : why} />
      Ignore whitespace
    </label>
    {#if readAt !== "checkout"}
      <span class="tf-cnote" id={why} data-testid="ws-why"
        >Read from the source, whose API cannot leave whitespace out. A checkout holding this commit can.</span
      >
    {/if}
  </div>
{/if}
{#if failed !== null}
  <p class="tf-cfail">{failed}</p>
{:else if files !== null && files.length === 0}
  <div class="tf-empty">This commit changes no file.</div>
{:else if files !== null}
  <TfDiffView
    {files}
    {cut}
    context={commitContext(ports.entityView, commitId, parent, ports.fileBytes, ports.piped)}
    openAt={whole === null ? null : { entityId: repoId, at: whole }}
  />
{/if}
