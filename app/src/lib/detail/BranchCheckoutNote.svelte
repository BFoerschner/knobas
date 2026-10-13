<!--
  On a branch detail, whether this machine's checkout of the repo is on this
  branch (#704): *Checked out locally · 2 to push*.

  Drawn by the shell from the checkout panel's read, never by the adapter,
  which knows nothing of the disk. The branch name is the mirror's and is only
  compared with what git reported (ADR-0021). Nothing is drawn when the
  checkout is elsewhere, detached, or not read yet.
-->
<script lang="ts">
  import type { TreeRead } from "../ipc/entity";
  import { branchNote } from "./working-tree";

  let { tree, branch }: { tree: TreeRead | null; branch: string } = $props();

  const note = $derived(branchNote(tree, branch));
</script>

{#if note}
  <!--
    Not `.k`: inside `.d-title` that class carries the key's size and a muted
    colour that would outrank the tone.
  -->
  <div class="d-local {note.tone}">{note.text}</div>
{/if}

<style>
  .d-local {
    font: 12px/1.5 var(--mono);
    font-variant-numeric: tabular-nums;
    margin-top: 4px;
  }
</style>
