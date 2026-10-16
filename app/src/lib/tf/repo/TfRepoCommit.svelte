<!--
  One commit under the repository's *Commits* tab (#838, R-8.7), as
  prototype-6's `renderCommitView`: its short sha, message, author and time,
  *New branch from here…* (the dialog is #839's, not built yet) and *All
  commits*, then its diff file by file.

  The header is the mirror's row (`get_entity`: title, author, `updated_at`);
  the diff is the source's (`entity_view`, section `diff`), read live.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { DiffFile, EntityDetail } from "../../ipc/entity";
  import TfDiff from "../code/TfDiff.svelte";
  import { when } from "../tickets/tickets";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { repoHash, shaOf } from "./repo";

  let {
    ports,
    repoId,
    commitId,
    now,
  }: {
    ports: Pick<RepoPorts, "entityView" | "getEntity">;
    repoId: string;
    commitId: string;
    now: Date;
  } = $props();

  let detail = $state<EntityDetail | null>(null);
  let files = $state<DiffFile[] | null>(null);
  let failed = $state<string | null>(null);

  $effect(() => {
    const id = commitId;
    let live = true;
    detail = null;
    files = null;
    failed = null;
    const fail = (error: unknown) => {
      if (live) failed = ipcErrorMessage(error);
    };
    ports.getEntity(id).then((answer) => {
      if (live) detail = answer;
    }, fail);
    ports.entityView(id, "diff").then((view) => {
      if (!live) return;
      const block = view?.sections[0]?.blocks.find((candidate) => candidate.type === "diff");
      files = block?.type === "diff" ? block.files : [];
    }, fail);
    return () => {
      live = false;
    };
  });

  const sha = $derived(shaOf(commitId));
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
    <button type="button" class="tf-btn tf-sm" aria-disabled="true" title="Not built in the new shell yet"
      >New branch from here…</button
    >
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
