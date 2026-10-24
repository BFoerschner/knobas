<!--
  One selectable line of the launcher: prototype-6's `.launch .res a` (kind,
  title, source), with the provenance as its source column (#1422; the rules
  are `tf/css/launcher.css`). Mono only on the key and the path.

  Results and the board's recent items both draw this, deliberately: spec §4
  promises the same per-row provenance everywhere, and two components drawing
  "a row" is how the two drift apart.

  **Everything here is text.** `title` and every `snippet` segment are raw
  source text — whatever somebody typed into a ticket, `<script>` included —
  and the highlight is the segment's `hit` flag, never markup inside the
  string (roadmap §4 gotcha 7). `ts_headline` happens to elide well-formed
  tags, which is a fidelity accident and *not* a sanitiser: `onclick=…` travels
  through untouched.
-->
<script lang="ts">
  import type { Segment } from "../ipc";
  import type { StateChip as Chip } from "../ipc/entity";
  import type { CredentialHealth } from "../ipc/sources";
  import StateChip from "../shell/StateChip.svelte";
  import { kindMonogram, kindSingular } from "../shell/kinds";
  import { provenance } from "./format";
  import { keyOf } from "../time/format";

  let {
    entityId,
    kind,
    sourceId,
    title,
    syncedAt,
    path = null,
    state = null,
    snippet = [],
    named = false,
    sources,
    id,
    selected,
    now,
    onopen,
    onhover,
  }: {
    entityId: string;
    kind: string;
    sourceId: string;
    title: string;
    syncedAt: string;
    /**
     * Where this item sits inside its source — a Confluence page's ancestor
     * path, `Engineering › Payments` (#284), or the parent chain any kind
     * declares, `tidewater/payout-service` for a Gitea commit (#596). Drawn
     * even when the title already repeats it.
     *
     * `null` for every row that has none, which is the ADR-0007 **miss**: the
     * row shows no path rather than a wrong one, and the caller does not have
     * to know what kind it is holding. Joined by one SQL expression
     * (`knobas_core::path_line_read!`) and carried on `EntityRow.path`, so
     * this row and the detail panel cannot disagree about what a path is.
     *
     * Drawn under the title and not beside the key, because a path is prose of
     * unbounded length and the key column is 56 px.
     */
    path?: string | null;
    /**
     * The item's state chip (#666) — a pull request's `merged`, a build's
     * `FAILURE` — drawn after the title, or `null` for a kind that declares
     * none. The same `StateChip` a room row draws.
     */
    state?: Chip | null;
    /** The matched excerpt, already split. Empty for a list or board row. */
    snippet?: Segment[];
    /** Whether another source's row holds the same key (#1476): the provenance then names this one's source. */
    named?: boolean;
    sources: CredentialHealth[];
    /** The option's element id, which the listbox names while it is active (#1411). */
    id: string;
    selected: boolean;
    now?: Date | undefined;
    onopen: () => void;
    onhover: () => void;
  } = $props();

  /** The key half of the id: `confluence:ENG:SEPA design` reads `ENG:SEPA design`. */
  const key = $derived(keyOf(entityId));
  const prov = $derived(provenance(sourceId, syncedAt, sources, now, named));
</script>

<button
  {id}
  class="tf-ln-res"
  class:tf-ln-on={selected}
  role="option"
  aria-selected={selected}
  onclick={onopen}
  onmouseenter={onhover}
>
  <span class="tf-ln-mg" title={kindSingular(kind)} aria-label={kindSingular(kind)}>{kindMonogram(kind)}</span>
  <span class="tf-ln-ty">{kind}</span>
  <span class="tf-ln-t">
    <span class="tf-ln-k">{key}</span>{title}
    {#if state}
      <StateChip {state} />
    {/if}
    {#if path}
      <span class="tf-ln-pa">{path}</span>
    {/if}
    {#if snippet.length > 0}
      <span class="tf-ln-sn"
        >{#each snippet as seg, i (i)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span
      >
    {/if}
  </span>
  <span class="tf-ln-sy" class:tf-ln-fail={prov.failing}>{prov.text}</span>
</button>
