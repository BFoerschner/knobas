<!--
  The grouped answer: one heading per kind, then three-cell rows (#2421).

  The group order is the **backend's** — `knobas_search::group` fixes it
  (tickets, PRs, builds, …, unknown kinds last), so a launcher that sorted its
  own groups would be a second opinion about the same question. Same for
  `label`, `plural` and `monogram`: they come off the group, which the engine
  filled from the adapter's declared `KindInfo` (§3a), never from a table here.
-->
<script lang="ts">
  import type { SearchResponse } from "../ipc";
  import { clockLabel } from "../time/format";
  import { aimed } from "./aim";
  import Row from "./Row.svelte";
  import { stateWord } from "../tf/state-word";
  import { cellsOf } from "./cells";
  import { authorGaps, underGaps } from "./coverage";
  import { holdsAnswer, ticketsFirst, type LauncherRow } from "./rows";

  let {
    response,
    rows,
    selected,
    optionId,
    chordOf = () => null,
    sourceOf = () => null,
    within = null,
    filtered = false,
    now,
    onopen,
    onhover,
  }: {
    response: SearchResponse;
    /** The clock an inbox line's time cell is read against (#2423). */
    now?: Date | undefined;
    /** The flat selectable list, so an index means the same thing everywhere. */
    rows: LauncherRow[];
    selected: number;
    /** The element id of the option at a flat-list index (#1411). */
    optionId: (index: number) => string;
    /** The chord bound to a row (#2429), drawn at its end; none when omitted. */
    chordOf?: (row: LauncherRow) => string | null;
    /** The source a row names, where its key repeats in the list (#1476); none when omitted. */
    sourceOf?: (row: LauncherRow) => string | null;
    /** The section whose filter the box is on, which an empty answer's line names. */
    within?: string | null;
    /** Whether filters narrowed the search, so an empty answer is no claim about everything. */
    filtered?: boolean;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();

  /** The *Go to* rows, which come first in the flat list. */
  const placeCount = $derived(rows.filter((row) => row.kind === "action").length);

  /** Where each group's hits start in the flat list. */
  const offsets = $derived.by(() => {
    const out: number[] = [];
    let at = placeCount;
    for (const group of ticketsFirst(response)) {
      out.push(at);
      at += group.hits.length;
    }
    return out;
  });

  /**
   * The sources this query's author filter could not be asked (#141).
   *
   * Drawn above the results rather than below them: an `author:` search that
   * comes back empty is read at the top of an empty list, and an explanation
   * under the fold explains nothing.
   */
  const gaps = $derived(authorGaps(response));
  /** The same report for `under:` (#595), drawn the same way. */
  const parentGaps = $derived(underGaps(response));
</script>

{#if gaps.length > 0}
  <div class="tf-ln-gap">
    <!-- Deliberately not "not every source": that is false in the case a
         reader most needs this, `@jonas in:tc`, where the one source in scope is
         the one that cannot be asked. A heading that does not count is honest
         at one source and at five. -->
    <p class="tf-ln-gap-h">An author search cannot be answered by:</p>
    <ul>
      {#each gaps as gap (gap.sourceId)}
        <li><span class="tf-ln-gap-s">{gap.name}</span> — {gap.reason}</li>
      {/each}
    </ul>
  </div>
{/if}

{#if parentGaps.length > 0}
  <div class="tf-ln-gap">
    <p class="tf-ln-gap-h">An under: search cannot be answered by:</p>
    <ul>
      {#each parentGaps as gap (gap.sourceId)}
        <li><span class="tf-ln-gap-s">{gap.name}</span> — {gap.reason}</li>
      {/each}
    </ul>
  </div>
{/if}

<!-- Places the words name, above their answer: a name is the likeliest thing the reader is after. -->
{#if placeCount > 0}
  <div class="tf-ln-h"><span class="tf-ln-lab">Go to</span></div>
  {#each rows as row, index (row.id)}
    {#if row.kind === "action"}
      <button
        id={optionId(index)}
        class="tf-ln-opt tf-ln-go"
        class:tf-ln-on={index === selected}
        role="option"
        aria-selected={index === selected}
        onclick={() => onopen(row)}
        onpointermove={aimed(() => onhover(index))}
      >
        <span class="tf-ln-nm">{row.action.label}</span>
        <span class="tf-ln-s">{row.action.detail}</span>
      </button>
    {/if}
  {/each}
{/if}

<!-- No count on a heading (#2421): the list is quiet, and the rows are what is read. -->
{#each ticketsFirst(response) as group, g (group.kind)}
  <div class="tf-ln-h"><span class="tf-ln-lab">{group.plural}</span></div>
  {#each group.hits as hit, h (hit.entity_id)}
    {@const index = (offsets[g] ?? 0) + h}
    {@const row = rows[index]}
    {#if row}
      {@const cells = cellsOf(hit, group.label)}
      <Row
        monogram={group.monogram}
        tone={hit.state?.tone ?? null}
        kindName={hit.state ? `${group.label}: ${stateWord(hit.state.text)}` : group.label}
        cell={cells.cell}
        mono={cells.mono}
        title={cells.title}
        chord={chordOf(row)}
        source={sourceOf(row)}
        id={optionId(index)}
        selected={index === selected}
        onopen={() => onopen(row)}
        onhover={() => onhover(index)}
      />
    {/if}
  {/each}
{/each}

<!--
  The inbox's lines behind `#i` (#2423): after the engine's groups, under
  their own heading, the time as the key cell. `rows` holds them only for an
  answer the engine read as `#i`.
-->
{#if rows.some((row) => row.kind === "inbox")}
  <div class="tf-ln-h"><span class="tf-ln-lab">Inbox</span></div>
{/if}
{#each rows as row, index (row.id)}
  {#if row.kind === "inbox"}
    <Row
      monogram="IN"
      kindName="Inbox line"
      cell={clockLabel(row.item.occurred_at, now ?? new Date(), true)}
      mono
      title={row.item.title}
      id={optionId(index)}
      selected={index === selected}
      onopen={() => onopen(row)}
      onhover={() => onhover(index)}
    />
  {/if}
{/each}

<!-- *Search Docs for …* (#2425): last, after every group; `rows` holds it only for a `#d` answer with words. -->
{#each rows as row, index (row.id)}
  {#if row.kind === "docs"}
    <button
      id={optionId(index)}
      class="tf-ln-opt tf-ln-go"
      class:tf-ln-on={index === selected}
      role="option"
      aria-selected={index === selected}
      onclick={() => onopen(row)}
      onpointermove={aimed(() => onhover(index))}
    >
      <span class="tf-ln-nm">Search Docs for “{row.words}”</span>
      <span class="tf-ln-s">Every space, in the Docs area</span>
    </button>
  {/if}
{/each}

{#if !holdsAnswer(rows)}
  <!-- Under any place row, so the line is about the search and not about the row above it. -->
  <p class="tf-ln-none">
    {#if within !== null}Nothing in {within} matches{:else if placeCount > 0 && !filtered}No tickets, code, builds or docs match{:else}Nothing matches{/if}
    <strong>{response.interpreted.text || "these filters"}</strong>.
  </p>
  {#each rows as row, index (row.id)}
    {#if row.kind === "widen"}
      <button
        id={optionId(index)}
        class="tf-ln-opt tf-ln-go"
        class:tf-ln-on={index === selected}
        role="option"
        aria-selected={index === selected}
        onclick={() => onopen(row)}
        onpointermove={aimed(() => onhover(index))}
      >
        <span class="tf-ln-nm">Search everything</span>
        <span class="tf-ln-s">Every field, with no filters.</span>
      </button>
    {/if}
  {/each}
{/if}
