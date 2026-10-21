<!--
  The grouped answer (spec §4: "results grouped by type with source monogram
  and sync age").

  The group order is the **backend's** — `knobas_search::group` fixes it
  (tickets, PRs, builds, …, unknown kinds last), so a launcher that sorted its
  own groups would be a second opinion about the same question. Same for
  `label`, `plural` and `monogram`: they come off the group, which the engine
  filled from the adapter's declared `KindInfo` (§3a), never from a table here.
-->
<script lang="ts">
  import type { SearchResponse } from "../ipc";
  import type { CredentialHealth } from "../ipc/sources";
  import Row from "./Row.svelte";
  import { authorGaps, underGaps } from "./coverage";
  import { sharesKey } from "./format";
  import type { LauncherRow } from "./rows";

  let {
    response,
    rows,
    selected,
    optionId,
    sources,
    now,
    onopen,
    onhover,
  }: {
    response: SearchResponse;
    /** The flat selectable list, so an index means the same thing everywhere. */
    rows: LauncherRow[];
    selected: number;
    /** The element id of the option at a flat-list index (#1411). */
    optionId: (index: number) => string;
    sources: CredentialHealth[];
    now?: Date | undefined;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();

  /** Rows another source's row of the same key sits beside (#1476): they name their source. */
  const shares = $derived(sharesKey(response.groups.flatMap((group) => group.hits)));
  /** Where each group's hits start in the flat list. */
  const offsets = $derived.by(() => {
    const out: number[] = [];
    let at = 0;
    for (const group of response.groups) {
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
         reader most needs this, `@jonas /tc`, where the one source in scope is
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

{#each response.groups as group, g (group.kind)}
  <div class="tf-ln-h">
    <span class="tf-ln-lab">{group.plural}</span>
    <span class="tf-ln-n">
      {group.total}
      {#if group.total > group.hits.length}
        <span class="tf-ln-more">· {group.total - group.hits.length} more</span>
      {/if}
    </span>
  </div>
  {#each group.hits as hit, h (hit.entity_id)}
    {@const index = (offsets[g] ?? 0) + h}
    {@const row = rows[index]}
    {#if row}
      <Row
        entityId={hit.entity_id}
        kind={hit.kind}
        sourceId={hit.source_id}
        title={hit.title}
        syncedAt={hit.synced_at}
        path={hit.path}
        state={hit.state}
        snippet={hit.snippet}
        named={shares(hit)}
        {sources}
        {now}
        id={optionId(index)}
        selected={index === selected}
        onopen={() => onopen(row)}
        onhover={() => onhover(index)}
      />
    {/if}
  {/each}
{/each}

{#if response.groups.length === 0}
  <p class="tf-ln-none">
    Nothing in the mirror matches
    <strong>{response.interpreted.text || "these filters"}</strong>.
  </p>
{/if}
