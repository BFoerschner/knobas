<!--
  The ticket page's card (#816, R-4.2, spec #2396): the same card in two
  moods. On the yellow (`tf-card`, *Job card*) once the ticket is the chosen
  job; on a sheet (`tf-sheetcard`, *Ticket*) while it is only read. The mood
  class is the only difference: the same markup, no punched hole, no status
  select, no clock.

  Top to bottom: the key, the source when another source holds the same key
  (#1476), with a link to each other one, the summary, the status as the
  card's chip, the facts line and the blocked chip, *What it asks for*, the
  conversation's summary (#819), *Time this week*, and at the foot one large
  button (`footAction` in `job.ts`): the decision while reading, the timer
  once pinned. The running time is the top bar's chip alone.
-->
<script lang="ts">
  import type { EntityDetail } from "../../ipc/entity";
  import { proseMarkdown } from "../../render/prose";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { ticketHash } from "../route";
  import { commentCount, firstName, mentionCount, since, stamp, type Summary } from "./conversation";
  import { blockers, factsLine, keyOf, weekLine, type WeekReading } from "./job";

  let {
    detail,
    others = [],
    pinned,
    action,
    onaction,
    week,
    conversation = null,
    conversationFailure = null,
    lastVisit = null,
    now = new Date(),
  }: {
    detail: EntityDetail;
    /** The other sources' tickets under the same key (#1476); none draws no source. */
    others?: EntityDetail[];
    /** Whether the ticket is the chosen job: the yellow, else the sheet. */
    pinned: boolean;
    /** The foot's one button, or `null` where the card offers none. */
    action: { label: string } | null;
    onaction: () => void;
    week: WeekReading | null;
    /** The thread's summary, `null` while its read is out or after it failed. */
    conversation?: Summary | null;
    /** What the thread's read failed with, if it did. */
    conversationFailure?: string | null;
    /** When the reader last opened this job, for "new since 09:00". */
    lastVisit?: string | null;
    now?: Date;
  } = $props();

  const key = $derived(keyOf(detail.row.entity_id));
  const facts = $derived(factsLine(detail));
  const blockedBy = $derived(blockers(detail));
  /** The tallest weekday, never under 4.5 hours, so a short day stays short. */
  const peak = $derived(Math.max(4.5 * 3600, ...(week?.days ?? []).map((day) => day.logged + day.unlogged)));
  const conversationHref = $derived(ticketHash(detail.row.entity_id, "conversation"));
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
    <h1 class="tf-jobtitle">{detail.row.title}</h1>
  </div>
  <div class="tf-jobrow">
    <span class="tf-chip">{detail.status ?? "No status"}</span>
    {#if facts}<span class="tf-jobdim">{facts}</span>{/if}
    {#each blockedBy as blocker (blocker)}
      <span class="tf-chip tf-err">Blocked by {blocker}</span>
    {/each}
  </div>
  <section>
    <h2>What it asks for</h2>
    {#if detail.description !== null && detail.description.trim() !== ""}
      {@const markdown = proseMarkdown(detail.description, detail.markup)}
      {#if markdown !== null}<div class="tf-asks"><TfMarkdown text={markdown} /></div>{:else}<p class="tf-asks">{detail.description}</p>{/if}
    {:else}
      <p>The ticket has no description.</p>
    {/if}
  </section>
  <section>
    <h2>Conversation</h2>
    {#if conversationFailure !== null}
      <p class="tf-small">Could not read the conversation: {conversationFailure}</p>
    {:else if conversation === null}
      <p class="tf-small">…</p>
    {:else if conversation.comments === 0}
      <p class="tf-small">No comments yet.</p>
      <a class="tf-btn tf-sm tf-cbtn" href={conversationHref}>Comment</a>
    {:else}
      <p class="tf-small">
        {commentCount(conversation.comments)}{#if conversation.fresh > 0 && lastVisit !== null}, <b
            >{conversation.fresh} new since {since(lastVisit, now)}</b
          >{/if}{#if conversation.mentions > 0}, {mentionCount(conversation.mentions)}{/if}
      </p>
      {#if conversation.latest}
        <div class="tf-last">
          <b>{conversation.latest.mine ? "You" : firstName(conversation.latest.author)}</b>{conversation.latest.at
            ? `, ${stamp(conversation.latest.at, now)}`
            : ""}: {conversation.latest.body}
        </div>
      {/if}
      <a class="tf-btn tf-sm tf-cbtn" href={conversationHref}>Read the conversation</a>
    {/if}
  </section>
  <section>
    <h2><a href="#/time/week">Time this week</a></h2>
    <div class="tf-jobdim tf-small">{week ? weekLine(week) : "…"}</div>
    {#if week}
      <svg class="tf-week" viewBox="0 0 5 100" preserveAspectRatio="none" aria-hidden="true">
        {#each week.days as day, index (index)}
          {@const total = ((day.logged + day.unlogged) / peak) * 100}
          {@const logged = (day.logged / peak) * 100}
          <g class={day.today ? "tf-today" : undefined}>
            <rect class="tf-week-unlogged" x={index + 0.08} width="0.84" y={100 - Math.max(4, total)} height={Math.max(4, total)} />
            <rect class="tf-week-logged" x={index + 0.08} width="0.84" y={100 - logged} height={logged} />
          </g>
        {/each}
      </svg>
      <div class="tf-weekends tf-jobdim"><span>Mon</span><span>Fri</span></div>
    {/if}
  </section>
  {#if action !== null}
    <div class="tf-go">
      <button type="button" class="tf-btn tf-primary tf-lg" onclick={onaction}>{action.label}</button>
    </div>
  {/if}
</article>
