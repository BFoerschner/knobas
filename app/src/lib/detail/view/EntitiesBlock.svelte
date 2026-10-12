<!--
  An entity-list block (ADR-0020): a commit's parents, a pull request's
  commits, a build's changes. A row is an **address** only where it carries an
  `entity_id`, which the app leaves set only for an entity the mirror holds
  (rule 7); every other row is plain text.
-->
<script lang="ts">
  import type { ViewRow } from "../../ipc/entity";
  import { hashFor } from "../../shell/router.svelte";
  import StateChip from "../../shell/StateChip.svelte";
  import { toneClass } from "./tone";

  let {
    title,
    rows,
    onnavigate,
    onopenurl,
  }: {
    title: string | null;
    rows: ViewRow[];
    onnavigate: (hash: string) => void;
    onopenurl: (url: string) => void;
  } = $props();

  /** The row's detail address: the `#/entity/<id>` alias, since a row names no kind. */
  function addressOf(entityId: string): string {
    return hashFor({ view: "room", ctx: "all", detail: { kind: null, entityId } });
  }
</script>

<div class="eb">
  {#if title}
    <div class="sec-h"><span class="lab">{title}</span><span class="k muted">{rows.length}</span></div>
  {/if}
  {#if rows.length === 0}
    <p class="eb-none">None.</p>
  {/if}
  {#each rows as row, index (index)}
    <div class="eb-row">
      {#if row.entity_id}
        {@const id = row.entity_id}
        <button class="eb-t" title="Open {id}" onclick={() => onnavigate(addressOf(id))}>
          {row.title}
        </button>
      {:else}
        <span class="eb-t plain">{row.title}</span>
      {/if}
      {#if row.chip}
        <!-- The rooms' own chip (#666), so a row's state reads the same here. -->
        <StateChip state={{ text: row.chip, tone: row.tone ?? "muted" }} />
      {/if}
      {#if row.meta}
        <span class="eb-meta {row.chip ? '' : toneClass(row.tone)}">{row.meta}</span>
      {/if}
      {#if row.url && !row.entity_id}
        {@const url = row.url}
        <button class="btn ghost sm" onclick={() => onopenurl(url)}>Open</button>
      {/if}
    </div>
  {/each}
</div>

<style>
  .eb-none {
    color: var(--muted);
    font-size: 12px;
  }

  .eb-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 26px;
    border-bottom: 1px solid var(--hair);
    font-size: 12px;
  }

  .eb-row:last-child {
    border-bottom: 0;
  }

  .eb-t {
    flex: 1;
    min-width: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--link);
  }

  .eb-t:hover {
    text-decoration: underline;
  }

  .eb-t.plain {
    color: var(--text);
  }

  .eb-t.plain:hover {
    text-decoration: none;
  }

  .eb-meta {
    flex: none;
    max-width: 45%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: 400 11px var(--mono);
  }

  /*
    The untoned colour at zero specificity, so a tone's class from `app.css`
    (`.ok`, `.fail`, ...) wins over it.
  */
  :where(.eb-meta) {
    color: var(--muted);
  }
</style>
