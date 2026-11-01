<!--
  The ticket page's card (#816, R-4.2, spec #2396): the same card in two
  moods. On the yellow (`tf-card`, *Job card*) once the ticket is the chosen
  job; on a sheet (`tf-sheetcard`, *Ticket*) while it is only read. The mood
  class is the only difference: the same markup, no punched hole, no status
  select, no clock.

  Top to bottom: the key, the source when another source holds the same key
  (#1476), with a link to each other one, the status as the card's chip, the
  facts line and the blocked chip, and at the foot one large button
  (`footAction` in `job.ts`): the decision while reading, the timer once
  pinned. The running time is the top bar's chip alone. The title, the
  description and the conversation are the reading column's (#2399); *Time
  this week* is Time's.
-->
<script lang="ts">
  import type { EntityDetail } from "../../ipc/entity";
  import { ticketHash } from "../route";
  import { blockers, factsLine, keyOf } from "./job";

  let {
    detail,
    others = [],
    pinned,
    action,
    onaction,
  }: {
    detail: EntityDetail;
    /** The other sources' tickets under the same key (#1476); none draws no source. */
    others?: EntityDetail[];
    /** Whether the ticket is the chosen job: the yellow, else the sheet. */
    pinned: boolean;
    /** The foot's one button, or `null` where the card offers none. */
    action: { label: string } | null;
    onaction: () => void;
  } = $props();

  const key = $derived(keyOf(detail.row.entity_id));
  const facts = $derived(factsLine(detail));
  const blockedBy = $derived(blockers(detail));
</script>

<article
  class="tf-jobcard tf-ticketcard"
  class:tf-card={pinned}
  class:tf-sheetcard={!pinned}
  aria-label={pinned ? "Job card" : "Ticket"}
>
  <div>
    <div class="tf-jobkey">{key}</div>
    {#if others.length > 0}
      <p class="tf-jobsrc">
        In {detail.source.display_name} · also in {#each others as other, at (other.row.entity_id)}{#if at > 0},
          {/if}<a href={ticketHash(other.row.entity_id)}>{other.source.display_name}</a>{/each}
      </p>
    {/if}
  </div>
  <div class="tf-jobrow">
    <span class="tf-chip">{detail.status ?? "No status"}</span>
    {#if facts}<span class="tf-jobdim">{facts}</span>{/if}
    {#each blockedBy as blocker (blocker)}
      <span class="tf-chip tf-err">Blocked by {blocker}</span>
    {/each}
  </div>
  {#if action !== null}
    <div class="tf-go">
      <button type="button" class="tf-btn tf-primary tf-lg" onclick={onaction}>{action.label}</button>
    </div>
  {/if}
</article>
