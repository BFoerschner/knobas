<!--
  The repository page's *Commits* tab (#838, R-8.7): the filter, then each
  commit's short sha, message and who and when, as prototype-6's
  `renderRepo(...,'commits')`.

  The commits are the source's view of the branch picked (`entity_view`,
  section `commits`): on the default branch its latest commits, on another
  what that branch carries over the default one. A row's who and when is the
  source's own line (`author · age`), with *You* for the reader's own. A commit the mirror
  holds opens its diff; one it does not has no id to open and is drawn plain.

  *Author: Mine* (#973) keeps the commits whose declared author is mine
  (`code_facts`), never a reading of that line: a commit the mirror does not
  hold has no declared author, so the chip leaves it out.

  *Diff selected* (#2271): a commit's box picks it, a shift-click on another's
  stretches the pick to it, and the button opens the diff address from the
  commit before the oldest picked to the newest (`diff/address.ts`). The pick
  is over the rows drawn; commits the filter hides between its ends are in
  the diff all the same, since it is a range of history. A commit the mirror
  does not hold has no sha, so its box is never a pick's end: it is
  disabled, and drawn checked when a pick spans it. A new list (another branch, a filter, a refresh) drops the pick.

  *Open in…* (#2274): a commit the mirror holds has the menu at the row's end,
  offering what accepts a commit and *Browser*; a click in it is not the row's.

  **On the working tree** (`disk`, 2026-11-21, `docs/decisions/2026-11-21-the-working-tree-is-a-choice-in-the-branch-box.md`):
  the table starts with *Uncommitted changes* where the checkout has any (it opens *Local changes*), then the commits
  the checkout has that the forge lacks, read from the checkout (`entity_range_files` at `~working-tree`, ADR-0049:
  against the mirror's tip of the checkout's branch, or the default branch where the forge lacks it), each *not pushed*
  and drawn plain. A branch the forge lacks says so in a line over the table. The tab draws once both reads have
  answered, so no row comes in above another.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { RangeCommit, ShownView, ViewRow } from "../../ipc/entity";
  import type { WriteSource } from "../../shell/queued-toast";
  import TfHeldNote from "../TfHeldNote.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import TfNoMatch from "../filter/TfNoMatch.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import { untrack } from "svelte";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { onRefreshed } from "./refreshed.svelte";
  import { onViewFresh } from "./view-fresh.svelte";
  import { commitLine } from "./file-tree";
  import { commitHash, repoHash, sourceName, viewRows, visibleViewRows } from "./repo";
  import { diffHash, picked, rangeOf, selectRange, shaOfCommit, WORKING_TREE, type RangePick } from "../diff/address";
  import TfStateChip from "../TfStateChip.svelte";
  import TfWaiting from "../TfWaiting.svelte";
  import TfOpenObject from "../openin/TfOpenObject.svelte";
  import { waitPhase } from "../waiting";

  let {
    ports,
    repoId,
    at,
    branch,
    defaultBranch,
    source = null,
    refreshed = 0,
    disk = null,
  }: {
    ports: Pick<RepoPorts, "entityView" | "codeFacts" | "listen" | "rangeFiles">;
    repoId: string;
    /** The entity the commits are read at: the repository, or the branch picked. */
    at: string;
    branch: string | null;
    defaultBranch: string | null;
    /** The repository's source, which the held line names. */
    source?: WriteSource | null;
    /** The page's refreshes (#2135): each reads the commits again over what the tab drew. */
    refreshed?: number;
    /**
     * The working tree chosen in the branch box: the checkout's branch (`null` detached), whether the forge has it (`null`
     * while the branches are being read), what its unpushed commits are counted against, how many paths are uncommitted,
     * how many commits git counts ahead of the upstream, and a stamp that changes with any of it.
     */
    disk?: { branch: string | null; onForge: boolean | null; base: string | null; changed: number; ahead: number; stamp: string } | null;
  } = $props();

  /** The commits drawn: another branch's are kept, dimmed, while this one's are read (#2246). */
  let rows = $state<ViewRow[] | null>(null);
  let reading = $state(false);
  let failed = $state<string | null>(null);
  /** The commits whose declared author is mine. */
  let mine = $state<ReadonlySet<string>>(new Set());
  /** How many commits the branch has, where the source counts them: it may hold only the latest of them. */
  let counted = $state<number | null>(null);
  /**
   * When the commits drawn were read, where knobas held them (ADR-0042), and why the read behind them failed: a branch
   * never read draws the tip the last sync stored at once (the inner-pages pass, 2026-11-12), and the rest join after it.
   */
  let heldAt = $state<string | null>(null);
  let heldFailed = $state<string | null>(null);

  $effect(() => {
    const where = at;
    let live = true;
    failed = null;
    readCommits(where, () => live);
    return () => {
      live = false;
    };
  });

  /** On the working tree, the checkout's commits the forge lacks, newest first; `null` until read. */
  let unpushed = $state<RangeCommit[] | null>(null);
  let unpushedRead = 0;
  /** What the last read was for: the page hands a new `disk` on any change of its own, which asks nothing new. */
  let unpushedFor: string | null = null;
  $effect(() => {
    const was = disk;
    const key = was === null ? null : JSON.stringify(was);
    if (key === unpushedFor) return;
    unpushedFor = key;
    if (was === null) return void (unpushed = null);
    // A branch the forge has, with nothing ahead of its upstream, has nothing to list: no read.
    const asks = was.branch !== null && was.base !== null && was.onForge !== null && (!was.onForge || was.ahead > 0);
    if (!asks || ports.rangeFiles === undefined) return void (unpushed = was.onForge === null ? null : []);
    const id = ++unpushedRead;
    // Read again on a new stamp behind the rows drawn, which stay until it lands.
    untrack(() => ports.rangeFiles!)(repoId, was.base!, WORKING_TREE, false, `commits-unpushed-${repoId}`).then(
      (list) => {
        if (id === unpushedRead) unpushed = list.range?.commits ?? [];
      },
      () => {
        // A checkout git can't list leaves the forge's commits as they are.
        if (id === unpushedRead) unpushed = [];
      },
    );
  });
  /** The working tree's own rows are drawn: none to wait for, or both reads answered. */
  const diskReady = $derived(disk === null || unpushed !== null);

  // Kept by the app and read again behind its answer (PR #2479): a move is drawn over what the tab drew.
  onViewFresh(
    () => at,
    "commits",
    () => {
      const where = at;
      void readCommits(where, () => at === where, true);
    },
    untrack(() => ports.listen),
    (why) => (heldFailed = why),
  );

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
        heldAt = (view as ShownView | null)?.held_at ?? null;
        heldFailed = null;
        rows = read;
        counted = view?.sections[0]?.count == null ? null : Number(view.sections[0].count);
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
    placeholder: "Filter commits",
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
  /** The working tree's rows the filter keeps: none under *Mine*, which reads only what the mirror declares. */
  const mineOnly = $derived((filter.chips["who"] ?? []).includes("Mine"));
  const typed = $derived(filter.text.trim().toLowerCase());
  const localShown = $derived(
    mineOnly || unpushed === null ? [] : unpushed.filter((commit) => typed === "" || [commit.sha.slice(0, 7), commit.title, commit.meta ?? ""].some((text) => text.toLowerCase().includes(typed))),
  );
  const dirtyShown = $derived(disk !== null && disk.changed > 0 && !mineOnly && (typed === "" || "uncommitted changes".includes(typed)));
  const anyLocal = $derived(dirtyShown || localShown.length > 0);

  /** The commits picked for *Diff selected*, as rows of `shown`. */
  let pick = $state<RangePick | null>(null);
  $effect(() => {
    void shown;
    pick = null;
  });
  const range = $derived(pick === null ? null : rangeOf(shown, pick));
  /** Whether any commit on screen can be picked: a list of commits the mirror holds no sha for offers no *Diff selected*. */
  const anyPickable = $derived(shown.some((row) => shaOfCommit(row.entity_id) !== null));
  const count = $derived(pick === null ? 0 : Math.abs(pick.anchor - pick.end) + 1);

  /** A box clicked: a click picks its commit, a shift-click stretches the pick to it. */
  function choose(event: MouseEvent, at: number) {
    pick = selectRange(pick, at, event.shiftKey);
    // The browser has already toggled the clicked box; `checked=` is only set
    // again when its value changes, so a box whose pick did not change would
    // keep the browser's toggle. Every box is set to the pick here.
    for (const [n, box] of boxes.entries()) if (box) box.checked = picked(pick, n);
  }
  let boxes = $state<(HTMLInputElement | undefined)[]>([]);

  function diffSelected() {
    if (range !== null) location.hash = diffHash(repoId, range.base, range.head);
  }

  /** A row opens its commit; a click on the link or the box inside already did its own. */
  function open(event: MouseEvent, row: ViewRow) {
    if (row.entity_id === null || (event.target as Element | null)?.closest("a, input, .tf-openin")) return;
    location.hash = commitHash(repoId, row.entity_id);
  }
</script>

<div class="tf-rbar tf-rpickbar">
  <FilterBar {config} {filter} shown={rows === null ? null : shown.length} total={rows === null ? null : Math.max(rows.length, counted ?? 0)} />
  <button
    type="button"
    class="tf-btn tf-sm"
    disabled={range === null}
    hidden={!anyPickable}
    title={range === null ? "Pick a commit's box, then shift-click another's for a range" : undefined}
    onclick={diffSelected}>Diff selected{count > 0 ? ` (${count})` : ""}</button>
</div>
<TfHeldNote {heldAt} source={sourceName(source)} failed={heldFailed} />
{#if disk !== null && disk.onForge === false && disk.branch !== null}
  <!-- The checkout's branch is not on the forge: what the table lists under it is the default branch's. -->
  <p class="tf-heldnote tf-wtnote">{disk.branch} is not on {sourceName(source)} yet. Push it to open a pull request from it.</p>
{/if}
<TfWaiting phase={waitPhase({ drawn: rows !== null && diskReady, reading, failed: failed !== null })} error={failed}>
{#if rows !== null && rows.length === 0 && !anyLocal}
  <div class="tf-empty"><p>{branch !== null && branch !== defaultBranch ? `${branch} has no commits ${defaultBranch ?? "its base"} lacks.` : "This branch has no commits yet."}</p></div>
{:else if shown.length === 0 && !anyLocal}
  <TfNoMatch noun="commits" {filter} />
{:else}
  <table class="tf-tbl tf-rcommits">
    <tbody>
      {#if dirtyShown}
        <tr class="tf-row tf-rlocal" onclick={(event) => (event.target as Element | null)?.closest("a") || (location.hash = repoHash(repoId, "local"))}>
          {#if anyPickable}<td class="tf-rpick"></td>{/if}
          <td class="tf-mono"></td>
          <td><a href={repoHash(repoId, "local")}>Uncommitted changes</a></td>
          <td class="tf-dim tf-nowrap"><span class="tf-rowval">{disk!.changed} {disk!.changed === 1 ? "file" : "files"} changed</span></td>
          <td class="tf-ropen"></td>
        </tr>
      {/if}
      {#each localShown as commit (commit.sha)}
        <!-- Read from the checkout: the mirror holds no id for it yet, so it is drawn plain. -->
        <tr class="tf-rlocal" title={`${commit.sha.slice(0, 7)} is in your checkout, not on ${sourceName(source)}`}>
          {#if anyPickable}<td class="tf-rpick"></td>{/if}
          <td class="tf-mono"><Marked text={commit.sha.slice(0, 7)} typed={filter.text} /></td>
          <td><Marked text={commit.title} typed={filter.text} /> <TfStateChip text="not pushed" tone="warn" /></td>
          <td class="tf-dim tf-nowrap"><span class="tf-rowval"><Marked text={commit.meta ?? ""} typed={filter.text} /></span></td>
          <td class="tf-ropen"></td>
        </tr>
      {/each}
      {#each shown as row, n (n)}
        <tr class:tf-row={row.entity_id !== null} class:tf-picked={picked(pick, n)} onclick={(event) => open(event, row)}>
          {#if anyPickable}
          <td class="tf-rpick"
            ><input
              type="checkbox"
              bind:this={boxes[n]}
              checked={picked(pick, n)}
              disabled={shaOfCommit(row.entity_id) === null}
              aria-label={`Pick ${row.chip ?? row.title}`}
              onclick={(event) => choose(event, n)}
            /></td
          >
          {/if}
          <td class="tf-mono"
            >{#if row.entity_id !== null}<a href={commitHash(repoId, row.entity_id)}
                ><Marked text={row.chip ?? ""} typed={filter.text} /></a
              >{:else}<Marked text={row.chip ?? ""} typed={filter.text} />{/if}</td
          >
          <td class:tf-dim={row.chip === null}><Marked text={row.title} typed={filter.text} /></td>
          <td class="tf-dim tf-nowrap"><span class="tf-rowval"><Marked text={commitLine(row, row.entity_id !== null && mine.has(row.entity_id))} typed={filter.text} /></span></td>
          <td class="tf-ropen"
            >{#if shaOfCommit(row.entity_id) !== null}<span class="tf-rowend"><span class="tf-rowacts"><TfOpenObject
                entityId={repoId}
                object={{ kind: "commit", sha: shaOfCommit(row.entity_id)! }}
              /></span></span>{/if}</td
          >
        </tr>
      {/each}
    </tbody>
  </table>
{/if}
</TfWaiting>
