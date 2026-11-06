<!--
  What this item is linked to — spec §5a's core object, drawn from the viewed
  entity's side.

  **Presentational on purpose.** The panel does no IPC of its own: it is handed
  the hydrated entries `get_entity` returned and hands three intentions back
  (open, unlink, link). `Detail.svelte` owns the writes and the refresh, which
  is what lets this file be tested by mounting it with an array — the sibling
  `HistoryPanel.svelte` draws the same line.

  Three things here are not decoration:

  * **The wording is computed per row** (`relations.ts`). A link is directed
    and one word describes it, but its two ends read different sentences, and a
    header that said `blocks` over a row somebody drew *at* this item would
    state the opposite of the truth.
  * **A withdrawn target stays.** §5a keeps the entity so a link never dangles;
    the row is marked rather than dropped.
  * **Unlink asks nothing.** Re-linking a pair is one action (the unique index
    is partial), so removal is cheap to reverse, and a confirmation dialog for
    a reversible action is a dialog that trains people to dismiss dialogs.
  * **It is one more group of the pane it stands in**: the pane's label and badge
    and no way of its own to link, since the pane's actions already hold *Link to…*.
-->
<script lang="ts">
  import type { LinkEntry } from "../ipc/entity";
  import Monogram from "../shell/Monogram.svelte";
  import { kindMonogram, kindSingular } from "../shell/kinds";
  import { kindRegistry } from "../shell/kind-registry.svelte";
  import { entityAddress } from "../tf/address";
  import { ago } from "../shell/time";
  import { groupLinks } from "./relations";

  let {
    entityId,
    links,
    onopen,
    onunlink,
  }: {
    /** The entity whose detail this is — which end of each link is "here". */
    entityId: string;
    links: LinkEntry[];
    /** Navigate to an address (spec §2). The shell sets `location.hash`. */
    onopen: (hash: string) => void;
    onunlink: (entry: LinkEntry) => void;
  } = $props();

  const groups = $derived(groupLinks(links, entityId));

  /**
   * The other end's stable address (spec §2), so a row is navigation rather
   * than decoration.
   *
   * Built through `entityAddress` (the ticket-first shell's one rule for a
   * kind's page) and not by string concatenation: entity keys carry `#` and `/`
   * (`acme/payout-service#142`), and an unencoded one truncates the fragment
   * at the browser level.
   */
  function addressOf(entry: LinkEntry): string {
    return entityAddress(entry.other.kind, entry.other.entity_id);
  }

  /** The adapter's word for a kind, falling back to §3a's humaniser. */
  function labelOf(kind: string): string {
    return kindSingular(kind, kindRegistry.info(kind));
  }

  function monogramOf(kind: string): string {
    return kindMonogram(kind, kindRegistry.info(kind));
  }
</script>

<div class="sec links">
  <h3 class="lab">Linked items <span class="tf-count">{links.length}</span></h3>

  {#if groups.length === 0}
    <p class="none">Nothing linked yet.</p>
  {:else}
    {#each groups as group (group.reading)}
      <div class="row hd lrow">
        <span></span>
        <span>{group.reading}</span>
        <span></span>
        <span></span>
      </div>
      {#each group.entries as entry (entry.link.id)}
        <div class="row lrow">
          <Monogram text={monogramOf(entry.other.kind)} label={labelOf(entry.other.kind)} />
          <!--
            The title is raw source text and is rendered as text (gotcha 7).
          -->
          <button
            class="t lopen"
            title="Open {entry.other.entity_id}"
            onclick={() => onopen(addressOf(entry))}
          >
            {entry.other.title || entry.other.entity_id}
            {#if entry.other.deleted_at}
              <span class="wd">withdrawn</span>
            {/if}
          </button>
          <span class="r" title="{labelOf(entry.other.kind)} · {entry.link.origin}">
            {labelOf(entry.other.kind)} · {ago(entry.link.created_at)}
          </span>
          <button
            class="tf-btn tf-sm"
            title={entry.link.origin === "manual" ? "Remove this link. Linking again is one action." : "Remove this link. Asks first, because linking again would mark it as drawn by hand."}
            onclick={() => onunlink(entry)}
          >
            {entry.link.origin === "manual" ? "Unlink" : "Unlink…"}
          </button>
        </div>
        {#if entry.link.note}
          <p class="lnote">{entry.link.note}</p>
        {/if}
        <!--
          An accepted suggestion keeps the reason knobas proposed it with, and
          the panel shows it: "there are not two kinds of link to reason about"
          (#41 story 5) cuts both ways -- the row behaves like every other one,
          and it can still say why it exists. A hand-drawn link has no reason
          and draws no line.
        -->
        {#if entry.link.reason}
          <p class="lnote lwhy">{entry.link.reason}</p>
        {/if}
      {/each}
    {/each}
  {/if}
</div>

<style>
  /* A group of the pane it stands in: no box and no indent of its own, its label and its empty line the pane's. */
  .links {
    padding: 0;
    border: 0;
  }

  .links h3 {
    margin: 0 0 6px;
  }

  .links .tf-count {
    margin-left: 4px;
  }

  .none {
    margin: 0;
    font-size: var(--t-sm);
    color: var(--ink-2);
  }

  /* A row is its tag, its title and *Unlink*, with its kind and age under the title: a 310 px pane has no room for four columns. */
  .links .row {
    margin: 0;
    padding: 0;
    border: 0;
  }

  .links .row:hover {
    background: none;
  }

  .links .row.hd {
    display: block;
    height: auto;
    margin-top: 6px;
    background: none;
    color: var(--ink-2);
    font: var(--t-xs) var(--sans);
  }

  .links .row.hd span:empty {
    display: none;
  }

  .links .row.lrow:not(.hd) {
    grid-template-columns: 26px minmax(0, 1fr) auto;
    height: auto;
    gap: 0 6px;
    align-items: baseline;
    padding: 4px 0;
  }

  .links .row.lrow:not(.hd) > :global(.tf-btn) {
    grid-column: 3;
    grid-row: 1 / span 2;
    align-self: center;
    overflow: visible;
  }

  /* The title is the way to it, underlined as a link made of words is (design system, Layout). */
  .lopen {
    text-align: left;
    width: 100%;
    color: var(--ink);
    font-size: var(--t-sm);
    text-decoration: underline;
    text-underline-offset: 2px;
    white-space: normal;
    overflow-wrap: anywhere;
  }

  .links .row .r {
    grid-column: 2;
    grid-row: 2;
    text-align: left;
    color: var(--ink-2);
    font: var(--t-xs) var(--sans);
  }

  .wd {
    display: inline-block;
    margin-left: 6px;
    font-size: var(--t-2xs);
    font-weight: 600;
    color: var(--amber);
  }

  /*
    The note is prose the user wrote about *why* the link exists, so it gets a
    line of its own under the row rather than a column that would clip it.
  */
  /* knobas' words, not the user's: fainter, so the two are not one voice. */
  .lwhy {
    color: var(--faint);
  }

  .lnote {
    padding: 0 0 6px 32px;
    color: var(--muted);
    font-size: var(--t-xs);
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
