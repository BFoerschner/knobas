<!--
  One ticket on the ticket board (#810, R-3.7): key, type and priority, the
  summary, the epic under it, its signals (#815) and the assignee's
  initials. The chosen job's card is the yellow card, selected or not; the
  selected card carries a two-pixel ink outline and is lifted off the board
  (spec #2396, #2398). A double click opens the ticket's page (#813). The
  epic is its key and name and the comment count is drawn above zero only
  (#894).

  A key another source holds too (#1476) draws its source's name beside it,
  so two sources' PAY-231 are told apart.

  A ticket view says what its cards show (#812, R-3.16, *Cards show*):
  `fields` names them; without it the card shows what it always has.
-->
<script lang="ts">
  import type { MiniBoardCard } from "../../ipc/entity";
  import type { CardField } from "../../ipc/search";
  import Marked from "../filter/Marked.svelte";
  import Signals from "./Signals.svelte";
  import { commentsLine, epicLine, initials } from "./tickets";

  let {
    card,
    needles,
    selected,
    chosen = false,
    onselect,
    onopen,
    fields = null,
    source = null,
  }: {
    card: MiniBoardCard;
    needles: readonly string[];
    selected: boolean;
    /** The card is the chosen job's (`job/chosen.svelte.ts`): the yellow card. */
    chosen?: boolean;
    onselect: () => void;
    /** A double click: open the ticket's page. */
    onopen: () => void;
    /** What the card shows, from its view; `null` is everything it has. */
    fields?: CardField[] | null;
    /** The source's name, when another source holds the same key (#1476); `null` draws none. */
    source?: string | null;
  } = $props();

  const shows = (field: CardField) => fields === null || fields.includes(field);
  /** Labels are shown only when a view asks for them, as in prototype-6. */
  const labels = $derived(fields?.includes("labels") ? card.labels : []);

  const kind = $derived([card.issue_type, card.priority].filter(Boolean).join(", "));
  const epic = $derived(epicLine(card));
  const comments = $derived(commentsLine(card.comment_count));
</script>

<button
  type="button"
  class="tf-tcard"
  class:tf-card={chosen}
  class:tf-sel={selected}
  aria-current={selected ? "true" : undefined}
  data-entity={card.entity_id}
  onclick={onselect}
  ondblclick={onopen}
>
  <span class="tf-k"><Marked text={card.key} {needles} /></span>
  {#if source}<span class="tf-dim tf-tsrc">{source}</span>{/if}
  {#if kind && shows("type")}<span class="tf-dim tf-tkind">{kind}</span>{/if}
  <span class="tf-s"><Marked text={card.title} {needles} /></span>
  {#if epic && shows("epic")}<span class="tf-cep">{epic}</span>{/if}
  <span class="tf-meta">
    {#if shows("signals")}<Signals signals={card.signals} />{/if}
    {#each labels as label (label)}<span class="tf-lab">{label}</span>{/each}
    {#if comments && shows("comments")}<span class="tf-signal tf-dim tf-comments">{comments}</span>{/if}
    {#if shows("assignee")}
      <span class="tf-av" class:tf-me={card.mine} title={card.assignee ?? "Unassigned"}>{initials(card.assignee)}</span>
    {/if}
  </span>
</button>
