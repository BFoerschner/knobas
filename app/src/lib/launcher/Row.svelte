<!--
  One selectable line of the launcher (`.res`, `signal-miller.html:413`).

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
  import type { CredentialHealth } from "../ipc/sources";
  import Monogram from "../shell/Monogram.svelte";
  import { kindMonogram, kindSingular } from "../shell/kinds";
  import { provenance } from "./format";

  let {
    entityId,
    kind,
    sourceId,
    title,
    syncedAt,
    path = null,
    snippet = [],
    sources,
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
     * path, `Engineering › Payments` (#284).
     *
     * `null` for every row that has none, which is the ADR-0007 **miss**: a
     * row shows no path rather than a wrong one, and the caller does not have
     * to know what kind it is holding. Read with
     * {@link ancestorPath | `ancestorPath`} in `format.ts`, which is the one
     * place the shape is spelled.
     *
     * It is drawn under the title and not beside the key, because a path is
     * prose of unbounded length and the key column is 56 px.
     */
    path?: string | null;
    /** The matched excerpt, already split. Empty for a list or board row. */
    snippet?: Segment[];
    sources: CredentialHealth[];
    selected: boolean;
    now?: Date | undefined;
    onopen: () => void;
    onhover: () => void;
  } = $props();

  /**
   * The key half of the id — only the *first* colon separates namespace from
   * key, so `split(":")[1]` would truncate `confluence:ENG:SEPA design`.
   */
  const key = $derived(entityId.slice(entityId.indexOf(":") + 1));
  const prov = $derived(provenance(sourceId, syncedAt, sources, now));
</script>

<button
  class="res"
  class:on={selected}
  role="option"
  aria-selected={selected}
  onclick={onopen}
  onmouseenter={onhover}
>
  <Monogram text={kindMonogram(kind)} label={kindSingular(kind)} />
  <span class="ty">{kind}</span>
  <span class="t">
    <span class="k">{key}</span>{title}
    {#if path}
      <span class="pa">{path}</span>
    {/if}
    {#if snippet.length > 0}
      <span class="sn"
        >{#each snippet as seg, i (i)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span
      >
    {/if}
  </span>
  <span class="sy" class:fail={prov.failing}>{prov.text}</span>
</button>

<style>
  /*
    `.res` is the mockup's launcher row (round 3, lines 413-425). It is not in
    `app.css` on purpose — the sheet's header records the launcher's selectors
    as stream E's, shipping with the component.

    Four columns, not the mockup's six: its status and per-row action cells
    need write-back (M2), and a column reserved for a control that cannot work
    is how a UI fills up with dead chrome.
  */
  .res {
    display: grid;
    grid-template-columns: 28px 56px minmax(0, 1fr) 140px;
    gap: 8px;
    align-items: center;
    width: 100%;
    text-align: left;
    min-height: 32px;
    padding: 4px 12px 4px 8px;
    border-bottom: 1px solid var(--hair);
    font-size: 12px;
    color: var(--text);
  }
  .res:hover,
  .res.on {
    background: var(--raised);
  }
  .res.on {
    box-shadow: inset 2px 0 0 var(--amber);
  }
  .res > * {
    min-width: 0;
    overflow: hidden;
  }
  .ty {
    font: 400 10px var(--mono);
    color: var(--faint);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .t {
    overflow: hidden;
  }
  .t .k {
    color: var(--muted);
    margin-right: 6px;
    font-family: var(--mono);
    font-size: 11.5px;
  }
  /*
    The excerpt is a second line rather than a third column: it is prose, it
    is the reason the row matched, and truncating it to a column width throws
    away the half a reader is scanning for.
  */
  .sn {
    display: block;
    color: var(--muted);
    font-size: 11.5px;
    line-height: 1.35;
    margin-top: 1px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sn mark {
    background: transparent;
    color: var(--amber);
    font-weight: 500;
  }
  /*
    The path is where the item lives, not what it says, so it is quieter than
    the excerpt and sits above it — a reader scanning five pages called
    *Runbook* is looking for the space, and finds it on the same line every
    time.
  */
  .pa {
    display: block;
    color: var(--faint);
    font-size: 11px;
    line-height: 1.3;
    margin-top: 1px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sy {
    font: 400 10px var(--mono);
    color: var(--faint);
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Spec §4 pairs provenance with health: a stale row whose source is
     refusing the credential says so instead of quoting an innocent age. */
  .sy.fail {
    color: var(--fail);
  }
</style>
