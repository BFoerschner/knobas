<!--
  What this item is linked to — spec §5a's core object, drawn from the viewed
  entity's side. The asset page's *Linked items* panel, in the design
  system's panel (`.tf-panel`) since the page left the tree's pane.

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
-->
<script lang="ts">
  import type { LinkEntry } from "../ipc/entity";
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
    onlink,
  }: {
    /** The entity whose detail this is — which end of each link is "here". */
    entityId: string;
    links: LinkEntry[];
    /** Navigate to an address (spec §2). The shell sets `location.hash`. */
    onopen: (hash: string) => void;
    onunlink: (entry: LinkEntry) => void;
    /** Open *Link to…*. The empty state's invitation is the same action. */
    onlink: () => void;
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

<section class="tf-panel tf-lnk" aria-label="Linked items">
  <header>
    <h3>Linked items</h3>
    {#if links.length > 0}<span class="tf-lnk-n">{links.length}</span>{/if}
    <span class="tf-trail">
      <button class="tf-btn tf-sm" onclick={onlink}>Link to…</button>
    </span>
  </header>
  {#if groups.length === 0}
    <!--
      An empty state that offers the action rather than explaining why there
      is nothing: the button on the heading is the action.
    -->
    <p class="tf-lnk-none">Nothing linked yet. Link this to the ticket it implements, the build it broke, or the page that documents it.</p>
  {:else}
    {#each groups as group (group.reading)}
      <h4 class="tf-lnk-h">{group.reading}</h4>
      <ul class="tf-lnk-rows">
        {#each group.entries as entry (entry.link.id)}
          <li class="tf-lnk-row">
            <span class="tf-dp-mono" title={labelOf(entry.other.kind)} aria-label={labelOf(entry.other.kind)}
              >{monogramOf(entry.other.kind)}</span
            >
            <!-- The title is raw source text and is rendered as text (gotcha 7). -->
            <button class="tf-lnk-open" title="Open {entry.other.entity_id}" onclick={() => onopen(addressOf(entry))}>
              {entry.other.title || entry.other.entity_id}
              {#if entry.other.deleted_at}
                <span class="tf-chip tf-warn">withdrawn</span>
              {/if}
            </button>
            <span class="tf-lnk-when" title="{labelOf(entry.other.kind)} · {entry.link.origin}">
              {labelOf(entry.other.kind)}, {ago(entry.link.created_at)}
            </span>
            <button
              class="tf-btn tf-sm tf-ghost"
              title="Remove this link. Linking again is one action."
              onclick={() => onunlink(entry)}
            >
              Unlink
            </button>
            {#if entry.link.note}
              <p class="tf-lnk-note">{entry.link.note}</p>
            {/if}
            <!--
              An accepted suggestion keeps the reason knobas proposed it with:
              the row behaves like every other one and can still say why it
              exists. A hand-drawn link has no reason and draws no line.
            -->
            {#if entry.link.reason}
              <p class="tf-lnk-note tf-lnk-why">{entry.link.reason}</p>
            {/if}
          </li>
        {/each}
      </ul>
    {/each}
  {/if}
</section>
