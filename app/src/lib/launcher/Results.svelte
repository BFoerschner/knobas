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
  import { authorGaps } from "./coverage";
  import type { LauncherRow } from "./rows";

  let {
    response,
    rows,
    selected,
    sources,
    now,
    onopen,
    onhover,
  }: {
    response: SearchResponse;
    /** The flat selectable list, so an index means the same thing everywhere. */
    rows: LauncherRow[];
    selected: number;
    sources: CredentialHealth[];
    now?: Date | undefined;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();

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
</script>

{#if gaps.length > 0}
  <div class="gap">
    <!-- Deliberately not "not every source": that is false in the case a
         reader most needs this, `@jonas /tc`, where the one source in scope is
         the one that cannot be asked. A heading that does not count is honest
         at one source and at five. -->
    <p class="gap-h">An author search cannot be answered by:</p>
    <ul>
      {#each gaps as gap (gap.sourceId)}
        <li><span class="gap-s">{gap.name}</span> — {gap.reason}</li>
      {/each}
    </ul>
  </div>
{/if}

{#each response.groups as group, g (group.kind)}
  <div class="secl">
    <span class="lab">{group.plural}</span>
    <span class="n">
      {group.total}
      {#if group.total > group.hits.length}
        <span class="more">· {group.total - group.hits.length} more</span>
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
        snippet={hit.snippet}
        {sources}
        {now}
        selected={index === selected}
        onopen={() => onopen(row)}
        onhover={() => onhover(index)}
      />
    {/if}
  {/each}
{/each}

{#if response.groups.length === 0}
  <p class="none">
    Nothing in the local index matches
    <span class="mono">{response.interpreted.text || "these filters"}</span>.
  </p>
{/if}

<style>
  /* `.secl` — the sticky group heading (round 3, line 411). */
  .secl {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 24px;
    padding: 0 12px 0 8px;
    border-bottom: 1px solid var(--hair);
    background: var(--panel);
    position: sticky;
    top: 0;
    z-index: 1;
  }
  .n {
    margin-left: auto;
    font: 400 11px var(--mono);
    color: var(--faint);
  }
  /* §4 asks for the count *and* what the page left behind: a group of 400
     showing 10 has to say so, or the reader thinks there are ten. */
  .more {
    color: var(--muted);
  }
  .none {
    padding: 14px 12px;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.5;
  }
  /* The #141 explanation. Amber-ruled rather than red: a source that cannot be
     asked an author question is not a failure, it is a fact about the corpus,
     and drawing it as an error would put a fault where there is none. */
  .gap {
    padding: 10px 12px;
    border-bottom: 1px solid var(--hair);
    box-shadow: inset 2px 0 0 var(--amber);
    background: var(--panel);
    font-size: 12px;
    line-height: 1.5;
    color: var(--muted);
  }
  .gap-h {
    color: var(--text);
  }
  .gap ul {
    margin: 2px 0 0;
    padding: 0;
    list-style: none;
  }
  .gap-s {
    color: var(--text);
  }
</style>
