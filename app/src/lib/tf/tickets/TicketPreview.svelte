<!--
  A ticket read (#810, R-3.9, spec #2227): key, title, the facts, the epic,
  the description, *Around this ticket*, which is the card's own rows in the card's
  words (F-109, `job/TfCardRows.svelte`), and one primary button whose label follows
  the ticket's state (`previewAction`).

  The Tickets view's preview of the selected card. The ticket's own page is
  the job page in its reading mood (spec #2396, `job/TfJob.svelte`); this
  keeps its own drawing. Each row of *Around this ticket* opens its section
  narrowed to the ticket, as the same row on the ticket's card does.

  The button goes by the ticket's entity id: *Resume work* to the job's address,
  which makes it the chosen job (`job/chosen.svelte.ts`), lands on its page and
  starts the timer, as the same words at the card's foot do; *Open ticket* only
  reads, so it goes to the ticket's page and chooses nothing (spec #2396); and
  *Start work* to the ticket's page, opened on its steps (`pageAsk`), choosing
  nothing until they run (F-130).
-->
<script lang="ts">
  import { clockLabel } from "../../time/format";
  import { proseMarkdown, withoutAttachmentMarks, type Prose } from "../../render/prose";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { jobHash, ticketHash } from "../route";
  import { pageAsk } from "../job/chosen.svelte";
  import type { CardTable } from "../job/card-sections";
  import TfCardRows from "../job/TfCardRows.svelte";
  import TfKey from "../TfKey.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import { assigneeName, columnName, epicLine, previewAction, sentenceCase, type TicketFacts } from "./tickets";

  let {
    card,
    description,
    around,
    timerOnIt,
    chosen = false,
  }: {
    /** The ticket; its status is its own, whatever column it stands in (#1423). */
    card: TicketFacts;
    /** The description as its source wrote it, and how (#2308); `null` while it is read or where there is none. */
    description: Prose | null;
    /** *Around this ticket*, or `null` while it is read. */
    around: CardTable | null;
    /** Whether the timer runs on this ticket. */
    timerOnIt: boolean;
    /** The ticket is the chosen job. */
    chosen?: boolean;
  } = $props();

  const action = $derived(previewAction(card, timerOnIt, chosen));
  const epic = $derived(epicLine(card));
  // The preview has no attachment list to resolve a wiki's `!name!` against, so it says the file's name instead of the mark.
  const markdown = $derived(description === null ? null : proseMarkdown(withoutAttachmentMarks(description.text, description.markup), description.markup));

</script>

<aside class="tf-preview" aria-label="Selected ticket">
  <div>
    <TfKey value={card.key} lead class="tf-k" />
    <h2 class="tf-ticket-title">{card.title}</h2>
  </div>
  <dl class="tf-facts">
    <dt>Status</dt>
    <dd>{columnName(card.status)}</dd>
    <dt>Assignee</dt>
    <dd>{assigneeName(card) ?? "Unassigned"}{card.mine && card.assignee !== null ? " (you)" : ""}</dd>
    <dt>Priority</dt>
    <dd>{sentenceCase(card.priority) ?? "None"}</dd>
    {#if epic}<dt>Epic</dt>
      <dd><TfKeyed text={epic} /></dd>{/if}
    <dt>Updated</dt>
    <dd>{clockLabel(card.updated_at)}</dd>
  </dl>
  {#if markdown !== null}<div class="tf-desc"><TfMarkdown text={markdown} /></div>
  {:else if description !== null && description.text.trim() !== ""}<p class="tf-desc">{description.text}</p>{/if}
  <div>
    <div class="tf-pane-h">Around this ticket</div>
    {#if around !== null && around.rows.length > 0}
      <TfCardRows rows={around.rows} entityId={card.entity_id} ticketKey={card.key} />
    {:else if around !== null}
      <p class="tf-dim tf-small">Nothing connected yet.</p>
    {/if}
  </div>
  <div class="tf-go">
    <a class="tf-btn tf-primary tf-lg" onclick={() => action.asks !== undefined && pageAsk.ask(card.entity_id, action.asks)} href={action.chooses ? jobHash(card.entity_id) : ticketHash(card.entity_id)}>{action.label}</a>
    <small>{action.note}</small>
  </div>
</aside>
