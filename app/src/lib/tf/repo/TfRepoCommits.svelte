<!--
  The repository page's *Commits* tab (#838, R-8.7): the filter, then each
  commit's short sha, message and who and when, as prototype-6's
  `renderRepo(...,'commits')`.

  The commits are the source's view of the branch picked (`entity_view`,
  section `commits`): on the default branch its latest commits, on another
  what that branch carries over the default one. A row's who and when is the
  source's own line (`author · age`), drawn as it came. A commit the mirror
  holds opens its diff; one it does not has no id to open and is drawn plain.

  *Author: Mine* (#973) keeps the commits whose declared author is mine
  (`code_facts`), never a reading of that line: a commit the mirror does not
  hold has no declared author, so the chip leaves it out.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { ViewRow } from "../../ipc/entity";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { commitHash, viewRows, visibleViewRows } from "./repo";

  let {
    ports,
    repoId,
    at,
    branch,
    defaultBranch,
  }: {
    ports: Pick<RepoPorts, "entityView" | "codeFacts">;
    repoId: string;
    /** The entity the commits are read at: the repository, or the branch picked. */
    at: string;
    branch: string | null;
    defaultBranch: string | null;
  } = $props();

  let rows = $state<ViewRow[] | null>(null);
  let failed = $state<string | null>(null);
  /** The commits whose declared author is mine. */
  let mine = $state<ReadonlySet<string>>(new Set());

  $effect(() => {
    const where = at;
    let live = true;
    rows = null;
    failed = null;
    mine = new Set();
    ports.entityView(where, "commits").then(
      (view) => {
        if (!live) return;
        const read = viewRows(view?.sections[0]);
        rows = read;
        const ids = read.flatMap((row) => (row.entity_id === null ? [] : [row.entity_id]));
        if (ids.length === 0) return;
        ports.codeFacts(ids).then(
          (facts) => {
            if (live) mine = new Set(facts.filter((entry) => entry.mine).map((entry) => entry.entity_id));
          },
          () => {},
        );
      },
      (error) => {
        if (live) failed = ipcErrorMessage(error);
      },
    );
    return () => {
      live = false;
    };
  });

  const filter = $derived(sessionFilter(`repo-commits:${repoId}`));
  const config = $derived<FilterConfig>({
    id: `repo-commits:${repoId}`,
    placeholder: "Filter commits by message, sha or author",
    noun:
      branch === null
        ? "commits"
        : branch === defaultBranch || defaultBranch === null
          ? `commits on ${branch}`
          : `commits on ${branch} not in ${defaultBranch}`,
    chips: [{ key: "who", label: "Author", options: [{ value: "Mine", label: "Mine" }] }],
  });
  const shown = $derived(
    rows === null ? [] : visibleViewRows(rows, filter.text, (filter.chips["who"] ?? []).includes("Mine") ? mine : null),
  );

  /** A row opens its commit; a click on the link inside already did. */
  function open(event: MouseEvent, row: ViewRow) {
    if (row.entity_id === null || (event.target as Element | null)?.closest("a")) return;
    location.hash = commitHash(repoId, row.entity_id);
  }
</script>

<div class="tf-rbar">
  <FilterBar {config} {filter} shown={rows === null ? null : shown.length} total={rows?.length ?? null} />
</div>
{#if failed !== null}
  <p class="tf-cfail">{failed}</p>
{:else if rows !== null && shown.length === 0}
  <div class="tf-empty">No commits match.</div>
{:else if shown.length > 0}
  <table class="tf-tbl tf-rcommits">
    <tbody>
      {#each shown as row, n (n)}
        <tr class:tf-row={row.entity_id !== null} onclick={(event) => open(event, row)}>
          <td class="tf-mono"
            >{#if row.entity_id !== null}<a href={commitHash(repoId, row.entity_id)}
                ><Marked text={row.chip ?? ""} typed={filter.text} /></a
              >{:else}<Marked text={row.chip ?? ""} typed={filter.text} />{/if}</td
          >
          <td class:tf-dim={row.chip === null}><Marked text={row.title} typed={filter.text} /></td>
          <td class="tf-dim tf-nowrap"><Marked text={row.meta ?? ""} typed={filter.text} /></td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}
