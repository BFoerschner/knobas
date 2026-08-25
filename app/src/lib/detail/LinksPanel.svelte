<!--
  What this item is linked to — spec §5a's core object.

  **Always empty in M1.** `knobas.link` exists (migration `0001`) and
  `knobas_core::link` can write it, but nothing in M1 does: drawing a link is
  the M2 identity release, and `EntityDetail.links` is `[]` for every entity
  in this milestone (interfaces §2.5).

  It ships now because this panel is where every later milestone hangs — the
  suggestion tray, *Link to…*, the unlink tombstone — and writing it against
  the real `LinkRow` shape costs ten lines today and saves a redesign later.
  What it must not do is *pretend*: the empty state says nothing is linked and
  the reason it cannot be, and nothing on it is clickable.
-->
<script lang="ts">
  import type { LinkRow } from "../ipc/entity";
  import { ago } from "../shell/time";

  let { entityId, links }: { entityId: string; links: LinkRow[] } = $props();

  /**
   * Links read undirected (`knobas_core::link::links_of` returns both
   * directions), so the row shows the *other* end whichever way it was drawn.
   */
  const rows = $derived(
    links.map((link) => ({
      ...link,
      other: link.from_id === entityId ? link.to_id : link.from_id,
    })),
  );

  /** Newest first within each relation, relations in first-seen order. */
  const groups = $derived(
    [...new Set(rows.map((row) => row.relation))].map((relation) => ({
      relation,
      rows: rows.filter((row) => row.relation === relation),
    })),
  );
</script>

<div class="sec">
  <div class="sec-h">
    <span class="lab">Linked items</span>
    <span class="k muted">{links.length}</span>
  </div>

  {#if groups.length === 0}
    <div class="empty" title="Drawing links is M2 — knobas mirrors in M1, it does not yet write.">
      Nothing linked yet.
    </div>
  {:else}
    {#each groups as group (group.relation)}
      <div class="row hd g4">
        <span></span>
        <span>{group.relation}</span>
        <span></span>
        <span></span>
      </div>
      {#each group.rows as link (link.id)}
        <div class="row g4">
          <span class="mg">{link.origin.slice(0, 2).toUpperCase()}</span>
          <span class="t k">{link.other}</span>
          <span class="r">{link.origin}</span>
          <span class="r">{ago(link.created_at)}</span>
        </div>
      {/each}
    {/each}
  {/if}
</div>
