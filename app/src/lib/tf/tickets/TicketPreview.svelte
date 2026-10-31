<!--
  A ticket read (#810, R-3.9, spec #2227): key, title, the facts, the epic,
  the description, *Around this ticket* one line a section, and one primary
  button whose label follows the ticket's state (`previewAction`).

  Drawn twice from this one component, so neither copies the other: the
  Tickets view's preview of the selected card, and the ticket page
  (`#/ticket/<id>`, `page`), whose card is the ticket-yellow, as the job card
  is. *Around this ticket* lines are counts, not links: opening a section's
  rows means opening the job, and that is the button's.

  The button goes to the job's address by the ticket's entity id, which opens
  the job and makes it the chosen job (`job/chosen.svelte.ts`); what *Start
  work* does there is #818's.
-->
<script lang="ts">
  import { clockLabel } from "../../time/format";
  import { proseMarkdown, type Prose } from "../../render/prose";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { jobHash } from "../route";
  import { columnName, epicLine, previewAction, type AroundLine, type TicketFacts } from "./tickets";

  let {
    card,
    description,
    around,
    timerOnIt,
    page = false,
  }: {
    /** The ticket; its status is its own, whatever column it stands in (#1423). */
    card: TicketFacts;
    /** The description as its source wrote it, and how (#2308); `null` while it is read or where there is none. */
    description: Prose | null;
    /** *Around this ticket*, or `null` while it is read. */
    around: AroundLine[] | null;
    /** Whether the timer runs on this ticket. */
    timerOnIt: boolean;
    /** The ticket page's yellow card, rather than the Tickets view's preview pane. */
    page?: boolean;
  } = $props();

  const action = $derived(previewAction(card, timerOnIt));
  const epic = $derived(epicLine(card));
  const markdown = $derived(description === null ? null : proseMarkdown(description.text, description.markup));
</script>

<svelte:element
  this={page ? "article" : "aside"}
  class={page ? "tf-preview tf-ticketcard tf-card" : "tf-preview"}
  aria-label={page ? "Ticket" : "Selected ticket"}
>
  <div>
    <span class="tf-k">{card.key}</span>
    <svelte:element this={page ? "h1" : "h2"} class="tf-ticket-title">{card.title}</svelte:element>
  </div>
  <dl class="tf-facts">
    <dt>Status</dt>
    <dd>{columnName(card.status)}</dd>
    <dt>Assignee</dt>
    <dd>{card.assignee ?? "Unassigned"}</dd>
    <dt>Priority</dt>
    <dd>{card.priority ?? "None"}</dd>
    {#if epic}<dt>Epic</dt>
      <dd>{epic}</dd>{/if}
    <dt>Updated</dt>
    <dd>{clockLabel(card.updated_at)}</dd>
  </dl>
  {#if markdown !== null}<div class="tf-desc"><TfMarkdown text={markdown} /></div>
  {:else if description !== null && description.text.trim() !== ""}<p class="tf-desc">{description.text}</p>{/if}
  <div>
    <div class="tf-pane-h">Around this ticket</div>
    {#if around !== null && around.length > 0}
      <div class="tf-rollup">
        {#each around as line (line.section)}
          <div>
            <b>{line.section}</b><span
              >{line.count}{#if line.failing}, <span class="tf-bad">{line.failing}</span>{/if}</span
            >
          </div>
        {/each}
      </div>
    {:else if around !== null}
      <p class="tf-dim tf-small">Nothing connected yet.</p>
    {/if}
  </div>
  <div class="tf-go">
    <a class="tf-btn tf-primary tf-lg" href={jobHash(card.entity_id)}>{action.label}</a>
    <small>{action.note}</small>
  </div>
</svelte:element>
