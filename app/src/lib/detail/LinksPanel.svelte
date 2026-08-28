<!--
  What this item is linked to — spec §5a's core object, drawn from the viewed
  entity's side.

  **Presentational on purpose.** The panel does no IPC of its own: it is handed
  the hydrated entries `get_entity` returned and hands two intentions back
  (open, unlink). `Detail.svelte` owns the writes and the refresh, which
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
  import Monogram from "../shell/Monogram.svelte";
  import { kindMonogram, kindSingular } from "../shell/kinds";
  import { kindRegistry } from "../shell/kind-registry.svelte";
  import { hashFor } from "../shell/router.svelte";
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
    /** Navigate to an address (spec §2). The shell's router is the only one. */
    onopen: (hash: string) => void;
    onunlink: (entry: LinkEntry) => void;
  } = $props();

  const groups = $derived(groupLinks(links, entityId));

  /**
   * The other end's stable address (spec §2), so a row is navigation rather
   * than decoration.
   *
   * Built through `hashFor` and not by string concatenation: entity keys carry
   * `#` and `/` (`acme/payout-service#142`), and an unencoded one truncates the
   * fragment at the browser level.
   */
  function addressOf(entry: LinkEntry): string {
    return hashFor({
      view: "room",
      ctx: "all",
      detail: { kind: entry.other.kind, entityId: entry.other.entity_id },
    });
  }

  /** The adapter's word for a kind, falling back to §3a's humaniser. */
  function labelOf(kind: string): string {
    return kindSingular(kind, kindRegistry.info(kind));
  }

  function monogramOf(kind: string): string {
    return kindMonogram(kind, kindRegistry.info(kind));
  }
</script>

<div class="sec">
  <div class="sec-h">
    <span class="lab">Linked items</span>
    <span class="k muted">{links.length}</span>
  </div>

  {#if groups.length === 0}
    <!--
      The M1 caveat ("drawing links is M2 — knobas mirrors, it does not write")
      is gone: it described a limitation that no longer exists, and an empty
      state that explains why you cannot act is worse than one that offers the
      action.
    -->
    <div class="empty">
      <p>Nothing linked yet.</p>
      <p>Link this to the ticket it implements, the build it broke, or the page that documents it.</p>
    </div>
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
            class="btn sm"
            title="Remove this link. Linking again is one action."
            onclick={() => onunlink(entry)}
          >
            Unlink
          </button>
        </div>
        {#if entry.link.note}
          <p class="lnote">{entry.link.note}</p>
        {/if}
      {/each}
    {/each}
  {/if}
</div>

<style>
  /*
    Four columns rather than the stylesheet's `g4`: the last one holds a
    control, so it is sized by its content instead of by a fixed width that a
    longer word would clip.
  */
  .lrow {
    grid-template-columns: 34px 1fr auto auto;
  }

  /* The whole title cell is the navigation target, not a link-coloured word. */
  .lopen {
    text-align: left;
    width: 100%;
    color: var(--text);
  }

  .lopen:hover {
    color: var(--link);
  }

  .wd {
    margin-left: 6px;
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--amber);
  }

  /*
    The note is prose the user wrote about *why* the link exists, so it gets a
    line of its own under the row rather than a column that would clip it.
  */
  .lnote {
    padding: 4px 0 6px 42px;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
