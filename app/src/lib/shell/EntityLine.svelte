<!--
  One synced item as a room row (`.row.g5w`, `signal-miller.html:2394`).

  Five columns: the kind's monogram, the item's key, its title, the source it
  came from, and when the source last changed it. The last two are spec §4's
  per-row provenance — *where it came from and when it was last seen* — which
  is the one thing a mirror owes its reader on every single line.

  Four in a room scoped to one source (#666): a source room or a project room
  would print the same source id on every line, so the column goes and the
  provenance tooltip still names it. The title cell carries the item's state
  chip after the title, where the adapter declares one.

  Everything here is **text**. `title` is whatever somebody typed into a
  ticket (gotcha 7).
-->
<script lang="ts">
  import type { EntityRow } from "../ipc/entity";
  import { prefetch } from "../detail/prefetch";
  import Monogram from "./Monogram.svelte";
  import StateChip from "./StateChip.svelte";
  import { kindRegistry } from "./kind-registry.svelte";
  import { kindMonogram, kindSingular } from "./kinds";
  import { ago } from "./time";

  let {
    row,
    onopen,
    now,
    showSource = true,
  }: {
    row: EntityRow;
    onopen: (row: EntityRow) => void;
    /**
     * Whether to draw the source column. `false` in a room scoped to one
     * source, where every line would repeat it (#666).
     */
    showSource?: boolean;
    /** Injectable clock, so the age column is testable. */
    now?: Date;
  } = $props();

  /**
   * The key half of the id.
   *
   * Only the first `:` separates namespace from key (`knobas_core::entity`),
   * so `split(":")[1]` would truncate `confluence:ENG:SEPA design` at its
   * second colon.
   */
  const key = $derived(row.entity_id.slice(row.entity_id.indexOf(":") + 1));

  /**
   * What the adapter that produced this row calls its kind (§3a).
   *
   * By kind and not by adapter: an `EntityRow` carries `source_id` — the
   * *instance* id — and the mapping from instance to adapter lives on
   * `SourceSummary`, which a room row does not have. The registry's first-wins
   * answer is the honest one available here, and the detail header, which does
   * know the adapter, resolves it properly.
   */
  const declared = $derived(kindRegistry.info(row.kind));

  const updated = $derived(row.updated_at ? ago(row.updated_at, now) : "—");
  const provenance = $derived(
    `${kindSingular(row.kind, declared)} from ${row.source_id} · ${row.updated_at ? `updated ${updated}` : "never dated by its source"} · synced ${ago(row.synced_at, now)}`,
  );
</script>

<button
  class="row"
  class:g5w={showSource}
  class:g4w={!showSource}
  title={provenance}
  use:prefetch={{ entityId: row.entity_id, kind: row.kind }}
  onclick={() => onopen(row)}
>
  <Monogram text={kindMonogram(row.kind, declared)} label={kindSingular(row.kind, declared)} />
  <span class="k link">{key}</span>
  <span class="t"
    ><span class="tt">{row.title}</span>{#if row.state}<StateChip state={row.state} />{/if}</span
  >
  {#if showSource}
    <span class="r src">{row.source_id}</span>
  {/if}
  <span class="r">{updated}</span>
</button>

<style>
  /* The title gives way to the chip: a long title ellipsises, and the state
     after it stays whole, because the state is the reading a reader is
     scanning the room for. */
  .t {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .tt {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
