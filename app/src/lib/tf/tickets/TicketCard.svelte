<!--
  One ticket on the ticket board (#810, R-3.7): key, type and priority, the
  summary, the epic under it, its signals (#815) and the assignee's
  initials. The selected card is the yellow card, lifted off the board. A
  double click opens the job (#813). The comment count waits for a declared
  count (see the #810 rulings).
-->
<script lang="ts">
  import type { MiniBoardCard } from "../../ipc/entity";
  import Marked from "../filter/Marked.svelte";
  import Signals from "./Signals.svelte";
  import { initials } from "./tickets";

  let {
    card,
    typed,
    selected,
    onselect,
    onopen,
  }: {
    card: MiniBoardCard;
    typed: string;
    selected: boolean;
    onselect: () => void;
    /** A double click: open the job. */
    onopen: () => void;
  } = $props();

  const kind = $derived([card.issue_type, card.priority].filter(Boolean).join(", "));
</script>

<button
  type="button"
  class="tf-tcard"
  class:tf-card={selected}
  aria-current={selected ? "true" : undefined}
  data-entity={card.entity_id}
  onclick={onselect}
  ondblclick={onopen}
>
  <span class="tf-k"><Marked text={card.key} {typed} /></span>
  {#if kind}<span class="tf-dim tf-tkind">{kind}</span>{/if}
  <span class="tf-s"><Marked text={card.title} {typed} /></span>
  {#if card.path}<span class="tf-cep">{card.path}</span>{/if}
  <span class="tf-meta">
    <Signals signals={card.signals} />
    <span class="tf-av" class:tf-me={card.mine} title={card.assignee ?? "Unassigned"}>{initials(card.assignee)}</span>
  </span>
</button>
