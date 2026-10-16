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
  import { ipcErrorMessage } from "../../ipc";
  import type { ChoiceOption, DiffFile, EntityDetail } from "../../ipc/entity";
  import TfDiff from "../code/TfDiff.svelte";
  import { when } from "../tickets/tickets";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { branchFrom, repoHash, shaOf } from "./repo";

  let {
    ports,
    repoId,
    commitId,
    now,
    onNewBranch,
  }: {
    ports: Pick<RepoPorts, "entityView" | "getEntity">;
    repoId: string;
    commitId: string;
    now: Date;
    /** Open *New branch…* starting at this commit: its whole sha the value, its short one the label. */
    onNewBranch: (from: ChoiceOption) => void;
  } = $props();

  let detail = $state<EntityDetail | null>(null);
  let files = $state<DiffFile[] | null>(null);
  let start = $state<ReturnType<typeof branchFrom>>(null);
  let failed = $state<string | null>(null);

  $effect(() => {
    const id = commitId;
    let live = true;
    detail = null;
    files = null;
    start = null;
    failed = null;
    const fail = (error: unknown) => {
      if (live) failed = ipcErrorMessage(error);
    };
    ports.getEntity(id).then((answer) => {
      if (live) detail = answer;
    }, fail);
    ports.entityView(id, null).then((view) => {
      if (!live) return;
      const diff = view?.sections.find((candidate) => candidate.id === "diff");
      const block = diff?.blocks.find((candidate) => candidate.type === "diff");
      files = block?.type === "diff" ? block.files : [];
      start = branchFrom(view);
    }, fail);
    return () => {
      live = false;
    };
  });

  const sha = $derived(shaOf(commitId));

  function branchHere() {
    if (start !== null && start.disabled === null) onNewBranch(start.from);
  }
</script>

<h2 class="tf-cgroup-h">
  {#if sha !== null}<span class="tf-mono">{sha}</span>{/if}
  {detail?.row.title ?? ""}
  {#if detail !== null}
    <span class="tf-dim tf-rc-who"
      >{[detail.author, detail.row.updated_at ? when(detail.row.updated_at, now) : null].filter(Boolean).join(", ")}</span
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
  </span>
</h2>
{#if failed !== null}
  <p class="tf-cfail">{failed}</p>
{:else if files !== null && files.length === 0}
  <div class="tf-empty">This commit changes no file.</div>
{:else if files !== null}
  <TfDiff {files} />
{/if}
