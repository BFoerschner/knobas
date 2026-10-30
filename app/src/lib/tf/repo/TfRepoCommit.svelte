<!--
  One commit under the repository's *Commits* tab (#838, R-8.7), as
  prototype-6's `renderCommitView`: its short sha, message, author and time,
  *New branch from here…* (#839: the page's *New branch…* dialog, starting
  where the commit view's own *New branch…* starts, its whole sha) and *All
  commits*, then its diff file by file.

  The header is the mirror's row (`get_entity`: title, author, `updated_at`);
  the diff and the toolbar are the source's whole view (`entity_view`), read
  live.
-->
<script lang="ts">
  import { clockLabel } from "../../time/format";
  import { ipcErrorMessage } from "../../ipc";
  import type { ChoiceOption, DiffFile, EntityDetail } from "../../ipc/entity";
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
    ports: Pick<RepoPorts, "entityView" | "getEntity" | "fileBytes" | "piped">;
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

  $effect(() => {
    const id = commitId;
    let live = true;
    detail = null;
    files = null;
    start = null;
    failed = null;
    readCommit(id, () => live);
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

  /** The commit's row and its diff; `again` keeps what the tab drew when a read fails. */
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
      const diff = view?.sections.find((candidate) => candidate.id === "diff");
      const block = diff?.blocks.find((candidate) => candidate.type === "diff");
      files = block?.type === "diff" ? block.files : [];
      cut = block?.type === "diff" && block.truncated;
      parent = parentOf(view);
      start = branchFrom(view);
    }, fail);
    return Promise.all([rowRead, viewRead]);
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
