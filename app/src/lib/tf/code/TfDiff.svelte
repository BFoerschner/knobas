<!--
  One diff, file by file (#838), as prototype-6's `diffFile`: a file header
  with its path, `+added −removed` and *new file* where it is one, then its
  lines with their numbers, `⋯` between hunks. Drawn from the source's own
  diff (`ViewBlock` `diff`, `DiffFile`), so a commit, a pull request (#842)
  and the checkout's uncommitted change (#847) draw the same block.

  A removed line carries its old number, every other line its new one.
-->
<script lang="ts">
  import type { DiffFile } from "../../ipc/entity";

  let { files }: { files: readonly DiffFile[] } = $props();

  const SIGN = { context: " ", add: "+", del: "−" } as const;
</script>

{#each files as file (file.path)}
  <div class="tf-diff">
    <div class="tf-fh">
      <span class="tf-mono">{file.old_path && file.old_path !== file.path ? `${file.old_path} → ${file.path}` : file.path}</span>
      <span class="tf-add-n">+{file.additions}</span>
      <span class="tf-del-n">−{file.deletions}</span>
      {#if file.change === "added"}<span class="tf-chip">new file</span>{/if}
    </div>
    {#each file.hunks as hunk, at (at)}
      {#if at > 0}
        <div class="tf-ln tf-hunk"><span></span><span>⋯</span></div>
      {/if}
      {#each hunk.lines as line, n (n)}
        <div class="tf-ln" class:tf-add={line.kind === "add"} class:tf-del={line.kind === "del"}>
          <span>{line.kind === "del" ? line.old : line.new}</span><span>{SIGN[line.kind]} {line.text}</span>
        </div>
      {/each}
    {/each}
    {#if file.truncated}
      <div class="tf-ln tf-hunk"><span></span><span>Cut here: the rest of this file's change is at the source.</span></div>
    {/if}
  </div>
{/each}
