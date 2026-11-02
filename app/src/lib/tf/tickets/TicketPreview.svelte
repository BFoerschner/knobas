<!--
  A ticket read (#810, R-3.9, spec #2227): key, title, the facts, the epic,
  the description, *Around this ticket* one line a section, and one primary
  button whose label follows the ticket's state (`previewAction`).

  The Tickets view's preview of the selected card. The ticket's own page is
  the job page in its reading mood (spec #2396, `job/TfJob.svelte`); this
  keeps its own drawing. *Around this ticket* lines are counts, not links.

  The button goes by the ticket's entity id: *Start work* and *Resume work*
  to the job's address, which makes it the chosen job (`job/chosen.svelte.ts`)
  and lands on its page; *Open job* only reads, so it goes to the ticket's
  page and chooses nothing (spec #2396).
-->
<script lang="ts">
  import { clockLabel } from "../../time/format";
  import { proseMarkdown, type Prose } from "../../render/prose";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { jobHash, ticketHash } from "../route";
  import { columnName, epicLine, previewAction, type AroundLine, type TicketFacts } from "./tickets";

  let {
    card,
    description,
    around,
    timerOnIt,
  }: {
    /** The ticket; its status is its own, whatever column it stands in (#1423). */
    card: TicketFacts;
    /** The description as its source wrote it, and how (#2308); `null` while it is read or where there is none. */
    description: Prose | null;
    /** *Around this ticket*, or `null` while it is read. */
    around: AroundLine[] | null;
    /** Whether the timer runs on this ticket. */
    timerOnIt: boolean;
  } = $props();

  const action = $derived(previewAction(card, timerOnIt));
  const epic = $derived(epicLine(card));
  const markdown = $derived(description === null ? null : proseMarkdown(description.text, description.markup));
</script>

<aside class="tf-preview" aria-label="Selected ticket">
  <div>
    <span class="tf-k">{card.key}</span>
    <h2 class="tf-ticket-title">{card.title}</h2>
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
    <a class="tf-btn tf-primary tf-lg" href={action.chooses ? jobHash(card.entity_id) : ticketHash(card.entity_id)}>{action.label}</a>
    <small>{action.note}</small>
  </div>
</aside>
