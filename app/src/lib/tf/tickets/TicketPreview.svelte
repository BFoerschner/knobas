<!--
  A ticket read (#810, R-3.9, spec #2227): key, title, the facts, the epic,
  the description, *Around this ticket* one row a section in the shape of the
  card's rows (`.tf-tktable`), and one primary button whose label follows the
  ticket's state (`previewAction`).

  The Tickets view's preview of the selected card. The ticket's own page is
  the job page in its reading mood (spec #2396, `job/TfJob.svelte`); this
  keeps its own drawing. Each row of *Around this ticket* opens its section
  narrowed to the ticket, as the same row on the ticket's card does.

  The button goes by the ticket's entity id: *Start work* and *Resume work*
  to the job's address, which makes it the chosen job (`job/chosen.svelte.ts`)
  and lands on its page; *Open job* only reads, so it goes to the ticket's
  page and chooses nothing (spec #2396).
-->
<script lang="ts">
  import { clockLabel } from "../../time/format";
  import { proseMarkdown, withoutAttachmentMarks, type Prose } from "../../render/prose";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { jobHash, ticketHash } from "../route";
  import { scopedTicket, type ScopedSection } from "../job/scoped.svelte";
  import TfKey from "../TfKey.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import { columnName, epicLine, previewAction, sentenceCase, type AroundLine, type TicketFacts } from "./tickets";

  const SCOPED_SECTION: Record<string, ScopedSection | undefined> = { Code: "code", Builds: "builds", Docs: "docs", Monitors: "monitors" };

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
    around: AroundLine[] | null;
    /** Whether the timer runs on this ticket. */
    timerOnIt: boolean;
    /** The ticket is the chosen job. */
    chosen?: boolean;
  } = $props();

  const action = $derived(previewAction(card, timerOnIt, chosen));
  const epic = $derived(epicLine(card));
  // The preview has no attachment list to resolve a wiki's `!name!` against, so it says the file's name instead of the mark.
  const markdown = $derived(description === null ? null : proseMarkdown(withoutAttachmentMarks(description.text, description.markup), description.markup));

  /** Where a row lands: its section narrowed to the ticket, or for Assets the first asset with the rest marked. */
  function hrefOf(line: AroundLine): string {
    const section = SCOPED_SECTION[line.section];
    if (section !== undefined) return `#/${section}`;
    return line.first !== null ? `#/asset/${encodeURIComponent(line.first)}` : "#/assets";
  }
  function press(line: AroundLine) {
    const section = SCOPED_SECTION[line.section];
    if (section !== undefined) scopedTicket.open(section, card.entity_id);
    else scopedTicket.openAssets(card.entity_id);
  }
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
    <dd>{card.assignee ?? "Unassigned"}</dd>
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
    {#if around !== null && around.length > 0}
      <div class="tf-tktable tf-tksecs">
        {#each around as line (line.section)}
          <a href={hrefOf(line)} title={line.failing === null ? line.count : `${line.count}, ${line.failing}`} onclick={() => press(line)}
            ><span class="tf-tkname">{line.section}</span><span class="tf-n">{line.n}</span>{#if line.failing !== null}<span class="tf-st tf-asks"
                ><span class="tf-dot tf-err" aria-hidden="true"></span>{line.failing}</span
              >{:else if line.note !== null}<span class="tf-st">{line.note}</span>{:else}<span></span>{/if}<span class="tf-chev" aria-hidden="true">›</span></a
          >
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
