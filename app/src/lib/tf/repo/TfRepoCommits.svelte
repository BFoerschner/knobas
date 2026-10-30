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

  *Diff selected* (#2271): a commit's box picks it, a shift-click on another's
  stretches the pick to it, and the button opens the diff address from the
  commit before the oldest picked to the newest (`diff/address.ts`). The pick
  is over the rows drawn; commits the filter hides between its ends are in
  the diff all the same, since it is a range of history. A commit the mirror
  does not hold has no sha, so its box can sit inside a pick but not
  end one. A new list (another branch, a filter, a refresh) drops the pick.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { ViewRow } from "../../ipc/entity";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { onRefreshed } from "./refreshed.svelte";
  import { commitHash, viewRows, visibleViewRows } from "./repo";
  import { diffHash, picked, rangeOf, selectRange, shaOfCommit, type RangePick } from "../diff/address";
  import TfWaiting from "../TfWaiting.svelte";
  import { waitPhase } from "../waiting";

  let {
    ports,
    repoId,
    at,
    branch,
    defaultBranch,
    refreshed = 0,
  }: {
    ports: Pick<RepoPorts, "entityView" | "codeFacts">;
    repoId: string;
    /** The entity the commits are read at: the repository, or the branch picked. */
    at: string;
    branch: string | null;
    defaultBranch: string | null;
    /** The page's refreshes (#2135): each reads the commits again over what the tab drew. */
    refreshed?: number;
  } = $props();

  /** The commits drawn: another branch's are kept, dimmed, while this one's are read (#2246). */
  let rows = $state<ViewRow[] | null>(null);
  let reading = $state(false);
  let failed = $state<string | null>(null);
  /** The commits whose declared author is mine. */
  let mine = $state<ReadonlySet<string>>(new Set());

  $effect(() => {
    const where = at;
    let live = true;
    failed = null;
    readCommits(where, () => live);
    return () => {
      live = false;
    };
  });

  // A refresh of the page reads the commits again, over what the tab drew.
  onRefreshed(
    () => refreshed,
    () => {
      const where = at;
      return readCommits(where, () => at === where, true);
    },
  );

  /** The commits at `where`, then whose are mine; `again` keeps what the tab drew when it fails. */
  function readCommits(where: string, alive: () => boolean, again = false): Promise<unknown> {
    reading = true;
    return ports.entityView(where, "commits").then(
      (view) => {
        if (!alive()) return;
        const read = viewRows(view?.sections[0]);
        reading = false;
        rows = read;
        mine = new Set();
        const ids = read.flatMap((row) => (row.entity_id === null ? [] : [row.entity_id]));
        if (ids.length === 0) return;
        return ports.codeFacts(ids).then(
          (facts) => {
            if (alive()) mine = new Set(facts.filter((entry) => entry.mine).map((entry) => entry.entity_id));
          },
          () => {},
        );
      },
      (error) => {
        if (!alive()) return;
        reading = false;
        if (again) return;
        failed = ipcErrorMessage(error);
        rows = null;
      },
    );
  }

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

  /** The commits picked for *Diff selected*, as rows of `shown`. */
  let pick = $state<RangePick | null>(null);
  $effect(() => {
    void shown;
    pick = null;
  });
  const range = $derived(pick === null ? null : rangeOf(shown, pick));
  const count = $derived(pick === null ? 0 : Math.abs(pick.anchor - pick.end) + 1);

  /** A box clicked: a click picks its commit, a shift-click stretches the pick to it. */
  function choose(event: MouseEvent, at: number) {
    pick = selectRange(pick, at, event.shiftKey);
    for (const [n, box] of boxes.entries()) if (box) box.checked = picked(pick, n);
  }
  let boxes = $state<(HTMLInputElement | undefined)[]>([]);

  function diffSelected() {
    if (range !== null) location.hash = diffHash(repoId, range.base, range.head);
  }

  /** A row opens its commit; a click on the link or the box inside already did its own. */
  function open(event: MouseEvent, row: ViewRow) {
    if (row.entity_id === null || (event.target as Element | null)?.closest("a, input")) return;
    location.hash = commitHash(repoId, row.entity_id);
  }
</script>

<div class="tf-rbar tf-rpickbar">
  <FilterBar {config} {filter} shown={rows === null ? null : shown.length} total={rows?.length ?? null} />
  <button
    type="button"
    class="tf-btn tf-sm"
    disabled={range === null}
    title={range === null ? "Pick a commit's box, then shift-click another's for a range" : undefined}
    onclick={diffSelected}>Diff selected{count > 0 ? ` (${count})` : ""}</button>
</div>
<TfWaiting phase={waitPhase({ drawn: rows !== null, reading, failed: failed !== null })} error={failed}>
{#if shown.length === 0}
  <div class="tf-empty">No commits match.</div>
{:else}
  <table class="tf-tbl tf-rcommits">
    <tbody>
      {#each shown as row, n (n)}
        <tr class:tf-row={row.entity_id !== null} class:tf-picked={picked(pick, n)} onclick={(event) => open(event, row)}>
          <td class="tf-rpick"
            ><input
              type="checkbox"
              bind:this={boxes[n]}
              checked={picked(pick, n)}
              disabled={shaOfCommit(row.entity_id) === null && !picked(pick, n)}
              aria-label={`Pick ${row.chip ?? row.title}`}
              onclick={(event) => choose(event, n)}
            /></td
          >
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
</TfWaiting>
