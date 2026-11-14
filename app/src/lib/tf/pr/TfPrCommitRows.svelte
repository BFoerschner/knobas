<!--
  One list of a pull request's commits on its *Commits* tab (#840): the short sha linking the commit's diff, its title
  and its line (who, when). A pull request of hundreds of commits draws only the rows near the viewport (the extremes
  pass, 2026-11-13: 500 commits were 1,500 cells built as the tab opened).
-->
<script lang="ts">
  import type { ViewRow } from "../../ipc/entity";
  import { Windowed } from "../../shell/windowed.svelte";
  import { commitLine } from "../repo/file-tree";
  import { commitHash } from "../repo/repo";

  let {
    commits,
    repoId,
    mine,
    onopen,
  }: {
    commits: ViewRow[];
    /** The repository the commits are in, where the page knows it: their sha links its commit page. */
    repoId: string | null;
    /** The reader's own commits, by entity id. */
    mine: ReadonlySet<string>;
    /** A press on the row, off its link. */
    onopen: (event: MouseEvent, row: ViewRow) => void;
  } = $props();

  /** By place: a commit list is the source's, in its order, and a row with no id has only its place. */
  const win = new Windowed({ count: () => commits.length, key: (i) => i, estimate: 37 });
</script>

<table class="tf-tbl tf-rcommits">
  <tbody {@attach win.list}>
    {#if win.before > 0}<tr class="tf-wgap" aria-hidden="true"><td colspan="3" {@attach win.room("before")}></td></tr>{/if}
    {#each win.slice(commits) as row, n (win.start + n)}
      {@const opens = row.entity_id !== null && repoId !== null}
      <tr class:tf-row={opens} onclick={(event) => onopen(event, row)} {@attach win.row(win.start + n)}>
        <td class="tf-mono"
          >{#if opens}<a href={commitHash(repoId!, row.entity_id!)}>{row.chip ?? ""}</a>{:else}{row.chip ?? ""}{/if}</td
        >
        <td class:tf-dim={row.chip === null}>{row.title}</td>
        <td class="tf-dim tf-nowrap">{commitLine(row, row.entity_id !== null && mine.has(row.entity_id))}</td>
      </tr>
    {/each}
    {#if win.after > 0}<tr class="tf-wgap" aria-hidden="true"><td colspan="3" {@attach win.room("after")}></td></tr>{/if}
  </tbody>
</table>
