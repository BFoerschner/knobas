<!--
  The job's Conversation tab (#819, R-5.2 to R-5.4): `#/job/<key>/conversation`.

  The whole thread, oldest first, under the filter bar: words, a chip per
  person who commented, and *Mentions me*, *New since …* and *Include changes*
  (on by default). Unfiltered, the old part folds behind *Show N earlier
  entries* and a blue rule marks where the reader's last visit ends. New
  comments are edged blue, mentions edged yellow with a chip and the `@name` in
  blue. The rules are `conversation.ts`'s; this draws them.

  The comment box at the end queues a `Comment` write (BUILT): the box clears
  on queue, and the comment joins the thread when a sync mirrors it.
-->
<script lang="ts">
  import type { Thread } from "../../ipc/entity";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import { initials } from "../tickets/tickets";
  import { CHANGES, MENTIONS, NEW, changeLine, drawThread, firstName, mentionRuns, since, stamp } from "./conversation";

  let {
    jobKey,
    thread,
    lastVisit,
    failure,
    now,
    oncomment,
  }: {
    jobKey: string;
    /** `null` while the read is out. */
    thread: Thread | null;
    /** When the reader last opened this job before now; `null` for never. */
    lastVisit: string | null;
    /** What the thread's read failed with, if it did. */
    failure: string | null;
    now: Date;
    /** Queue a comment; resolves whether it was queued, so the box keeps a refused text. */
    oncomment: (body: string) => Promise<boolean>;
  } = $props();

  // The tab is drawn afresh for each job it opens on, so its filter is read
  // once: the job's own, kept for the session (R-2.6).
  // svelte-ignore state_referenced_locally
  const filter = sessionFilter(`convo-${jobKey}`);
  // *Include changes* starts on (R-5.2), once per session.
  if (filter.chips["only"] === undefined) filter.chips["only"] = [CHANGES];

  let expanded = $state(false);
  let draft = $state("");
  let sending = $state(false);

  const people = $derived([...new Set((thread?.comments ?? []).map((c) => c.author).filter((a): a is string => a !== null))]);
  const config = $derived<FilterConfig>({
    id: `convo-${jobKey}`,
    placeholder: "Filter comments",
    noun: "comments",
    chips: [
      { key: "who", label: "People", options: people.map((person) => ({ value: person, label: firstName(person) })) },
      {
        key: "only",
        label: "Show",
        options: [
          { value: MENTIONS, label: "Mentions me" },
          ...(lastVisit === null ? [] : [{ value: NEW, label: `New since ${since(lastVisit, now)}` }]),
          { value: CHANGES, label: "Include changes" },
        ],
      },
    ],
  });
  const drawn = $derived(thread === null ? null : drawThread(thread, lastVisit, filter, expanded));

  async function send() {
    const body = draft.trim();
    if (body === "" || sending) return;
    sending = true;
    try {
      if (await oncomment(body)) draft = "";
    } finally {
      sending = false;
    }
  }
</script>

<div class="tf-convo-bar">
  <FilterBar {config} {filter} shown={drawn?.shown ?? null} total={thread?.comments.length ?? null} />
</div>
<div class="tf-convo">
  {#if failure !== null}
    <p class="tf-dim">Could not read the conversation: {failure}</p>
  {:else if thread === null || drawn === null}
    <p class="tf-dim">Reading the conversation…</p>
  {:else}
    {#if drawn.folded > 0}
      <button type="button" class="tf-fold" onclick={() => (expanded = true)}
        >Show {drawn.folded} earlier entries{drawn.foldedFrom ? `, from ${drawn.foldedFrom}` : ""}</button
      >
    {/if}
    {#each drawn.entries as entry, index (index)}
      {#if index === drawn.newAt && lastVisit !== null}
        <div class="tf-newline">New since your last visit, {since(lastVisit, now)}</div>
      {/if}
      {#if entry.kind === "change"}
        <div class="tf-msg tf-event"><span></span><span>{changeLine(entry.change)}, {stamp(entry.change.at, now)}</span></div>
      {:else}
        {@const comment = entry.comment}
        <article class="tf-msg" class:tf-new={entry.fresh} class:tf-mention={comment.mentions_me}>
          <span class="tf-av" class:tf-me={comment.mine} aria-hidden="true">{initials(comment.author)}</span>
          <div class="tf-msg-body">
            <div class="tf-who">
              <b>{comment.mine ? "You" : firstName(comment.author)}</b>{stamp(comment.at, now)}
              {#if comment.mentions_me}<span class="tf-chip tf-chip-mention">mentions you</span>{/if}
            </div>
            <p>
              {#each mentionRuns(comment.body, thread.me) as run, at (at)}{#if run.at}<span class="tf-at">{run.text}</span>{:else}<Marked text={run.text} typed={filter.text} />{/if}{/each}
            </p>
          </div>
        </article>
      {/if}
    {/each}
    {#if drawn.entries.length === 0}
      <div class="tf-empty">{thread.comments.length === 0 ? "No comments yet." : "No comments match."}</div>
    {/if}
  {/if}
  <div class="tf-compose">
    <textarea bind:value={draft} placeholder="Comment on {jobKey}" aria-label="Comment on {jobKey}"></textarea>
    <div class="tf-acts">
      <button type="button" class="tf-btn tf-primary" disabled={draft.trim() === "" || sending} onclick={() => void send()}>Comment</button>
    </div>
  </div>
</div>
