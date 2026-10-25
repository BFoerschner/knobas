<!--
  The job card (#816, R-4.2): the ticket you are on, on the yellow, in a
  fixed 320px column left of the work.

  Top to bottom: *All tickets* (`Esc`), the key at 64px in Archivo width 62,
  the source when another source holds the same key (#1476), with a link to
  each other one, the summary, the status select, the facts line and the blocked chip, the
  clock, *What it asks for* and *Time this week*. The conversation summary is
  #819's.

  **The status select offers only where the ticket can go now** (BUILT
  `reachable_transitions`). A pick queues the move, and the select shows the
  picked status muted with its echo beside it (#1424, #2103), *queued* and
  then *sent*, until a read of the ticket (the refresh after the write, or a
  run's end) has its status moved or the queue holds or refuses the move; then it shows
  where the source last said. *Edit and resend* on a held move focuses it.
  A workflow that cannot be read leaves the select showing the current status
  and nothing else. A move the source's credential cannot make (`refused`,
  #2052) draws the select the same way, `aria-disabled` with the reason as
  its title, so nothing can be picked.
-->
<script lang="ts">
  import type { EntityDetail } from "../../ipc/entity";
  import type { Timer } from "../../shell/timer.svelte";
  import { clockReading } from "../../time/day";
  import { refusedAttrs } from "../../shell/refused-writes";
  import { takeAgain, type Echo } from "../queued.svelte";
  import { jobHash } from "../route";
  import { commentCount, firstName, mentionCount, since, stamp, type Summary } from "./conversation";
  import { blockers, factsLine, keyOf, weekLine, type WeekReading } from "./job";

  let {
    detail,
    others = [],
    offered,
    week,
    conversation = null,
    conversationFailure = null,
    lastVisit = null,
    now = new Date(),
    timer,
    queued = null,
    queuedState = "queued",
    refused = null,
    onmove,
    onstop,
    onstart,
  }: {
    detail: EntityDetail;
    /** The other sources' tickets under the same key (#1476); none draws no source. */
    others?: EntityDetail[];
    /** Where the ticket can move now, or `null` while the workflow read is out. */
    offered: string[] | null;
    /** Where that move's write is: *queued* while open, *sent* once landed (#2103). */
    queuedState?: Echo["state"];
    week: WeekReading | null;
    /** The thread's summary, `null` while its read is out or after it failed. */
    conversation?: Summary | null;
    /** What the thread's read failed with, if it did. */
    conversationFailure?: string | null;
    /** When the reader last opened this job, for "new since 09:00". */
    lastVisit?: string | null;
    now?: Date;
    timer: Pick<Timer, "current" | "elapsed">;
    /** The status a queued move picked, drawn muted until the mirror moves (#1424). */
    queued?: string | null;
    /** Why the source's credential cannot move this ticket (#2052), or `null`. */
    refused?: string | null;
    onmove: (status: string) => void;
    onstop: () => void;
    onstart: () => void;
  } = $props();

  const key = $derived(keyOf(detail.row.entity_id));
  const facts = $derived(factsLine(detail));
  const blockedBy = $derived(blockers(detail));
  const running = $derived(
    timer.current?.target.kind === "entity" && timer.current.target.entity_id === detail.row.entity_id
      ? timer.current
      : null,
  );
  const options = $derived(refused !== null ? [] : (offered ?? []).filter((status) => status !== detail.status));
  /** The tallest weekday, never under 4.5 hours, so a short day stays short. */
  const peak = $derived(Math.max(4.5 * 3600, ...(week?.days ?? []).map((day) => day.logged + day.unlogged)));

  /** What the select shows: a queued move's status, else the mirrored one. */
  const shown = $derived(queued ?? detail.status);

  function pick(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    const status = select.value;
    select.value = shown ?? "";
    if (status && status !== shown && refused === null) onmove(status);
  }

  let select = $state<HTMLSelectElement | null>(null);
  // *Edit and resend* on a held or refused move: the pick again.
  $effect(() => {
    if (takeAgain(detail.row.entity_id, "status") !== null) select?.focus();
  });
</script>

<aside class="tf-jobcard tf-card" aria-label="Job card">
  <a class="tf-back" href="#/tickets">All tickets <kbd>Esc</kbd></a>
  <div>
    <div class="tf-jobkey">{key}</div>
    {#if others.length > 0}
      <p class="tf-jobsrc">
        In {detail.source.display_name} · also in {#each others as other, at (other.row.entity_id)}{#if at > 0},
          {/if}<a href={jobHash(other.row.entity_id)}>{other.source.display_name}</a>{/each}
      </p>
    {/if}
    <h1 class="tf-jobtitle">{detail.row.title}</h1>
  </div>
  <div class="tf-jobrow">
    <select
      bind:this={select}
      aria-label="Status"
      class:tf-queued={queued !== null}
      value={shown ?? ""}
      onchange={pick}
      {...refusedAttrs(refused)}
      disabled={options.length === 0 && queued === null && refused === null}
    >
      {#if detail.status === null}
        <option value="" disabled>No status</option>
      {:else}
        <option value={detail.status}>{detail.status}</option>
      {/if}
      {#if queued !== null && queued !== detail.status && !options.includes(queued)}
        <option value={queued}>{queued}</option>
      {/if}
      {#each options as status (status)}
        <option value={status}>{status}</option>
      {/each}
    </select>
    {#if queued !== null}<span class="tf-chip tf-info tf-queued-mark">{queuedState}</span>{/if}
    {#if facts}<span class="tf-jobdim">{facts}</span>{/if}
    {#each blockedBy as blocker (blocker)}
      <span class="tf-chip tf-err">Blocked by {blocker}</span>
    {/each}
  </div>
  <div class="tf-clock">
    {#if running}
      <span class="tf-clock-t">{timer.elapsed ?? "0:00"}</span>
      <span class="tf-jobdim tf-small">since {clockReading(running.started_at)}</span>
      <button class="tf-btn" onclick={onstop}>Stop and log</button>
    {:else}
      <span class="tf-clock-t">0:00</span>
      <button class="tf-btn" onclick={onstart}>Start timer</button>
    {/if}
  </div>
  <section>
    <h2>What it asks for</h2>
    {#if detail.body_text.trim() !== ""}
      <p class="tf-asks">{detail.body_text}</p>
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
      <a class="tf-btn tf-sm tf-cbtn" href="{jobHash(detail.row.entity_id)}/conversation">Comment</a>
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
      <a class="tf-btn tf-sm tf-cbtn" href="{jobHash(detail.row.entity_id)}/conversation">Read the conversation</a>
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
</aside>
