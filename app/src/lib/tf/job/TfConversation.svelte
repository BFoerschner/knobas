<!--
  The job's Conversation tab (#819, R-5.2 to R-5.4): `#/job/<key>/conversation`.

  The whole thread, oldest first, under the filter bar: words, a chip per
  person who commented, and *Mentions me* and *New since …*. The thread is the
  ticket's comments and nothing else (#2163). Unfiltered, the old part folds behind *Show N earlier
  entries* and a blue rule marks where the reader's last visit ends. New
  comments are edged blue, mentions edged yellow with a chip and the `@name` in
  blue. The rules are `conversation.ts`'s; this draws them.

  The comment box at the end queues a `Comment` write (BUILT): the box clears
  on queue, and the comment is drawn at the end of the thread at once, muted
  with its echo (#1424, #2103), *queued* and then *sent*, until a read of the
  thread (the refresh after the write, or a run's end) holds its words.
  *Edit and resend* on a held or refused one puts its words back in the box.
  A comment the source's credential cannot make (`refused`, #2052) draws
  *Comment* disabled with the reason under the box, and queues nothing.
-->
<script lang="ts">
  import type { Thread } from "../../ipc/entity";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import { refusedAttrs } from "../../shell/refused-writes";
  import { takeAgain, type Echo } from "../queued.svelte";
  import { initials } from "../tickets/tickets";
  import { MENTIONS, NEW, drawThread, firstName, mentionRuns, since, stamp } from "./conversation";

  let {
    jobKey,
    entityId,
    queued = [],
    thread,
    lastVisit,
    failure,
    now,
    refused = null,
    oncomment,
  }: {
    jobKey: string;
    /** The ticket's entity id, which *Edit and resend* names. */
    entityId: string;
    /** The comments queued here that the thread does not hold yet (#1424). */
    queued?: Pick<Echo, "text" | "state">[];
    /** `null` while the read is out. */
    thread: Thread | null;
    /** When the reader last opened this job before now; `null` for never. */
    lastVisit: string | null;
    /** What the thread's read failed with, if it did. */
    failure: string | null;
    now: Date;
    /** Why the source's credential cannot comment here (#2052), or `null`. */
    refused?: string | null;
    /** Queue a comment; resolves whether it was queued, so the box keeps a refused text. */
    oncomment: (body: string) => Promise<boolean>;
  } = $props();

  // The tab is drawn afresh for each job it opens on, so its filter is read
  // once: the job's own, kept for the session (R-2.6).
  // svelte-ignore state_referenced_locally
  const filter = sessionFilter(`convo-${jobKey}`);

  let expanded = $state(false);
  let draft = $state("");
  let sending = $state(false);
  let box = $state<HTMLTextAreaElement | null>(null);

  // *Edit and resend* on a held or refused comment: its words back in the box.
  $effect(() => {
    const again = takeAgain(entityId, "comment");
    if (again === null) return;
    draft = again;
    box?.focus();
  });

  const people = $derived([...new Set((thread?.comments ?? []).map((c) => c.author).filter((a): a is string => a !== null))]);
  const config = $derived<FilterConfig>({
    id: `convo-${jobKey}`,
    placeholder: "Filter comments",
    noun: "comments",
    one: "comment",
    chips: [
      { key: "who", label: "People", options: people.map((person) => ({ value: person, label: firstName(person) })) },
      {
        key: "only",
        label: "Show",
        options: [
          { value: MENTIONS, label: "Mentions me" },
          ...(lastVisit === null ? [] : [{ value: NEW, label: `New since ${since(lastVisit, now)}` }]),
        ],
      },
    ],
  });
  /** The reader as the thread names them: the author of their own comments, else their account. */
  const myName = $derived(thread?.comments.find((c) => c.mine)?.author ?? thread?.me[0] ?? null);
  const drawn = $derived(thread === null ? null : drawThread(thread, lastVisit, filter, expanded));

  async function send() {
    const body = draft.trim();
    if (body === "" || sending || refused !== null) return;
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
      {@const comment = entry.comment}
      {#if index === drawn.newAt && lastVisit !== null}
        <div class="tf-newline">New since your last visit, {since(lastVisit, now)}</div>
      {/if}
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
    {/each}
    {#if drawn.entries.length === 0 && queued.length === 0}
      <div class="tf-empty">{thread.comments.length === 0 ? "No comments yet." : "No comments match."}</div>
    {/if}
  {/if}
  {#each queued as echo, n (n)}
    <article class="tf-msg tf-queued">
      <span class="tf-av tf-me" aria-hidden="true">{initials(myName)}</span>
      <div class="tf-msg-body">
        <div class="tf-who"><b>You</b><span class="tf-chip tf-info">{echo.state}</span></div>
        <p>{echo.text}</p>
      </div>
    </article>
  {/each}
  <div class="tf-compose">
    <textarea bind:this={box} bind:value={draft} placeholder="Comment on {jobKey}" aria-label="Comment on {jobKey}"></textarea>
    <div class="tf-acts">
      {#if refused !== null}<span class="tf-dim tf-small">{refused}</span>{/if}
      <button type="button" class="tf-btn tf-primary" {...refusedAttrs(refused)} disabled={draft.trim() === "" || sending} onclick={() => void send()}>Comment</button>
    </div>
  </div>
</div>
