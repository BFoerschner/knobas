<!--
  One synced item as a room row (`.row.g5w`, `signal-miller.html:2394`).

  Five columns: the kind's monogram, the item's key, its title, the source it
  came from, and when the source last changed it. The last two are spec §4's
  per-row provenance — *where it came from and when it was last seen* — which
  is the one thing a mirror owes its reader on every single line.

  Everything here is **text**. `title` is whatever somebody typed into a
  ticket (gotcha 7).
-->
<script lang="ts">
  import type { EntityRow } from "../ipc/entity";
  import Monogram from "./Monogram.svelte";
  import { kindMonogram, kindSingular } from "./kinds";
  import { ago } from "./time";

  let {
    row,
    onopen,
    now,
  }: {
    row: EntityRow;
    onopen: (row: EntityRow) => void;
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

  const updated = $derived(row.updated_at ? ago(row.updated_at, now) : "—");
  const provenance = $derived(
    `${kindSingular(row.kind)} · ${row.updated_at ? `updated ${updated}` : "never dated by its source"} · synced ${ago(row.synced_at, now)}`,
  );
</script>

<button class="row g5w" title={provenance} onclick={() => onopen(row)}>
  <Monogram text={kindMonogram(row.kind)} label={kindSingular(row.kind)} />
  <span class="k link">{key}</span>
  <span class="t">{row.title}</span>
  <span class="r">{row.source_id}</span>
  <span class="r">{updated}</span>
</button>
