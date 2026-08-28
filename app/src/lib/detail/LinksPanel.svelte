<!--
  What this item is linked to — spec §5a's core object.

  **No longer always empty.** It was, through M1 — `knobas.link` existed and
  nothing wrote it. Links v1's tracer bullet (#52) gave the app
  `createLink`/`unlink`, so `EntityDetail.links` now carries whatever the user
  has drawn (interfaces §2.5).

  This panel is where every later milestone hangs — the suggestion tray,
  *Link to…*, the unlink tombstone — which is why it shipped against the real
  `LinkRow` shape in M1. What is still M1-shaped and **not** this file's to fix
  yet: the empty state's caveat text, the inverse labels, the note, click-through
  and the unlink control are all #53's, which rewrites the panel.
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
